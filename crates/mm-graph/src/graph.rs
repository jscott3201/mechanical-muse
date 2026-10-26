//! The compact block-graph format (see `docs/block-graph-schema.md`).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::palette::PortKind;

/// A control sequence as the model writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockGraph {
    pub name: String,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub blocks: Vec<Block>,
    /// Edges written `"<src> -> <dst>"`.
    pub connections: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalType {
    Bool,
    Real,
    Int,
}

impl SignalType {
    pub fn port_kind(self) -> PortKind {
        match self {
            SignalType::Bool => PortKind::Boolean,
            SignalType::Real => PortKind::Real,
            SignalType::Int => PortKind::Integer,
        }
    }
}

/// A boundary input. `unit` and `description` are for the model's own reasoning; CXF drops them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: SignalType,
    pub unit: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: SignalType,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: String,
    /// Catalog class path without the `CDL.` prefix, e.g. `Logical.TrueDelay`.
    pub class: String,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Param {
    pub name: String,
    pub value: ParamValue,
}

/// Bool is tried first so JSON `true` never becomes a number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

/// One end of an edge: a boundary connector or a block port.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Endpoint {
    Boundary(String),
    Port { block: String, port: String },
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Endpoint::Boundary(name) => f.write_str(name),
            Endpoint::Port { block, port } => write!(f, "{block}.{port}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: Endpoint,
    pub to: Endpoint,
}

/// Parses `"<src> -> <dst>"`. Returns `None` for anything else, including stray whitespace inside
/// a token or a second arrow.
pub fn parse_edge(text: &str) -> Option<Edge> {
    let (from, to) = text.split_once("->")?;
    Some(Edge {
        from: parse_endpoint(from.trim())?,
        to: parse_endpoint(to.trim())?,
    })
}

fn parse_endpoint(token: &str) -> Option<Endpoint> {
    let well_formed =
        |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    match token.split_once('.') {
        None if well_formed(token) => Some(Endpoint::Boundary(token.to_owned())),
        Some((block, port)) if well_formed(block) && well_formed(port) => Some(Endpoint::Port {
            block: block.to_owned(),
            port: port.to_owned(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_boundary_and_port_endpoints() {
        let edge = parse_edge("override_active -> staleOvr.u").unwrap();
        assert_eq!(edge.from, Endpoint::Boundary("override_active".into()));
        assert_eq!(
            edge.to,
            Endpoint::Port {
                block: "staleOvr".into(),
                port: "u".into()
            }
        );
        assert_eq!(edge.to.to_string(), "staleOvr.u");
    }

    #[test]
    fn rejects_malformed_edges() {
        for bad in [
            "a b -> c",
            "a -> b -> c",
            "a.b.c -> d",
            "-> d",
            "a ->",
            "a => b",
            ".u -> y",
        ] {
            assert!(parse_edge(bad).is_none(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn param_values_keep_their_json_type() {
        let params: Vec<Param> = serde_json::from_str(
            r#"[{"name":"a","value":true},{"name":"b","value":3},{"name":"c","value":"PI"}]"#,
        )
        .unwrap();
        assert_eq!(params[0].value, ParamValue::Bool(true));
        assert_eq!(params[1].value, ParamValue::Number(3.0));
        assert_eq!(params[2].value, ParamValue::Text("PI".into()));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let err = serde_json::from_str::<Param>(r#"{"name":"a","value":1,"unit":"s"}"#);
        assert!(err.is_err());
    }
}
