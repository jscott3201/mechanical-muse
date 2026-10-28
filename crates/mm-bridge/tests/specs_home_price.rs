//! The home price-response spec (`specs/home_price_response/`): the hand-built reference graph
//! must pass every vector on the pinned engine, and each common mistake must fail at least one
//! scenario. Only a spec that passes this gate is shown to the model.

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

fn add_block(graph: &mut BlockGraph, id: &str, class: &str) {
    graph.blocks.push(mm_graph::Block {
        id: id.to_owned(),
        class: class.to_owned(),
        params: Vec::new(),
    });
}

#[test]
fn home_price_response_reference_passes() {
    let (graph, vectors) = load("home_price_response");
    let failures = failing(&graph, &vectors);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn home_price_response_vectors_catch_common_mistakes() {
    let n = ParamValue::Number;
    let mutations: Vec<Mutation> = vec![
        (
            "peak at half the price range",
            Box::new(move |g| set_param(g, "peakPart", "k", n(0.5))),
        ),
        (
            "peak only strictly above the threshold",
            Box::new(|g| {
                add_block(g, "strict", "Reals.Greater");
                g.connections.push("pri00 -> strict.u1".into());
                g.connections.push("threshold.y -> strict.u2".into());
                rewire(g, "nowAtPeak.y -> peakNow.u1", "strict.y -> peakNow.u1");
            }),
        ),
        (
            "no minimum price spread",
            Box::new(|g| {
                rewire(g, "hasPeak.y -> peakNow.u2", "nowAtPeak.y -> peakNow.u2");
                rewire(g, "hasPeak.y -> peakSoon.u2", "soonAtPeak.y -> peakSoon.u2");
            }),
        ),
        (
            "minimum spread of zero",
            Box::new(move |g| set_param(g, "hasPeak", "t", n(0.0))),
        ),
        (
            "day max misses the last hour",
            Box::new(|g| rewire(g, "pri23 -> dayMax.u24", "pri22 -> dayMax.u24")),
        ),
        (
            "day min misses the last hour",
            Box::new(|g| rewire(g, "pri23 -> dayMin.u24", "pri22 -> dayMin.u24")),
        ),
        (
            "look-ahead of four hours",
            Box::new(move |g| {
                set_param(g, "soonMax", "nin", n(4.0));
                g.connections.push("pri04 -> soonMax.u4".into());
            }),
        ),
        (
            "coming floor ignored (floor now only)",
            Box::new(|g| {
                rewire(
                    g,
                    "floorSoon.y -> floorSoonPlus.u",
                    "TLo00 -> floorSoonPlus.u",
                );
                rewire(g, "floorSoon.y -> coastFloor.u", "TLo00 -> coastFloor.u");
            }),
        ),
        (
            "coming ceiling ignored (ceiling now only)",
            Box::new(|g| rewire(g, "ceilingSoon.y -> ceilingCap.u", "THi00 -> ceilingCap.u")),
        ),
        (
            "margin of 0.5 K above the floor",
            Box::new(move |g| {
                set_param(g, "floorNowPlus", "p", n(0.5));
                set_param(g, "floorSoonPlus", "p", n(0.5));
            }),
        ),
        (
            "away setpoint of 21 degC",
            Box::new(move |g| set_param(g, "awaySet", "k", n(294.15))),
        ),
        (
            "coast 1 K below base",
            Box::new(move |g| set_param(g, "coastBase", "p", n(-1.0))),
        ),
        (
            "coast 0.2 K above the coming floor",
            Box::new(move |g| set_param(g, "coastFloor", "p", n(0.2))),
        ),
        (
            "preheat of 1.5 K",
            Box::new(move |g| set_param(g, "boost", "p", n(1.5))),
        ),
        (
            "preheat 1 K below the ceiling",
            Box::new(move |g| set_param(g, "ceilingCap", "p", n(-1.0))),
        ),
        (
            "warm band of 1 K",
            Box::new(move |g| set_param(g, "warmLine", "p", n(1.0))),
        ),
        (
            "preheat even when already warm",
            Box::new(|g| rewire(g, "notWarm.y -> doPreheat.u2", "peakSoon.y -> doPreheat.u2")),
        ),
        (
            "preheat wins over coasting",
            Box::new(|g| {
                rewire(g, "peakNow.y -> choose.u2", "doPreheat.y -> choose.u2");
                rewire(g, "coast.y -> choose.u1", "preheat.y -> choose.u1");
                rewire(
                    g,
                    "doPreheat.y -> beforePeak.u2",
                    "peakNow.y -> beforePeak.u2",
                );
                rewire(g, "preheat.y -> beforePeak.u1", "coast.y -> beforePeak.u1");
            }),
        ),
    ];
    assert_caught("home_price_response", mutations);
}
