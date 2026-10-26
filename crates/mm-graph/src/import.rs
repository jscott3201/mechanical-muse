//! CXF to block graph, for library-dialect documents (flat, one root block). Used to turn library
//! rules into expected graphs and few-shot exemplars, and to prove the emitter round-trips.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::graph::{Block, BlockGraph, Input, Output, Param, ParamValue, SignalType};

const CDL_IRI_PREFIX: &str = "Buildings.Controls.OBC.CDL.";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("CXF is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("CXF has no @graph array or no S231:Block root")]
    NoRoot,
    #[error("node {0} is referenced but missing or malformed")]
    MissingNode(String),
    #[error("{0} is not inside the root block")]
    OutsideRoot(String),
    #[error("{0} has an unsupported type")]
    Unsupported(String),
}

/// A graph plus the identity it was imported with, so it can be emitted back to the same IRIs.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub graph: BlockGraph,
    pub namespace: String,
    pub root_label: String,
}

pub fn cxf_to_graph(bytes: &[u8]) -> Result<Imported, ImportError> {
    let doc: Value = serde_json::from_slice(bytes)?;
    let nodes = doc
        .get("@graph")
        .and_then(Value::as_array)
        .ok_or(ImportError::NoRoot)?;
    let root = nodes
        .iter()
        .find(|n| n.get("@type").and_then(Value::as_str) == Some("S231:Block"))
        .ok_or(ImportError::NoRoot)?;
    let root_id = str_field(root, "@id")?;
    let root_label = str_field(root, "S231:label")?.to_owned();
    let namespace = doc
        .pointer("/@context/base")
        .and_then(Value::as_str)
        .ok_or(ImportError::NoRoot)?
        .to_owned();
    let by_id: BTreeMap<&str, &Value> = nodes
        .iter()
        .filter_map(|n| Some((n.get("@id")?.as_str()?, n)))
        .collect();
    let prefix = format!("{root_id}.");
    let local = |id: &str| -> Result<String, ImportError> {
        id.strip_prefix(&prefix)
            .map(str::to_owned)
            .ok_or_else(|| ImportError::OutsideRoot(id.to_owned()))
    };
    let node = |id: &str| {
        by_id
            .get(id)
            .copied()
            .ok_or_else(|| ImportError::MissingNode(id.to_owned()))
    };

    let mut inputs = Vec::new();
    for id in refs(root, "S231:hasInput") {
        let ty = signal_type(str_field(node(id)?, "@type")?)?;
        inputs.push(Input {
            name: local(id)?,
            ty,
            unit: String::new(),
            description: String::new(),
        });
    }
    let mut outputs = Vec::new();
    for id in refs(root, "S231:hasOutput") {
        let ty = signal_type(str_field(node(id)?, "@type")?)?;
        outputs.push(Output {
            name: local(id)?,
            ty,
            description: String::new(),
        });
    }
    let mut blocks = Vec::new();
    for id in refs(root, "S231:containsBlock") {
        let block = node(id)?;
        let block_id = local(id)?;
        let class = str_field(block, "@type")?
            .split_once(CDL_IRI_PREFIX)
            .map(|(_, class)| class.to_owned())
            .ok_or_else(|| ImportError::Unsupported(id.to_owned()))?;
        let mut params = Vec::new();
        for pid in refs(block, "S231:hasParameter") {
            let name = local(pid)?
                .strip_prefix(&format!("{block_id}."))
                .map(str::to_owned)
                .ok_or_else(|| ImportError::OutsideRoot(pid.to_owned()))?;
            params.push(Param {
                name,
                value: param_value(pid, node(pid)?)?,
            });
        }
        blocks.push(Block {
            id: block_id,
            class,
            params,
        });
    }

    // Dataflow order: edges from boundary inputs first (declared order), then from block outputs
    // (block order). The sort is stable, so each source keeps its fan-out order.
    let input_rank: BTreeMap<&str, usize> = inputs
        .iter()
        .enumerate()
        .map(|(i, x)| (x.name.as_str(), i))
        .collect();
    let block_rank: BTreeMap<&str, usize> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id.as_str(), i))
        .collect();
    let mut edges: Vec<((usize, usize), String)> = Vec::new();
    for n in nodes {
        let Some(src_id) = n.get("@id").and_then(Value::as_str) else {
            continue;
        };
        for dst_id in refs(n, "S231:isConnectedTo") {
            let src = local(src_id)?;
            let dst = local(dst_id)?;
            let rank = if let Some(r) = input_rank.get(src.as_str()) {
                (0, *r)
            } else {
                let block = src.split('.').next().unwrap_or_default();
                let r = block_rank
                    .get(block)
                    .ok_or_else(|| ImportError::MissingNode(src_id.to_owned()))?;
                (1, *r)
            };
            edges.push((rank, format!("{src} -> {dst}")));
        }
    }
    edges.sort_by_key(|(rank, _)| *rank);

    Ok(Imported {
        graph: BlockGraph {
            name: root_label.clone(),
            inputs,
            outputs,
            blocks,
            connections: edges.into_iter().map(|(_, e)| e).collect(),
        },
        namespace,
        root_label,
    })
}

fn str_field<'a>(node: &'a Value, key: &str) -> Result<&'a str, ImportError> {
    node.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ImportError::MissingNode(format!("{key} on {node}")))
}

/// `@id`s under `key`, which may hold one reference object or an array of them.
fn refs<'a>(node: &'a Value, key: &str) -> Vec<&'a str> {
    match node.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|r| r.get("@id")?.as_str())
            .collect(),
        Some(single) => single
            .get("@id")
            .and_then(Value::as_str)
            .into_iter()
            .collect(),
        None => Vec::new(),
    }
}

fn signal_type(port_type: &str) -> Result<SignalType, ImportError> {
    match port_type {
        "S231:RealInput" | "S231:RealOutput" => Ok(SignalType::Real),
        "S231:BooleanInput" | "S231:BooleanOutput" => Ok(SignalType::Bool),
        "S231:IntegerInput" | "S231:IntegerOutput" => Ok(SignalType::Int),
        other => Err(ImportError::Unsupported(other.to_owned())),
    }
}

fn param_value(id: &str, node: &Value) -> Result<ParamValue, ImportError> {
    let bad = || ImportError::Unsupported(id.to_owned());
    let value = node.get("S231:value").ok_or_else(bad)?;
    let text = value
        .get("@value")
        .and_then(Value::as_str)
        .ok_or_else(bad)?;
    let xsd = value
        .get("@type")
        .and_then(Value::as_str)
        .and_then(|t| t.strip_prefix(XSD))
        .ok_or_else(bad)?;
    match xsd {
        "double" => text.parse().map(ParamValue::Number).map_err(|_| bad()),
        "integer" => text
            .parse::<i64>()
            .map(|n| ParamValue::Number(n as f64))
            .map_err(|_| bad()),
        "boolean" => match text {
            "true" => Ok(ParamValue::Bool(true)),
            "false" => Ok(ParamValue::Bool(false)),
            _ => Err(bad()),
        },
        _ => Err(bad()),
    }
}
