//! Library rules round-trip through the compact graph: CXF -> graph -> CXF.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod common;

use common::{canonical, library_fixture};
use mm_graph::{EmitOptions, Palette, cxf_to_graph, emit_cxf, validate};

fn round_trip(bytes: &[u8]) -> Vec<u8> {
    let palette = Palette::pinned().unwrap();
    let imported = cxf_to_graph(bytes).unwrap();
    let findings = validate(&imported.graph, &palette);
    assert!(
        findings.is_empty(),
        "library rule has findings: {findings:#?}"
    );
    let options = EmitOptions {
        namespace: Some(imported.namespace),
        root_label: Some(imported.root_label),
    };
    emit_cxf(&imported.graph, &palette, &options).unwrap()
}

#[test]
fn exact_rules_reproduce_the_library_bytes() {
    for id in ["AHU-0027", "AHU-0005", "HW-0011"] {
        let original = library_fixture(id);
        let emitted = round_trip(&original);
        assert!(
            emitted == original,
            "{id}: emitted CXF differs from the library file"
        );
    }
}

#[test]
fn every_fixture_round_trips_by_meaning() {
    for id in [
        "AHU-0027", "AHU-0005", "HW-0011", "RTU-0007", "HP-0005", "AHU-0023",
    ] {
        let original = library_fixture(id);
        let emitted = round_trip(&original);
        assert_eq!(canonical(&emitted), canonical(&original), "{id}");
    }
}

#[test]
fn ahu_0005_keeps_subtract_port_order() {
    let imported = cxf_to_graph(&library_fixture("AHU-0005")).unwrap();
    let edges = &imported.graph.connections;
    let mat = edges.iter().position(|e| e.ends_with(".u1")).unwrap();
    let sat = edges.iter().position(|e| e.ends_with(".u2")).unwrap();
    assert!(edges[mat].starts_with("mat ->") && edges[sat].starts_with("sat ->"));
}

/// Every rule in a full library checkout. Run with
/// `MM_LIBRARY_DIR=<path to open-control-library at 124427b> cargo test -p mm-graph -- --ignored`.
#[test]
#[ignore = "needs MM_LIBRARY_DIR pointing at an open-control-library checkout"]
fn whole_library_round_trips() {
    let dir = std::env::var("MM_LIBRARY_DIR").expect("set MM_LIBRARY_DIR");
    let mut rules: Vec<_> = std::fs::read_dir(format!("{dir}/faults"))
        .unwrap()
        .flat_map(|family| {
            std::fs::read_dir(family.unwrap().path())
                .into_iter()
                .flatten()
        })
        .map(|rule| rule.unwrap().path().join("rule.cxf.jsonld"))
        .filter(|path| path.exists())
        .collect();
    rules.sort();
    assert_eq!(rules.len(), 137);
    let mut exact = 0;
    let mut same_json = 0;
    let mut inexact = Vec::new();
    for path in &rules {
        let original = std::fs::read(path).unwrap();
        let emitted = round_trip(&original);
        assert_eq!(
            canonical(&emitted),
            canonical(&original),
            "{}",
            path.display()
        );
        let parsed = |b: &[u8]| serde_json::from_slice::<serde_json::Value>(b).unwrap();
        if parsed(&emitted) == parsed(&original) {
            same_json += 1;
        }
        if emitted == original {
            exact += 1;
        } else {
            inexact.push(path.display().to_string());
        }
    }
    // 130 match after JSON parsing (as the research prototype found); 2 of those (AHU-0016,
    // AHU-0018) were hand-formatted, so 128 match byte for byte.
    assert!(same_json >= 130, "only {same_json} parse-equal");
    assert!(
        exact >= 128,
        "{exact} exact; layout differs in {inexact:#?}"
    );
}
