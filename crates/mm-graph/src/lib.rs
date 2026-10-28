//! Block-graph types, catalog-backed validator and deterministic CXF emitter.
//!
//! The model writes a compact [`BlockGraph`]; [`validate`] checks it against the engine catalog
//! ([`Palette`]); [`emit_cxf`] expands it to CXF JSON-LD bytes that depend only on the graph.
//! [`cxf_to_graph`] goes the other way, for turning library rules into expected graphs.

#![forbid(unsafe_code)]

mod emit;
mod graph;
mod import;
mod json;
mod number;
mod palette;
mod validate;

pub use emit::{EmitError, EmitOptions, emit_cxf};
pub use graph::{
    Block, BlockGraph, Edge, Endpoint, Input, Output, Param, ParamValue, SignalType, parse_edge,
};
pub use import::{ImportError, Imported, cxf_to_graph};
pub use number::python_float_repr;
pub use palette::{
    ClassSpec, MAX_WIDTH, Palette, PaletteError, ParamKind, ParamSpec, PortKind, PortSpec,
    WIDTH_PARAM, width_value,
};
pub use validate::{Finding, FindingCode, validate};

/// The model-facing JSON Schema (Meta strict structured-output subset).
pub const BLOCK_GRAPH_SCHEMA: &str = include_str!("../../../docs/block-graph.schema.json");
