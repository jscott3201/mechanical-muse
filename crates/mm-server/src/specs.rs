//! Our validated specs (`specs/<name>/`), loaded once at startup and kept in memory. Names are
//! sorted so every listing is deterministic.

use std::collections::BTreeMap;
use std::path::Path;

use mm_bridge::VectorFile;
use mm_graph::{BlockGraph, Endpoint, Palette, PortKind, parse_edge};
use serde::Deserialize;

use crate::dto::{
    BlockView, BoundaryView, ConnectionView, EndView, GraphView, ParamView, PortView, ScenarioInfo,
    Signal, SpecDetail, SpecSummary, TaskPoint,
};

#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("{spec}: cannot read {file}: {source}")]
    Read {
        spec: String,
        file: &'static str,
        source: std::io::Error,
    },
    #[error("{spec}: {file} is not valid: {source}")]
    Parse {
        spec: String,
        file: &'static str,
        source: serde_json::Error,
    },
    #[error("{spec}: {problem}")]
    Graph { spec: String, problem: String },
}

#[derive(Debug, Clone, Deserialize)]
struct TaskCard {
    description: String,
    inputs: Vec<CardPoint>,
    outputs: Vec<CardPoint>,
    params: Vec<CardParam>,
}

#[derive(Debug, Clone, Deserialize)]
struct CardPoint {
    name: String,
    r#type: String,
    unit: String,
    meaning: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CardParam {
    name: String,
    value: f64,
    unit: String,
    meaning: String,
}

/// One spec, with its graph already resolved against the catalog.
#[derive(Debug, Clone)]
pub struct Spec {
    pub detail: SpecDetail,
    pub graph: BlockGraph,
    pub vectors: VectorFile,
    pub view: GraphView,
}

#[derive(Debug, Clone, Default)]
pub struct SpecStore {
    specs: BTreeMap<String, Spec>,
}

impl SpecStore {
    /// Loads every folder under `dir` that has a `task.json`.
    pub fn load(dir: &Path, palette: &Palette) -> Result<Self, SpecError> {
        let mut specs = BTreeMap::new();
        let entries = std::fs::read_dir(dir).map_err(|source| SpecError::Read {
            spec: dir.display().to_string(),
            file: "specs directory",
            source,
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.join("task.json").is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let spec = load_one(&name, &path, palette)?;
            specs.insert(name, spec);
        }
        Ok(Self { specs })
    }

    pub fn get(&self, name: &str) -> Option<&Spec> {
        self.specs.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Spec> {
        self.specs.values()
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }
}

fn read<T: for<'de> Deserialize<'de>>(
    spec: &str,
    dir: &Path,
    file: &'static str,
) -> Result<T, SpecError> {
    let bytes = std::fs::read(dir.join(file)).map_err(|source| SpecError::Read {
        spec: spec.to_owned(),
        file,
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| SpecError::Parse {
        spec: spec.to_owned(),
        file,
        source,
    })
}

fn load_one(name: &str, dir: &Path, palette: &Palette) -> Result<Spec, SpecError> {
    let card: TaskCard = read(name, dir, "task.json")?;
    let graph: BlockGraph = read(name, dir, "graph.json")?;
    let vectors: VectorFile = read(name, dir, "vectors.json")?;
    let title = std::fs::read_to_string(dir.join("spec.md"))
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("# ").map(str::to_owned))
        })
        .unwrap_or_else(|| name.to_owned());
    let point = |p: &CardPoint| TaskPoint {
        name: p.name.clone(),
        r#type: Some(p.r#type.clone()),
        unit: p.unit.clone(),
        meaning: p.meaning.clone(),
        value: None,
    };
    let summary = SpecSummary {
        name: name.to_owned(),
        title,
        inputs: card.inputs.iter().map(point).collect(),
        outputs: card.outputs.iter().map(point).collect(),
        scenarios: u32::try_from(vectors.scenarios.len()).unwrap_or(u32::MAX),
    };
    let detail = SpecDetail {
        summary,
        description: card.description,
        params: card
            .params
            .iter()
            .map(|p| TaskPoint {
                name: p.name.clone(),
                r#type: None,
                unit: p.unit.clone(),
                meaning: p.meaning.clone(),
                value: Some(p.value),
            })
            .collect(),
        scenarios: vectors
            .scenarios
            .iter()
            .map(|s| ScenarioInfo {
                name: s.name.clone(),
                description: s.description.clone(),
            })
            .collect(),
        step_s: vectors.clock.step_s,
    };
    let view = graph_view(&graph, palette).map_err(|problem| SpecError::Graph {
        spec: name.to_owned(),
        problem,
    })?;
    Ok(Spec {
        detail,
        graph,
        vectors,
        view,
    })
}

fn signal(kind: PortKind) -> Signal {
    match kind {
        PortKind::Real => Signal::Real,
        PortKind::Integer => Signal::Integer,
        PortKind::Boolean => Signal::Boolean,
    }
}

/// Resolves every block's ports from the catalog and types every connection by its source.
pub fn graph_view(graph: &BlockGraph, palette: &Palette) -> Result<GraphView, String> {
    let ports = |list: &[mm_graph::PortSpec]| {
        list.iter()
            .map(|p| PortView {
                name: p.name.clone(),
                signal: signal(p.kind),
            })
            .collect::<Vec<_>>()
    };
    let mut blocks = Vec::with_capacity(graph.blocks.len());
    for b in &graph.blocks {
        let class = palette
            .class(&b.class)
            .ok_or_else(|| format!("block {} has unknown class {}", b.id, b.class))?;
        blocks.push(BlockView {
            id: b.id.clone(),
            class: b.class.clone(),
            inputs: ports(&class.inputs),
            outputs: ports(&class.outputs),
            params: b
                .params
                .iter()
                .map(|p| ParamView {
                    name: p.name.clone(),
                    value: serde_json::to_value(&p.value).unwrap_or(serde_json::Value::Null),
                })
                .collect(),
        });
    }
    let source_signal = |end: &Endpoint| -> Result<Signal, String> {
        match end {
            Endpoint::Boundary(name) => graph
                .inputs
                .iter()
                .find(|i| &i.name == name)
                .map(|i| signal(i.ty.port_kind()))
                .ok_or_else(|| format!("{name} is not an input")),
            Endpoint::Port { block, port } => blocks
                .iter()
                .find(|b| &b.id == block)
                .and_then(|b| b.outputs.iter().find(|p| &p.name == port))
                .map(|p| p.signal)
                .ok_or_else(|| format!("{block}.{port} is not an output")),
        }
    };
    let end = |e: &Endpoint| match e {
        Endpoint::Boundary(name) => EndView {
            node: name.clone(),
            port: None,
        },
        Endpoint::Port { block, port } => EndView {
            node: block.clone(),
            port: Some(port.clone()),
        },
    };
    let mut connections = Vec::with_capacity(graph.connections.len());
    for text in &graph.connections {
        let edge = parse_edge(text).ok_or_else(|| format!("bad connection {text}"))?;
        connections.push(ConnectionView {
            signal: source_signal(&edge.from)?,
            from: end(&edge.from),
            to: end(&edge.to),
        });
    }
    let boundary = |name: &str, ty: mm_graph::SignalType, description: &str| BoundaryView {
        name: name.to_owned(),
        signal: signal(ty.port_kind()),
        description: description.to_owned(),
    };
    Ok(GraphView {
        name: graph.name.clone(),
        inputs: graph
            .inputs
            .iter()
            .map(|i| boundary(&i.name, i.ty, &i.description))
            .collect(),
        outputs: graph
            .outputs
            .iter()
            .map(|o| boundary(&o.name, o.ty, &o.description))
            .collect(),
        blocks,
        connections,
    })
}
