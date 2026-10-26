//! Palette-level checks that run before the engine sees a draft. Findings are sorted and carry
//! stable codes, so the repair loop can branch on `code` and quote `detail` back to the model.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::graph::{BlockGraph, Endpoint, ParamValue, parse_edge};
use crate::palette::{Palette, ParamKind, PortKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingCode {
    InvalidName,
    DuplicateName,
    UnknownClass,
    ClassNotAuthorable,
    UnknownParam,
    UnsupportedParam,
    DuplicateParam,
    MissingRequiredParam,
    ParamTypeMismatch,
    MalformedEdge,
    UnknownSource,
    UnknownDestination,
    TypeMismatch,
    MultiplyDriven,
    Undriven,
}

impl FindingCode {
    pub fn as_str(self) -> &'static str {
        match self {
            FindingCode::InvalidName => "invalid-name",
            FindingCode::DuplicateName => "duplicate-name",
            FindingCode::UnknownClass => "unknown-class",
            FindingCode::ClassNotAuthorable => "class-not-authorable",
            FindingCode::UnknownParam => "unknown-param",
            FindingCode::UnsupportedParam => "unsupported-param",
            FindingCode::DuplicateParam => "duplicate-param",
            FindingCode::MissingRequiredParam => "missing-required-param",
            FindingCode::ParamTypeMismatch => "param-type-mismatch",
            FindingCode::MalformedEdge => "malformed-edge",
            FindingCode::UnknownSource => "unknown-source",
            FindingCode::UnknownDestination => "unknown-destination",
            FindingCode::TypeMismatch => "type-mismatch",
            FindingCode::MultiplyDriven => "multiply-driven",
            FindingCode::Undriven => "undriven",
        }
    }
}

/// One problem. `subject` names the thing at fault (a name, `block.param`, or an edge).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Finding {
    pub subject: String,
    pub code: FindingCode,
    pub detail: String,
}

/// Checks a graph against the palette. An empty result means the emitter can expand it; it says
/// nothing about whether the logic is right (that is what vectors are for).
pub fn validate(graph: &BlockGraph, palette: &Palette) -> Vec<Finding> {
    let mut v = Validator::default();
    v.names(graph);
    v.blocks(graph, palette);
    v.edges(graph);
    let mut findings = v.findings;
    findings.sort();
    findings.dedup();
    findings
}

#[derive(Default)]
struct Validator {
    findings: Vec<Finding>,
    /// Edge sources: boundary inputs and block outputs, with their kind.
    sources: BTreeMap<Endpoint, PortKind>,
    /// Edge destinations: boundary outputs and block inputs, with their kind.
    destinations: BTreeMap<Endpoint, PortKind>,
}

impl Validator {
    fn push(&mut self, code: FindingCode, subject: impl Into<String>, detail: impl Into<String>) {
        self.findings.push(Finding {
            subject: subject.into(),
            code,
            detail: detail.into(),
        });
    }

    fn names(&mut self, graph: &BlockGraph) {
        if !is_identifier(&graph.name) {
            self.push(
                FindingCode::InvalidName,
                &graph.name,
                "sequence name must be ASCII letters, digits or '_', starting with a letter",
            );
        }
        // Boundary names are matched by suffix during vector replay, so they get the stricter
        // rules; block ids only need to be identifiers (the library has a block named `z`).
        let boundary = graph
            .inputs
            .iter()
            .map(|i| (i.name.as_str(), true))
            .chain(graph.outputs.iter().map(|o| (o.name.as_str(), true)));
        let names = boundary.chain(graph.blocks.iter().map(|b| (b.id.as_str(), false)));
        let mut seen = BTreeSet::new();
        for (name, is_boundary) in names {
            if !seen.insert(name) {
                self.push(
                    FindingCode::DuplicateName,
                    name,
                    "input names, output names and block ids must all be distinct",
                );
            }
            if !is_identifier(name) {
                self.push(
                    FindingCode::InvalidName,
                    name,
                    "use ASCII letters, digits or '_', starting with a letter",
                );
            } else if is_boundary && (name.len() < 2 || looks_like_port(name)) {
                self.push(
                    FindingCode::InvalidName,
                    name,
                    "input and output names need 2+ characters and must not look like a port (u, y, u1)",
                );
            }
        }
        for input in &graph.inputs {
            self.sources
                .insert(Endpoint::Boundary(input.name.clone()), input.ty.port_kind());
        }
        for output in &graph.outputs {
            self.destinations.insert(
                Endpoint::Boundary(output.name.clone()),
                output.ty.port_kind(),
            );
        }
    }

