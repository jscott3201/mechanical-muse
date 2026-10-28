//! Seeded mutations of a known-good graph must each be flagged with the right code.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod common;

use mm_graph::{
    BlockGraph, EmitError, EmitOptions, FindingCode, Palette, Param, ParamValue, SignalType,
    cxf_to_graph, emit_cxf, validate,
};

fn ahu_0005() -> BlockGraph {
    cxf_to_graph(&common::library_fixture("AHU-0005"))
        .unwrap()
        .graph
}

fn codes(graph: &BlockGraph) -> Vec<(String, FindingCode)> {
    validate(graph, &Palette::pinned().unwrap())
        .into_iter()
        .map(|f| (f.subject, f.code))
        .collect()
}

fn block_id(graph: &BlockGraph, class: &str) -> String {
    graph
        .blocks
        .iter()
        .find(|b| b.class == class)
        .unwrap()
        .id
        .clone()
}

#[test]
fn baseline_is_clean() {
    assert_eq!(codes(&ahu_0005()), vec![]);
}

#[test]
fn dropped_edge_leaves_an_input_undriven() {
    let mut g = ahu_0005();
    // The last edge drives yFault; dropping the first would also leave an input unread.
    let removed = g.connections.pop().unwrap();
    let dest = removed.split(" -> ").nth(1).unwrap().to_owned();
    assert_eq!(codes(&g), vec![(dest, FindingCode::Undriven)]);
}

#[test]
fn second_driver_is_flagged() {
    let mut g = ahu_0005();
    let first_dest = g.connections[0].split(" -> ").nth(1).unwrap().to_owned();
    g.connections.push(format!("sat -> {first_dest}"));
    assert_eq!(codes(&g), vec![(first_dest, FindingCode::MultiplyDriven)]);
}

#[test]
fn unknown_class_is_flagged() {
    let mut g = ahu_0005();
    let id = block_id(&g, "Logical.TrueDelay");
    for b in &mut g.blocks {
        if b.id == id {
            b.class = "Logical.TrueDelayX".into();
        }
    }
    let found = codes(&g);
    assert!(
        found.contains(&(id, FindingCode::UnknownClass)),
        "{found:?}"
    );
}

#[test]
fn missing_required_param_is_flagged() {
    let mut g = ahu_0005();
    let id = block_id(&g, "Logical.TrueDelay");
    for b in &mut g.blocks {
        if b.id == id {
            b.params.retain(|p| p.name != "delayTime");
        }
    }
    assert_eq!(
        codes(&g),
        vec![(format!("{id}.delayTime"), FindingCode::MissingRequiredParam)]
    );
}

#[test]
fn swapped_boundary_type_is_a_type_mismatch() {
    let mut g = ahu_0005();
    g.inputs[0].ty = SignalType::Bool;
    let found = codes(&g);
    assert!(
        found.iter().all(|(_, c)| *c == FindingCode::TypeMismatch) && !found.is_empty(),
        "{found:?}"
    );
}

#[test]
fn wrong_param_value_type_is_flagged() {
    let mut g = ahu_0005();
    let id = block_id(&g, "Logical.TrueDelay");
    for b in &mut g.blocks {
        if b.id == id {
            for p in &mut b.params {
                if p.name == "delayOnInit" {
                    p.value = ParamValue::Number(1.0);
                }
            }
        }
    }
    assert_eq!(
        codes(&g),
        vec![(format!("{id}.delayOnInit"), FindingCode::ParamTypeMismatch)]
    );
}

#[test]
fn unknown_and_duplicate_params_are_flagged() {
    let mut g = ahu_0005();
    let id = block_id(&g, "Logical.TrueDelay");
    for b in &mut g.blocks {
        if b.id == id {
            b.params.push(Param {
                name: "delayTime".into(),
                value: ParamValue::Number(5.0),
            });
            b.params.push(Param {
                name: "gain".into(),
                value: ParamValue::Number(5.0),
            });
        }
    }
    let found = codes(&g);
    assert!(found.contains(&(format!("{id}.delayTime"), FindingCode::DuplicateParam)));
    assert!(found.contains(&(format!("{id}.gain"), FindingCode::UnknownParam)));
}

#[test]
fn bad_names_and_edges_are_flagged() {
    let mut g = ahu_0005();
    g.outputs[0].name = "y".into();
    g.connections.push("sat => nowhere".into());
    g.connections.push("gap.u1 -> nothing.u".into());
    let found = codes(&g);
    assert!(found.contains(&("y".into(), FindingCode::InvalidName)));
    assert!(found.contains(&("sat => nowhere".into(), FindingCode::MalformedEdge)));
    assert!(found.contains(&("gap.u1 -> nothing.u".into(), FindingCode::UnknownSource)));
    assert!(found.contains(&(
        "gap.u1 -> nothing.u".into(),
        FindingCode::UnknownDestination
    )));
}

#[test]
fn duplicate_names_are_flagged() {
    let mut g = ahu_0005();
    g.blocks[0].id = g.inputs[0].name.clone();
    let found = codes(&g);
    assert!(found.contains(&(g.inputs[0].name.clone(), FindingCode::DuplicateName)));
}

