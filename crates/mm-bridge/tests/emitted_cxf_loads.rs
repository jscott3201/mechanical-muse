//! The engine accepts what mm-graph emits: goldens load with no diagnostics, and a library rule
//! re-emitted through the compact graph gets the same engine content id as the original file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use mm_graph::{BlockGraph, EmitOptions, Palette, cxf_to_graph, emit_cxf};
use oce_api::Engine;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// Loads CXF into a fresh engine, requires a clean receipt, and returns the export content id.
fn load_clean(bytes: &[u8]) -> String {
    let mut engine = Engine::in_memory();
    let receipt = match engine.load_cxf_with_receipt(bytes) {
        Ok(receipt) => receipt,
        Err(failure) => {
            let codes: Vec<_> = failure
                .diagnostics()
                .records()
                .iter()
                .map(|r| r.key().code().to_owned())
                .collect();
            panic!("load failed at {:?}: {codes:?}", failure.stage());
        }
    };
    let records = receipt.diagnostics().records();
    assert!(
        records.is_empty(),
        "{} diagnostic(s) on load",
        records.len()
    );
    engine.export_cxf().unwrap().content_id_complete().unwrap()
}

#[test]
fn goldens_load_without_diagnostics_and_ids_are_stable() {
    for name in ["ahu_0027", "ahu_0005"] {
        let graph_json = std::fs::read(repo(&format!(
            "crates/mm-graph/tests/golden/{name}.graph.json"
        )))
        .unwrap();
        let graph: BlockGraph = serde_json::from_slice(&graph_json).unwrap();
        let cxf = emit_cxf(&graph, &Palette::pinned().unwrap(), &EmitOptions::default()).unwrap();
        let golden = std::fs::read(repo(&format!(
            "crates/mm-graph/tests/golden/{name}.cxf.jsonld"
        )))
        .unwrap();
        assert!(
            cxf == golden,
            "{name}: emitter output drifted from its golden"
        );
        assert_eq!(
            load_clean(&cxf),
            load_clean(&cxf),
            "{name}: content id unstable"
        );
    }
}

#[test]
fn reemitted_library_rules_keep_their_content_id() {
    let palette = Palette::pinned().unwrap();
    // RTU-0007 (literal "0.30") and HP-0005 (single output as an object) differ in layout only.
    for id in ["AHU-0027", "AHU-0005", "HW-0011", "RTU-0007", "HP-0005"] {
        let original = std::fs::read(repo(&format!(
            "fixtures/open-control-library/{id}/rule.cxf.jsonld"
        )))
        .unwrap();
        let imported = cxf_to_graph(&original).unwrap();
        let options = EmitOptions {
            namespace: Some(imported.namespace),
            root_label: Some(imported.root_label),
        };
        let emitted = emit_cxf(&imported.graph, &palette, &options).unwrap();
        assert_eq!(load_clean(&emitted), load_clean(&original), "{id}");
    }
}

/// Known gap (docs/upstream-gaps.md): the export content id depends on the authored `@graph` node
/// order. AHU-0023 lists one port node out of block order, so re-emitting it in the emitter's
/// order loads cleanly but gets a different id. If this starts failing, update the gap.
#[test]
fn content_id_depends_on_node_order() {
    let original = std::fs::read(repo(
        "fixtures/open-control-library/AHU-0023/rule.cxf.jsonld",
    ))
    .unwrap();
    let imported = cxf_to_graph(&original).unwrap();
    let options = EmitOptions {
        namespace: Some(imported.namespace),
        root_label: Some(imported.root_label),
    };
    let emitted = emit_cxf(&imported.graph, &Palette::pinned().unwrap(), &options).unwrap();
    assert_ne!(load_clean(&emitted), load_clean(&original));
}
