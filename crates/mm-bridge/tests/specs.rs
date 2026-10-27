//! Our own sequence specs (`specs/<name>/`): the hand-built reference graph must pass every
//! vector on the pinned engine, and each common mistake must fail at least one scenario. Only a
//! spec that passes this gate is shown to the model.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use mm_bridge::{ReplayLimits, ScenarioOutcome, VectorFile, replay};
use mm_graph::{BlockGraph, EmitOptions, Palette, ParamValue, emit_cxf, validate};

fn spec_file(name: &str, file: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs")
        .join(name)
        .join(file);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn load(name: &str) -> (BlockGraph, VectorFile) {
    (
        serde_json::from_slice(&spec_file(name, "graph.json")).unwrap(),
        serde_json::from_slice(&spec_file(name, "vectors.json")).unwrap(),
    )
}

/// Names of failing scenarios (empty means every scenario passed).
fn failing(graph: &BlockGraph, vectors: &VectorFile) -> Vec<String> {
    let palette = Palette::pinned().unwrap();
    assert_eq!(validate(graph, &palette), vec![], "graph must validate");
    let cxf = emit_cxf(graph, &palette, &EmitOptions::default()).unwrap();
    let report = replay(&cxf, vectors, &ReplayLimits::default()).unwrap();
    report
        .scenarios
        .iter()
        .filter(|s| s.outcome != ScenarioOutcome::Pass)
        .map(|s| format!("{}: {:?}", s.name, s.outcome))
        .collect()
}

fn set_param(graph: &mut BlockGraph, block: &str, param: &str, value: ParamValue) {
    let b = graph.blocks.iter_mut().find(|b| b.id == block).unwrap();
    let p = b.params.iter_mut().find(|p| p.name == param).unwrap();
    p.value = value;
}

fn rewire(graph: &mut BlockGraph, from: &str, to: &str) {
    let edge = graph.connections.iter_mut().find(|e| *e == from).unwrap();
    *edge = to.to_owned();
}

#[test]
fn chw_plant_reset_reference_passes() {
    let (graph, vectors) = load("chw_plant_reset");
    let failures = failing(&graph, &vectors);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn chw_plant_reset_vectors_catch_common_mistakes() {
    let (graph, vectors) = load("chw_plant_reset");
    let n = ParamValue::Number;
    type Mutation = (&'static str, Box<dyn Fn(&mut BlockGraph)>);
    let mutations: Vec<Mutation> = vec![
        // The start delay is the configured delay plus one sample period.
        (
            "delay without the extra sample period",
            Box::new(move |g| {
                set_param(g, "startDelay", "delayTime", n(900.0));
            }),
        ),
        // Requests at the ignore count do not count ("more than", not "at least").
        (
            "ignore count compared with >=",
            Box::new(move |g| {
                set_param(g, "excess", "p", n(-1.0));
            }),
        ),
        (
            "respond step not capped",
            Box::new(move |g| {
                set_param(g, "respCapValue", "k", n(1.0));
            }),
        ),
        (
            "respond amount 2 % instead of 3 %",
            Box::new(move |g| {
                set_param(g, "respGain", "k", n(0.02));
            }),
        ),
        // A disabled plant returns to the initial (design) value, not to zero.
        (
            "disabled output 0 instead of design",
            Box::new(|g| {
                rewire(g, "one.y -> enabledValue.u3", "zero.y -> enabledValue.u3");
            }),
        ),
        (
            "staging hold of 1 s instead of 900 s",
            Box::new(move |g| {
                set_param(g, "stagingHold", "trueHoldDuration", n(1.0));
            }),
        ),
        (
            // Left at its default, the false hold equals the true hold: a second staging
            // start within 900 s of the end of the first is then not seen.
            "false hold of 900 s",
            Box::new(move |g| {
                set_param(g, "stagingHold", "falseHoldDuration", n(900.0));
            }),
        ),
        (
            "hold applied continuously instead of at updates",
            Box::new(|g| {
                rewire(
                    g,
                    "frozen.y -> heldChange.u2",
                    "stagingHold.y -> heldChange.u2",
                );
            }),
        ),
        (
            "requests used without sampling",
            Box::new(|g| {
                rewire(g, "reqSampled.y -> excess.u", "reqReal.y -> excess.u");
            }),
        ),
        (
            "temperature and pressure halves swapped",
            Box::new(|g| {
                rewire(g, "half.y -> tempMap.x1", "zero.y -> tempMap.x1");
                rewire(g, "one.y -> tempMap.x2", "half.y -> tempMap.x2");
                rewire(g, "zero.y -> dpMap.x1", "half.y -> dpMap.x1");
                rewire(g, "half.y -> dpMap.x2", "one.y -> dpMap.x2");
            }),
        ),
        (
            "no clamp at 100 %",
            Box::new(move |g| {
                set_param(g, "clamped", "uMax", n(2.0));
            }),
        ),
        (
            "trim of 1 %",
            Box::new(move |g| {
                set_param(g, "trimValue", "k", n(-0.01));
                set_param(g, "trimAndResp", "p", n(-0.01));
            }),
        ),
    ];
    for (what, mutate) in mutations {
        let mut mutant = graph.clone();
        mutate(&mut mutant);
        assert_ne!(mutant, graph, "{what}: mutation did not apply");
        assert!(
            !failing(&mutant, &vectors).is_empty(),
            "vectors do not catch: {what}"
        );
    }
}

#[test]
fn ahu_chw_requests_reference_passes() {
    let (graph, vectors) = load("ahu_chw_requests");
    let failures = failing(&graph, &vectors);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn ahu_chw_requests_vectors_catch_common_mistakes() {
    let (graph, vectors) = load("ahu_chw_requests");
    let n = ParamValue::Number;
    type Mutation = (&'static str, Box<dyn Fn(&mut BlockGraph)>);
    let mutations: Vec<Mutation> = vec![
        (
            "3-request threshold of 2.5 K",
            Box::new(move |g| set_param(g, "high3", "t", n(2.5))),
        ),
        (
            "2-request threshold of 1.5 K",
            Box::new(move |g| set_param(g, "high2", "t", n(1.5))),
        ),
        // "More than 3 K", not "at least": an error of exactly 3 K gives 2 requests.
        (
            "3-request compared with >=",
            Box::new(move |g| set_param(g, "high3", "t", n(2.999))),
        ),
        (
            "no release band on 3 requests",
            Box::new(move |g| set_param(g, "high3", "h", n(0.0))),
        ),
        (
            "no release band on 2 requests",
            Box::new(move |g| set_param(g, "high2", "h", n(0.0))),
        ),
        (
            "60 s persistence",
            Box::new(move |g| set_param(g, "high3Long", "delayTime", n(60.0))),
        ),
        (
            // A condition true from the first frame passes straight through.
            "delay counted from the first frame",
            Box::new(|g| {
                set_param(g, "high3Long", "delayOnInit", ParamValue::Bool(true));
            }),
        ),
        (
            "2 checked before 3",
            Box::new(|g| {
                rewire(g, "three.y -> req3.u1", "two.y -> req3.u1");
                rewire(g, "two.y -> req2.u1", "three.y -> req2.u1");
            }),
        ),
        (
            "1 checked before 2",
            Box::new(|g| {
                rewire(g, "resetLatch.y -> req1.u2", "high2Long.y -> req1.u2");
                rewire(g, "one.y -> req1.u1", "two.y -> req1.u1");
                rewire(g, "high2Long.y -> req2.u2", "resetLatch.y -> req2.u2");
                rewire(g, "two.y -> req2.u1", "one.y -> req2.u1");
            }),
        ),
        (
            "2-request delay counted from the first frame",
            Box::new(|g| {
                set_param(g, "high2Long", "delayOnInit", ParamValue::Bool(true));
            }),
        ),
        (
            "2-request compared with >=",
            Box::new(move |g| set_param(g, "high2", "t", n(1.999))),
        ),
        (
            "2-request threshold of 2.05 K",
            Box::new(move |g| set_param(g, "high2", "t", n(2.05))),
        ),
        (
            "2-request band of 0.05 K",
            Box::new(move |g| set_param(g, "high2", "h", n(0.05))),
        ),
        (
            "2-request band of 0.12 K",
            Box::new(move |g| set_param(g, "high2", "h", n(0.12))),
        ),
        (
            "3-request band of 0.08 K",
            Box::new(move |g| set_param(g, "high3", "h", n(0.08))),
        ),
        (
            "3-request band of 0.12 K",
            Box::new(move |g| set_param(g, "high3", "h", n(0.12))),
        ),
        (
            "valve request without the latch",
            Box::new(|g| rewire(g, "resetLatch.y -> req1.u2", "valveOpen.y -> req1.u2")),
        ),
        (
            "valve opens at 90 %",
            Box::new(move |g| set_param(g, "valveOpen", "t", n(0.9))),
        ),
        (
            "reset request clears at 90 %",
            Box::new(move |g| set_param(g, "valveReset", "t", n(0.9))),
        ),
        (
            "plant request clears with the reset request",
            Box::new(|g| {
                rewire(
                    g,
                    "valveClosed.y -> plantLatch.clr",
                    "valveReset.y -> plantLatch.clr",
                )
            }),
        ),
        (
            "plant request clears at 15 %",
            Box::new(move |g| set_param(g, "valveClosed", "t", n(0.15))),
        ),
        (
            "temperature error reversed",
            Box::new(|g| {
                rewire(g, "TAirSup -> tempErr.u1", "TAirSupSet -> tempErr.u1");
                rewire(g, "TAirSupSet -> tempErr.u2", "TAirSup -> tempErr.u2");
            }),
        ),
    ];
    for (what, mutate) in mutations {
        let mut mutant = graph.clone();
        mutate(&mut mutant);
        assert_ne!(mutant, graph, "{what}: mutation did not apply");
        assert!(
            !failing(&mutant, &vectors).is_empty(),
            "vectors do not catch: {what}"
        );
    }
}
