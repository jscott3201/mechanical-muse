//! Emitter goldens. Refresh only with `UPDATE_GOLDEN=1 cargo test -p mm-graph --test golden` and
//! review the diff before committing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use std::path::PathBuf;

use mm_graph::{BlockGraph, EmitOptions, Palette, emit_cxf};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn check_golden(name: &str) {
    let graph_path = golden_dir().join(format!("{name}.graph.json"));
    let cxf_path = golden_dir().join(format!("{name}.cxf.jsonld"));
    let graph: BlockGraph = serde_json::from_slice(&std::fs::read(&graph_path).unwrap()).unwrap();
    let palette = Palette::pinned().unwrap();
    let first = emit_cxf(&graph, &palette, &EmitOptions::default()).unwrap();
    let second = emit_cxf(&graph, &palette, &EmitOptions::default()).unwrap();
    assert_eq!(first, second, "emitter is not deterministic");

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&cxf_path, &first).unwrap();
        return;
    }
    let golden = std::fs::read(&cxf_path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with UPDATE_GOLDEN=1", cxf_path.display()));
    assert!(
        first == golden,
        "{name}: emitted CXF differs from {}",
        cxf_path.display()
    );
}

#[test]
fn ahu_0027_stale_override() {
    check_golden("ahu_0027");
}

#[test]
fn ahu_0005_supply_below_mixed() {
    check_golden("ahu_0005");
}
