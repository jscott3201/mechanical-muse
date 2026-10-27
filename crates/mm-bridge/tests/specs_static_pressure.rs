//! The static pressure specs (`specs/ahu_static_pressure_reset/`, `specs/zone_pressure_requests/`):
//! each hand-built reference graph must pass every vector on the pinned engine, and each common
//! mistake must fail at least one scenario. Only a spec that passes this gate is shown to the
//! model.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use mm_bridge::{ReplayLimits, ScenarioOutcome, VectorFile, replay};
use mm_graph::{BlockGraph, EmitOptions, Palette, ParamValue, emit_cxf, validate};

type Mutation = (&'static str, Box<dyn Fn(&mut BlockGraph)>);

fn load(name: &str) -> (BlockGraph, VectorFile) {
    let read = |file: &str| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../specs")
            .join(name)
            .join(file);
        std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };
    (
        serde_json::from_slice(&read("graph.json")).unwrap(),
        serde_json::from_slice(&read("vectors.json")).unwrap(),
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

fn assert_caught(name: &str, mutations: Vec<Mutation>) {
    let (graph, vectors) = load(name);
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
fn ahu_static_pressure_reset_reference_passes() {
    let (graph, vectors) = load("ahu_static_pressure_reset");
    let failures = failing(&graph, &vectors);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn ahu_static_pressure_reset_vectors_catch_common_mistakes() {
    let n = ParamValue::Number;
    let mutations: Vec<Mutation> = vec![
        // The start delay is the configured delay plus one update period.
        (
            "delay without the extra update period",
            Box::new(move |g| set_param(g, "startDelay", "delayTime", n(600.0))),
        ),
        (
            "ignore count compared with >=",
            Box::new(move |g| set_param(g, "excess", "p", n(-1.0))),
        ),
        (
            "respond step not capped",
            Box::new(move |g| set_param(g, "respCapValue", "k", n(100.0))),
        ),
        (
            "respond 12 Pa per request instead of 15",
            Box::new(move |g| set_param(g, "respGain", "k", n(12.0))),
        ),
        (
            "trim of 10 Pa",
            Box::new(move |g| {
                set_param(g, "trimValue", "k", n(-10.0));
                set_param(g, "trimAndResp", "p", n(-10.0));
            }),
        ),
        (
            "no floor at 50 Pa",
            Box::new(move |g| set_param(g, "clamped", "uMin", n(0.0))),
        ),
        (
            "no ceiling at 400 Pa",
            Box::new(move |g| set_param(g, "clamped", "uMax", n(1000.0))),
        ),
        // A stopped fan resets the stored setpoint, so a restart begins again at 400 Pa.
        (
            "restart resumes from the trimmed value",
            Box::new(|g| rewire(g, "initial.y -> fanValue.u3", "previous.y -> fanValue.u3")),
        ),
        (
            "requests used without sampling",
            Box::new(|g| rewire(g, "reqSampled.y -> excess.u", "reqReal.y -> excess.u")),
        ),
        (
            "update period of 300 s",
            Box::new(move |g| {
                set_param(g, "reqSampled", "samplePeriod", n(300.0));
                set_param(g, "previous", "samplePeriod", n(300.0));
            }),
        ),
        (
            "initial setpoint at the floor",
            Box::new(move |g| {
                set_param(g, "initial", "k", n(50.0));
                set_param(g, "previous", "y_start", n(50.0));
            }),
        ),
    ];
    assert_caught("ahu_static_pressure_reset", mutations);
}

#[test]
fn zone_pressure_requests_reference_passes() {
    let (graph, vectors) = load("zone_pressure_requests");
    let failures = failing(&graph, &vectors);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn zone_pressure_requests_vectors_catch_common_mistakes() {
    let n = ParamValue::Number;
    let b = ParamValue::Bool;
    let mutations: Vec<Mutation> = vec![
        (
            "damper request without its release band",
            Box::new(move |g| set_param(g, "damOpen", "h", n(0.0))),
        ),
        (
            "damper open above 90 % instead of 95 %",
            Box::new(move |g| set_param(g, "damOpen", "t", n(0.9))),
        ),
        (
            "no damper request (airflow only)",
            Box::new(|g| rewire(g, "damOpen.y -> oneReq.u2", "long1.y -> oneReq.u2")),
        ),
        // A shortfall already present at the first frame must still last the full 120 s.
        (
            "persistence passes a first-frame shortfall at once",
            Box::new(move |g| {
                for d in ["long1", "long2", "long3"] {
                    set_param(g, d, "delayOnInit", b(false));
                }
            }),
        ),
        (
            "persistence of 60 s",
            Box::new(move |g| {
                for d in ["long1", "long2", "long3"] {
                    set_param(g, d, "delayTime", n(60.0));
                }
            }),
        ),
        (
            "one request below 85 % instead of 90 %",
            Box::new(move |g| set_param(g, "set1", "k", n(0.85))),
        ),
        (
            "3 and 2 request fractions swapped",
            Box::new(move |g| {
                set_param(g, "set3", "k", n(0.7));
                set_param(g, "set2", "k", n(0.5));
            }),
        ),
        (
            "airflow requests without the served check",
            Box::new(|g| {
                for k in ["1", "2", "3"] {
                    rewire(
                        g,
                        &format!("served.y -> need{k}.u2"),
                        &format!("short{k}.y -> need{k}.u2"),
                    );
                }
            }),
        ),
        (
            "airflow comparisons without a release band",
            Box::new(move |g| {
                for s in ["short1", "short2", "short3"] {
                    set_param(g, s, "h", n(0.0));
                }
            }),
        ),
        (
            "airflow compared with the whole setpoint",
            Box::new(|g| rewire(g, "set3.y -> gap3.u1", "VSet_flow -> gap3.u1")),
        ),
    ];
    assert_caught("zone_pressure_requests", mutations);
}