#[test]
fn width_driven_class_is_not_authorable() {
    let mut g = ahu_0005();
    g.blocks[0].class = "Reals.MultiSum".into();
    let id = g.blocks[0].id.clone();
    assert!(codes(&g).contains(&(id, FindingCode::ClassNotAuthorable)));
}

#[test]
fn findings_are_sorted_and_stable() {
    let mut g = ahu_0005();
    g.connections.clear();
    let a = codes(&g);
    let b = codes(&g);
    assert_eq!(a, b);
    let mut sorted = a.clone();
    sorted.sort();
    assert_eq!(a, sorted);
}

#[test]
fn emitter_refuses_an_invalid_graph() {
    let mut g = ahu_0005();
    g.connections.pop();
    let err = emit_cxf(&g, &Palette::pinned().unwrap(), &EmitOptions::default()).unwrap_err();
    assert!(matches!(err, EmitError::Invalid(f) if f.len() == 1));
}

#[test]
fn a_graph_without_blocks_is_flagged() {
    let mut g = ahu_0005();
    g.blocks.clear();
    g.connections = vec!["mat -> yFault".into()];
    g.outputs[0].ty = SignalType::Real;
    let found = codes(&g);
    assert!(
        found.contains(&("ahu_fc_005".into(), FindingCode::NoBlocks)),
        "{found:?}"
    );
}

#[test]
fn edges_against_the_flow_are_flagged() {
    let mut g = ahu_0005();
    g.connections.push("gap.u1 -> mat".into());
    let found = codes(&g);
    assert!(found.contains(&("gap.u1 -> mat".into(), FindingCode::UnknownSource)));
    assert!(found.contains(&("gap.u1 -> mat".into(), FindingCode::UnknownDestination)));
}

#[test]
fn derived_real_params_can_be_set() {
    let g: BlockGraph = serde_json::from_str(
        r#"{"name":"hold_test","inputs":[{"name":"run","type":"bool","unit":"bool","description":""}],
            "outputs":[{"name":"held","type":"bool","description":""}],
            "blocks":[{"id":"hold","class":"Logical.TrueFalseHold","params":[
              {"name":"trueHoldDuration","value":300},{"name":"falseHoldDuration","value":120}]}],
            "connections":["run -> hold.u","hold.y -> held"]}"#,
    )
    .unwrap();
    assert_eq!(codes(&g), vec![]);
}

#[test]
fn classes_needing_enum_params_are_not_authorable() {
    let palette = Palette::pinned().unwrap();
    assert!(
        !palette
            .class("Reals.Sources.CalendarTime")
            .unwrap()
            .authorable
    );
}

#[test]
fn integer_params_are_written_as_integers() {
    let g: BlockGraph = serde_json::from_str(
        r#"{"name":"int_test","inputs":[{"name":"count","type":"int","unit":"1","description":""}],
            "outputs":[{"name":"high","type":"bool","description":""}],
            "blocks":[{"id":"gt","class":"Integers.GreaterThreshold","params":[{"name":"t","value":3}]}],
            "connections":["count -> gt.u","gt.y -> high"]}"#,
    )
    .unwrap();
    let cxf = emit_cxf(&g, &Palette::pinned().unwrap(), &EmitOptions::default()).unwrap();
    let text = String::from_utf8(cxf).unwrap();
    assert!(text.contains("\"@value\": \"3\""), "{text}");
    assert!(text.contains("XMLSchema#integer"));
}

#[test]
fn an_input_nothing_reads_is_flagged() {
    let mut g = ahu_0005();
    g.connections.retain(|e| !e.starts_with("sat ->"));
    g.connections.push("mat -> gap.u2".into());
    assert_eq!(codes(&g), vec![("sat".into(), FindingCode::UnusedInput)]);
}

fn width_graph() -> BlockGraph {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/width_max_min.graph.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn width_graph_validates() {
    assert_eq!(codes(&width_graph()), vec![]);
}

#[test]
fn width_out_of_range_is_flagged() {
    for bad in [0.0, 2.5, 33.0] {
        let mut g = width_graph();
        g.blocks[0].params[0].value = mm_graph::ParamValue::Number(bad);
        let found = codes(&g);
        assert!(
            found.contains(&("hi.nin".to_owned(), FindingCode::InvalidWidth)),
            "nin {bad}: {found:?}"
        );
    }
}

#[test]
fn width_ports_follow_nin() {
    // nin 2: u3 no longer exists, so the edge into it has no destination.
    let mut g = width_graph();
    g.blocks[0].params[0].value = mm_graph::ParamValue::Number(2.0);
    assert!(codes(&g).contains(&("pC -> hi.u3".to_owned(), FindingCode::UnknownDestination)));
    // nin 4: u4 exists and nothing drives it.
    let mut g = width_graph();
    g.blocks[1].params[0].value = mm_graph::ParamValue::Number(4.0);
    assert!(codes(&g).contains(&("lo.u4".to_owned(), FindingCode::Undriven)));
}

#[test]
fn width_without_nin_is_flagged() {
    let mut g = width_graph();
    g.blocks[0].params.clear();
    assert!(codes(&g).contains(&("hi.nin".to_owned(), FindingCode::MissingRequiredParam)));
}
