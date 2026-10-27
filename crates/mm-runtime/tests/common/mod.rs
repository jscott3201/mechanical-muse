//! Shared fixtures: the verified chilled-water plant reset and its replay receipt.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::path::PathBuf;

use mm_bridge::{ReplayLimits, ReplayReport, VectorFile, replay};
use mm_graph::{BlockGraph, EmitOptions, Palette, emit_cxf};
use mm_runtime::{
    FailSafe, InputBinding, Measurements, OutputBinding, RunningSequence, Scalar, Source,
    Supervisor,
};

pub const SEQ: &str = "chw";
pub const TSUP: &str = "oveChiWatSys_TWSet";
pub const DP: &str = "oveChiWatSys_dpSet";

fn spec(file: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs/chw_plant_reset")
        .join(file);
    std::fs::read(path).unwrap()
}

pub fn graph() -> BlockGraph {
    serde_json::from_slice(&spec("graph.json")).unwrap()
}

pub fn vectors() -> VectorFile {
    serde_json::from_slice(&spec("vectors.json")).unwrap()
}

pub fn emit(graph: &BlockGraph) -> Vec<u8> {
    emit_cxf(graph, &Palette::pinned().unwrap(), &EmitOptions::default()).unwrap()
}

pub fn cxf() -> Vec<u8> {
    emit(&graph())
}

/// Any spec under `specs/<name>`: its reference graph emitted, and a real receipt from replaying
/// its vectors on that CXF.
pub fn verified(name: &str) -> (Vec<u8>, ReplayReport) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs")
        .join(name);
    let graph: BlockGraph =
        serde_json::from_slice(&std::fs::read(dir.join("graph.json")).unwrap()).unwrap();
    let vectors: VectorFile =
        serde_json::from_slice(&std::fs::read(dir.join("vectors.json")).unwrap()).unwrap();
    let cxf = emit(&graph);
    let report = replay(&cxf, &vectors, &ReplayLimits::default()).unwrap();
    assert!(report.passed(), "{name}: reference must pass its vectors");
    (cxf, report)
}

/// A real receipt: the spec's vectors replayed on the emitted model.
pub fn receipt(cxf: &[u8]) -> ReplayReport {
    let report = replay(cxf, &vectors(), &ReplayLimits::default()).unwrap();
    assert!(report.passed(), "reference must pass its vectors");
    report
}

pub fn bindings() -> Vec<InputBinding> {
    vec![
        InputBinding {
            input: "nReqRes".into(),
            source: Source::Constant {
                value: Scalar::Int(0),
            },
        },
        InputBinding {
            input: "u1Ena".into(),
            source: Source::MeasurementAbove {
                point: "occ".into(),
                threshold: 0.5,
            },
        },
        InputBinding {
            input: "u1StaPro".into(),
            source: Source::Constant {
                value: Scalar::Bool(false),
            },
        },
    ]
}

pub fn binding(
    output: &str,
    point: &str,
    min: f64,
    max: f64,
    gate: Option<Source>,
) -> (String, OutputBinding) {
    (
        SEQ.into(),
        OutputBinding {
            output: output.into(),
            point: point.into(),
            min,
            max,
            gate,
        },
    )
}

pub fn outputs(gate: Option<Source>, tsup_max: f64) -> Vec<(String, OutputBinding)> {
    vec![
        binding("TSupSet", TSUP, 278.15, tsup_max, gate.clone()),
        binding("dpSet", DP, 0.0, 478_250.0, gate),
    ]
}

pub fn sequence(name: &str, bindings: Vec<InputBinding>) -> RunningSequence {
    let cxf = cxf();
    RunningSequence::load(name, &cxf, &receipt(&cxf), bindings).unwrap()
}

pub fn supervisor(policy: FailSafe, gate: Option<Source>, tsup_max: f64) -> Supervisor {
    Supervisor::new(
        vec![sequence(SEQ, bindings())],
        outputs(gate, tsup_max),
        policy,
    )
    .unwrap()
}

pub fn reading(time: f64, occ: f64) -> Measurements {
    [("time".to_owned(), time), ("occ".to_owned(), occ)]
        .into_iter()
        .collect()
}

pub fn u(point: &str) -> String {
    format!("{point}_u")
}

pub fn active(point: &str) -> String {
    format!("{point}_activate")
}
