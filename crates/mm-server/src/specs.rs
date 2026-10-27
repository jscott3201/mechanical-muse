//! Our validated specs (`specs/<name>/`), loaded once at startup and kept in memory. Names are
//! sorted so every listing is deterministic.

use std::collections::BTreeMap;
use std::path::Path;

use mm_bridge::{ReplayLimits, ReplayReport, VectorFile, replay};
use mm_graph::{BlockGraph, EmitOptions, Endpoint, Palette, PortKind, emit_cxf, parse_edge};
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
    r#type: mm_graph::SignalType,
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
    /// The task as the model receives it (the same `task.json`).
    pub task: mm_agent::TaskSpec,
    /// The hand-built reference graph; `None` for an engineer's own sequence from the workbench,
    /// which only Muse Spark's verified drafts implement.
    pub graph: Option<BlockGraph>,
    pub vectors: VectorFile,
    pub view: Option<GraphView>,
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
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    tracing::warn!(error = %e, "skipping an unreadable entry in the specs directory");
                    continue;
                }
            };
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

impl Spec {
    /// Emits the reference graph and replays the vectors (fresh engine per scenario). Blocking:
    /// call it from the blocking pool.
    pub fn replay(&self, palette: &Palette) -> Result<(Vec<u8>, ReplayReport), String> {
        let graph = self
            .graph
            .as_ref()
            .ok_or("this sequence has no reference graph; run a verified draft")?;
        self.replay_graph(graph, palette)
    }

    /// A spec from an engineer's reviewed task and accepted scenarios (no reference graph).
    pub fn from_workbench(
        name: &str,
        title: &str,
        about: &str,
        task: mm_agent::TaskSpec,
        vectors: VectorFile,
    ) -> Self {
        let point = |p: &mm_agent::PointSpec| TaskPoint {
            name: p.name.clone(),
            r#type: Some(signal(p.ty.port_kind())),
            unit: p.unit.clone(),
            meaning: p.meaning.clone(),
            value: None,
        };
        let detail = SpecDetail {
            summary: SpecSummary {
                name: name.to_owned(),
                title: title.to_owned(),
                inputs: task.inputs.iter().map(point).collect(),
                outputs: task.outputs.iter().map(point).collect(),
                scenarios: u32::try_from(vectors.scenarios.len()).unwrap_or(u32::MAX),
            },
            about: about.to_owned(),
            description: task.description.clone(),
            params: task
                .params
                .iter()
                .map(|p| TaskPoint {
                    name: p.name.clone(),
                    r#type: None,
                    unit: p.unit.clone(),
                    meaning: p.meaning.clone(),
                    value: match p.value {
                        mm_graph::ParamValue::Number(n) => Some(n),
                        mm_graph::ParamValue::Bool(b) => Some(if b { 1.0 } else { 0.0 }),
                        mm_graph::ParamValue::Text(_) => None,
                    },
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
            reference: false,
        };
        Self {
            detail,
            task,
            graph: None,
            vectors,
            view: None,
        }
    }

    /// [`Spec::replay`] for another graph of this spec (a model's draft).
    pub fn replay_graph(
        &self,
        graph: &BlockGraph,
        palette: &Palette,
    ) -> Result<(Vec<u8>, ReplayReport), String> {
        let cxf = emit_cxf(graph, palette, &EmitOptions::default())
            .map_err(|e| format!("emit failed: {e}"))?;
        let report = replay(&cxf, &self.vectors, &ReplayLimits::default())
            .map_err(|e| format!("replay failed: {e}"))?;
        Ok((cxf, report))
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
    let task: mm_agent::TaskSpec = read(name, dir, "task.json")?;
    let graph: BlockGraph = read(name, dir, "graph.json")?;
    let vectors: VectorFile = read(name, dir, "vectors.json")?;
    let doc = std::fs::read_to_string(dir.join("spec.md")).unwrap_or_default();
    let title = doc
        .lines()
        .find_map(|l| l.strip_prefix("# ").map(str::to_owned))
        .unwrap_or_else(|| name.to_owned());
    let about = first_paragraph(&doc);
    let point = |p: &CardPoint| TaskPoint {
        name: p.name.clone(),
        r#type: Some(signal(p.r#type.port_kind())),
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
        about,
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
        reference: true,
    };
    let view = graph_view(&graph, palette).map_err(|problem| SpecError::Graph {
        spec: name.to_owned(),
        problem,
    })?;
    Ok(Spec {
        detail,
        task,
        graph: Some(graph),
        vectors,
        view: Some(view),
    })
}

/// A task's content as a key, without its name (`task:<hex>`, FNV-1a 64 over its JSON): the
/// same task accepted under another name has the same key.
pub fn task_key(task: &mm_agent::TaskSpec) -> String {
    let unnamed = mm_agent::TaskSpec {
        name: String::new(),
        ..task.clone()
    };
    let text = serde_json::to_string(&unnamed).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("task:{hash:016x}")
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

/// The first paragraph after the title, joined into one line.
fn first_paragraph(doc: &str) -> String {
    doc.lines()
        .skip_while(|l| !l.starts_with("# "))
        .skip(1)
        .skip_while(|l| l.trim().is_empty())
        .take_while(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ")
}