    fn blocks(&mut self, graph: &BlockGraph, palette: &Palette) {
        for block in &graph.blocks {
            let Some(spec) = palette.class(&block.class) else {
                self.push(
                    FindingCode::UnknownClass,
                    &block.id,
                    format!("no CDL class {}", block.class),
                );
                continue;
            };
            if !spec.authorable {
                self.push(
                    FindingCode::ClassNotAuthorable,
                    &block.id,
                    format!("{} is width-driven or reserved", block.class),
                );
            }
            for port in &spec.outputs {
                let at = port_endpoint(&block.id, &port.name);
                self.sources.insert(at, port.kind);
            }
            for port in &spec.inputs {
                let at = port_endpoint(&block.id, &port.name);
                self.destinations.insert(at, port.kind);
            }
            let mut set = BTreeSet::new();
            for param in &block.params {
                let subject = format!("{}.{}", block.id, param.name);
                if !set.insert(param.name.as_str()) {
                    self.push(FindingCode::DuplicateParam, &subject, "parameter set twice");
                }
                let Some(pspec) = spec.params.get(&param.name) else {
                    self.push(
                        FindingCode::UnknownParam,
                        &subject,
                        format!("{} has no parameter {}", block.class, param.name),
                    );
                    continue;
                };
                if let Some(problem) = param_problem(pspec.kind, &param.value) {
                    let code = match pspec.kind {
                        ParamKind::Enum | ParamKind::String | ParamKind::Unspecified => {
                            FindingCode::UnsupportedParam
                        }
                        _ => FindingCode::ParamTypeMismatch,
                    };
                    self.push(code, &subject, problem);
                }
            }
            for (name, pspec) in &spec.params {
                if pspec.required && !set.contains(name.as_str()) {
                    self.push(
                        FindingCode::MissingRequiredParam,
                        format!("{}.{}", block.id, name),
                        format!("{} requires {}", block.class, name),
                    );
                }
            }
        }
    }

    fn edges(&mut self, graph: &BlockGraph) {
        let mut drivers: BTreeMap<Endpoint, usize> = BTreeMap::new();
        for text in &graph.connections {
            let Some(edge) = parse_edge(text) else {
                self.push(
                    FindingCode::MalformedEdge,
                    text,
                    "write edges as '<src> -> <dst>' with src an input or block.outPort",
                );
                continue;
            };
            let from = self.sources.get(&edge.from).copied();
            let to = self.destinations.get(&edge.to).copied();
            if from.is_none() {
                self.push(
                    FindingCode::UnknownSource,
                    text,
                    format!("{} is not a boundary input or a block output", edge.from),
                );
            }
            if to.is_none() {
                self.push(
                    FindingCode::UnknownDestination,
                    text,
                    format!("{} is not a boundary output or a block input", edge.to),
                );
            }
            if let (Some(a), Some(b)) = (from, to)
                && a != b
            {
                self.push(
                    FindingCode::TypeMismatch,
                    text,
                    format!(
                        "{} is {} but {} is {}",
                        edge.from,
                        a.as_str(),
                        edge.to,
                        b.as_str()
                    ),
                );
            }
            *drivers.entry(edge.to).or_default() += 1;
        }
        for (dest, kind) in &self.destinations.clone() {
            match drivers.get(dest).copied().unwrap_or(0) {
                0 => self.push(
                    FindingCode::Undriven,
                    dest.to_string(),
                    format!("{} input has no incoming edge", kind.as_str()),
                ),
                1 => {}
                n => self.push(
                    FindingCode::MultiplyDriven,
                    dest.to_string(),
                    format!("{n} edges drive it; exactly one is allowed"),
                ),
            }
        }
    }
}

fn port_endpoint(block: &str, port: &str) -> Endpoint {
    Endpoint::Port {
        block: block.to_owned(),
        port: port.to_owned(),
    }
}

fn param_problem(kind: ParamKind, value: &ParamValue) -> Option<String> {
    match (kind, value) {
        (ParamKind::Real, ParamValue::Number(x)) if x.is_finite() => None,
        (ParamKind::Integer, ParamValue::Number(x))
            if x.is_finite() && x.fract() == 0.0 && x.abs() < 9.0e15 =>
        {
            None
        }
        (ParamKind::Boolean, ParamValue::Bool(_)) => None,
        (ParamKind::Real, _) => Some("expects a finite number".into()),
        (ParamKind::Integer, _) => Some("expects a whole number".into()),
        (ParamKind::Boolean, _) => Some("expects true or false".into()),
        (ParamKind::Enum | ParamKind::String, _) => {
            Some("enum and string parameters are not supported yet; leave the default".into())
        }
        (ParamKind::Unspecified, _) => {
            Some("derived parameter; the engine computes it, do not set it".into())
        }
    }
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `u`, `y`, `u1`, `y12`: the names vector replay could confuse with a block port.
fn looks_like_port(name: &str) -> bool {
    let rest = name.strip_prefix('u').or_else(|| name.strip_prefix('y'));
    rest.is_some_and(|r| r.chars().all(|c| c.is_ascii_digit()))
}
