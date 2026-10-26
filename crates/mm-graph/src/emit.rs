//! Deterministic CXF JSON-LD emitter. The bytes depend only on the graph, the palette and the
//! options: node order follows the graph, keys follow a fixed order, no map iteration decides
//! anything. Layout follows the library dialect so library rules round-trip byte for byte.

use std::collections::BTreeMap;

use crate::graph::{BlockGraph, ParamValue, parse_edge};
use crate::json::Json;
use crate::number::python_float_repr;
use crate::palette::{Palette, ParamKind, PortKind};
use crate::validate::{Finding, validate};

const S231: &str = "http://data.ashrae.org/S231P#";
const CDL_IRI_PREFIX: &str = "Buildings.Controls.OBC.CDL.";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("graph has {} validation finding(s)", .0.len())]
    Invalid(Vec<Finding>),
    #[error("parameter {0} cannot be written as a CXF literal")]
    Unwritable(String),
}

/// Where the emitted model lives. Defaults: namespace `urn:mechanical-muse:<name>#`, root label
/// `<name>`. Library round-trips pass the library's own values.
#[derive(Debug, Clone, Default)]
pub struct EmitOptions {
    pub namespace: Option<String>,
    pub root_label: Option<String>,
}

/// Validates, then expands the graph to CXF bytes (UTF-8, ASCII-only, trailing newline).
pub fn emit_cxf(
    graph: &BlockGraph,
    palette: &Palette,
    options: &EmitOptions,
) -> Result<Vec<u8>, EmitError> {
    let findings = validate(graph, palette);
    if !findings.is_empty() {
        return Err(EmitError::Invalid(findings));
    }
    let ns = options
        .namespace
        .clone()
        .unwrap_or_else(|| format!("urn:mechanical-muse:{}#", graph.name));
    let label = options.root_label.as_deref().unwrap_or(&graph.name);
    let root = format!("{ns}{label}");
    let at = |local: &str| format!("{root}.{local}");
    let reference = |local: &str| Json::Obj(vec![("@id", Json::str(at(local)))]);

    // Destinations grouped by source, in the order the edges were written.
    let mut fan_out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for text in &graph.connections {
        if let Some(edge) = parse_edge(text) {
            fan_out
                .entry(edge.from.to_string())
                .or_default()
                .push(edge.to.to_string());
        }
    }
    let port = |local: &str, kind: PortKind, input: bool| {
        let (in_type, out_type, data_type) = port_types(kind);
        let mut node = vec![
            ("@id", Json::str(at(local))),
            ("@type", Json::str(if input { in_type } else { out_type })),
            (
                "S231:isOfDataType",
                Json::Obj(vec![("@id", Json::str(data_type))]),
            ),
        ];
        if let Some(dests) = fan_out.get(local) {
            node.push((
                "S231:isConnectedTo",
                one_or_many(dests.iter().map(|d| reference(d)).collect()),
            ));
        }
        Json::Obj(node)
    };

    let mut nodes = vec![Json::Obj(vec![
        ("@id", Json::str(&root)),
        ("@type", Json::str("S231:Block")),
        ("S231:label", Json::str(label)),
        (
            "S231:containsBlock",
            Json::Arr(graph.blocks.iter().map(|b| reference(&b.id)).collect()),
        ),
        (
            "S231:hasInput",
            Json::Arr(graph.inputs.iter().map(|i| reference(&i.name)).collect()),
        ),
        (
            "S231:hasOutput",
            Json::Arr(graph.outputs.iter().map(|o| reference(&o.name)).collect()),
        ),
    ])];

    for block in &graph.blocks {
        // Validation guarantees the class exists; skip defensively rather than panic.
        let Some(spec) = palette.class(&block.class) else {
            continue;
        };
        let id = &block.id;
        let mut node = vec![
            ("@id", Json::str(at(id))),
            (
                "@type",
                Json::str(format!("{ns}{CDL_IRI_PREFIX}{}", block.class)),
            ),
            ("S231:label", Json::str(id)),
        ];
        let refs = |names: Vec<&str>| {
            one_or_many(
                names
                    .into_iter()
                    .map(|n| reference(&format!("{id}.{n}")))
                    .collect(),
            )
        };
        if !block.params.is_empty() {
            node.push((
                "S231:hasParameter",
                refs(block.params.iter().map(|p| p.name.as_str()).collect()),
            ));
        }
        if !spec.inputs.is_empty() {
            node.push((
                "S231:hasInput",
                refs(spec.inputs.iter().map(|p| p.name.as_str()).collect()),
            ));
        }
        if !spec.outputs.is_empty() {
            node.push((
                "S231:hasOutput",
                refs(spec.outputs.iter().map(|p| p.name.as_str()).collect()),
            ));
        }
        nodes.push(Json::Obj(node));

        for param in &block.params {
            let local = format!("{id}.{}", param.name);
            let kind = spec
                .params
                .get(&param.name)
                .map_or(ParamKind::Real, |p| p.kind);
            let (literal, xsd) =
                literal(kind, &param.value).ok_or_else(|| EmitError::Unwritable(local.clone()))?;
            nodes.push(Json::Obj(vec![
                ("@id", Json::str(at(&local))),
                (
                    "S231:value",
                    Json::Obj(vec![
                        ("@value", Json::str(literal)),
                        ("@type", Json::str(format!("{XSD}{xsd}"))),
                    ]),
                ),
            ]));
        }
        for p in &spec.inputs {
            nodes.push(port(&format!("{id}.{}", p.name), p.kind, true));
        }
        for p in &spec.outputs {
            nodes.push(port(&format!("{id}.{}", p.name), p.kind, false));
        }
    }
    for input in &graph.inputs {
        nodes.push(port(&input.name, input.ty.port_kind(), true));
    }
    for output in &graph.outputs {
        nodes.push(port(&output.name, output.ty.port_kind(), false));
    }

    let doc = Json::Obj(vec![
        (
            "@context",
            Json::Obj(vec![("S231", Json::str(S231)), ("base", Json::str(&ns))]),
        ),
        ("@graph", Json::Arr(nodes)),
    ]);
    Ok(doc.to_pretty().into_bytes())
}

/// The library writes a single reference as an object and several as an array.
fn one_or_many(mut refs: Vec<Json>) -> Json {
    if refs.len() == 1
        && let Some(only) = refs.pop()
    {
        return only;
    }
    Json::Arr(refs)
}

fn port_types(kind: PortKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        PortKind::Real => ("S231:RealInput", "S231:RealOutput", "S231:Real"),
        PortKind::Boolean => ("S231:BooleanInput", "S231:BooleanOutput", "S231:Boolean"),
        PortKind::Integer => ("S231:IntegerInput", "S231:IntegerOutput", "S231:Integer"),
    }
}

fn literal(kind: ParamKind, value: &ParamValue) -> Option<(String, &'static str)> {
    match (kind, value) {
        (ParamKind::Boolean, ParamValue::Bool(b)) => Some((b.to_string(), "boolean")),
        (ParamKind::Integer, ParamValue::Number(x)) => {
            // Validation bounds integers well inside i64 and f64's exact range.
            #[allow(clippy::cast_possible_truncation)]
            let n = *x as i64;
            Some((n.to_string(), "integer"))
        }
        (ParamKind::Real, ParamValue::Number(x)) => Some((python_float_repr(*x)?, "double")),
        _ => None,
    }
}
