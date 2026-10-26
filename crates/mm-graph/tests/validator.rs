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
    let removed = g.connections.remove(0);
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
    g.blocks[0].class = "Logical.MultiAnd".into();
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
