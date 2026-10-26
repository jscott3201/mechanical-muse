//! Vector replay against library fixtures, plus the failures the repair loop must see as codes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use mm_bridge::{
    EngineErrorDto, ReplayError, ReplayLimits, ScenarioOutcome, VectorFile, load_cxf, replay,
};
use mm_graph::{EmitOptions, Palette, cxf_to_graph, emit_cxf};

const FIXTURES: [&str; 6] = [
    "AHU-0027", "AHU-0005", "HW-0011", "RTU-0007", "HP-0005", "AHU-0023",
];

fn fixture(id: &str, file: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/open-control-library")
        .join(id)
        .join(file);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn rule(id: &str) -> (Vec<u8>, VectorFile) {
    let vectors = serde_json::from_slice(&fixture(id, "vectors.json")).unwrap();
    (fixture(id, "rule.cxf.jsonld"), vectors)
}

fn only_outcome(cxf: &[u8], vectors: &VectorFile) -> ScenarioOutcome {
    let report = replay(cxf, vectors, &ReplayLimits::default()).unwrap();
    report.scenarios[0].outcome.clone()
}

fn error_code(outcome: &ScenarioOutcome) -> &str {
    match outcome {
        ScenarioOutcome::Error { code, .. } => code,
        other => panic!("expected an error outcome, got {other:?}"),
    }
}

#[test]
fn library_fixtures_pass_every_scenario() {
    for id in FIXTURES {
        let (cxf, vectors) = rule(id);
        let report = replay(&cxf, &vectors, &ReplayLimits::default()).unwrap();
        let failed: Vec<_> = report
            .scenarios
            .iter()
            .filter(|s| s.outcome != ScenarioOutcome::Pass)
            .collect();
        assert!(failed.is_empty(), "{id}: {failed:#?}");
        assert_eq!(report.pass_count(), vectors.scenarios.len());
    }
}

#[test]
fn replay_is_deterministic() {
    let (cxf, vectors) = rule("HW-0011");
    let a = replay(&cxf, &vectors, &ReplayLimits::default()).unwrap();
    let b = replay(&cxf, &vectors, &ReplayLimits::default()).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}

#[test]
fn a_constant_false_graph_fails() {
    // AHU-0005's logic, but yFault is ANDed with a constant false. The inputs stay connected:
    // the engine drops a boundary input nothing reads (docs/upstream-gaps.md).
    let (original, vectors) = rule("AHU-0005");
    let mut imported = cxf_to_graph(&original).unwrap();
    let g = &mut imported.graph;
    let into_fault = g
        .connections
        .iter()
        .position(|e| e.ends_with("-> yFault"))
        .unwrap();
    let source = g.connections.remove(into_fault).replace(" -> yFault", "");
    g.blocks.extend(
        serde_json::from_str::<Vec<mm_graph::Block>>(
            r#"[{"id":"never","class":"Logical.Sources.Constant","params":[{"name":"k","value":false}]},
                {"id":"gate","class":"Logical.And","params":[]}]"#,
        )
        .unwrap(),
    );
    g.connections.extend([
        format!("{source} -> gate.u1"),
        "never.y -> gate.u2".into(),
        "gate.y -> yFault".into(),
    ]);
    let cxf = emit_cxf(g, &Palette::pinned().unwrap(), &EmitOptions::default()).unwrap();
    let report = replay(&cxf, &vectors, &ReplayLimits::default()).unwrap();
    assert!(!report.passed());
    let fail = report
        .scenarios
        .iter()
        .find_map(|s| match &s.outcome {
            ScenarioOutcome::Fail { output, .. } => Some(output.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no Fail outcome: {:?}", report.scenarios[0]));
    assert_eq!(fail, "yFault");
}

#[test]
fn a_wrong_threshold_fails_a_scenario() {
    // Raising AHU-0005's gap threshold from 3.0 to 30.0 must break the alarm scenario.
    let (original, vectors) = rule("AHU-0005");
    let text = String::from_utf8(original).unwrap();
    assert!(text.contains("\"@value\": \"3.0\""));
    let broken = text.replacen("\"@value\": \"3.0\"", "\"@value\": \"30.0\"", 1);
    let report = replay(broken.as_bytes(), &vectors, &ReplayLimits::default()).unwrap();
    assert!(!report.passed());
    // A real Fail on yFault, not merely an error.
    assert!(report.scenarios.iter().any(|s| matches!(
        &s.outcome,
        ScenarioOutcome::Fail { output, .. } if output == "yFault"
    )));
}

#[test]
fn unknown_class_is_a_typed_load_failure() {
    let (cxf, vectors) = rule("AHU-0027");
    let broken = String::from_utf8(cxf).unwrap().replacen(
        "CDL.Logical.TrueDelay\"",
        "CDL.Logical.TrueDelayX\"",
        1,
    );
    let err = replay(broken.as_bytes(), &vectors, &ReplayLimits::default()).unwrap_err();
    let ReplayError::Load(failure) = &err else {
        panic!("{err:?}")
    };
    assert_eq!(
        (failure.stage, failure.code.as_str()),
        ("import", "class-not-found")
    );
}

#[test]
fn bad_connection_is_unresolved_reference() {
    let (cxf, vectors) = rule("AHU-0027");
    let text = String::from_utf8(cxf).unwrap();
    // Point persist's input connection at a node that does not exist.
    let target = "\"S231:isConnectedTo\": {\n        \"@id\": \"urn:cxf-library:ahu-fc-061#ahu_fc_061.persist.u\"";
    assert!(text.contains(target), "fixture layout changed");
    let broken = text.replacen(target, &target.replace("persist.u", "nobody.u"), 1);
    let err = replay(broken.as_bytes(), &vectors, &ReplayLimits::default()).unwrap_err();
    let ReplayError::Load(failure) = &err else {
        panic!("{err:?}")
    };
    let codes: Vec<_> = failure
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    assert!(codes.contains(&"unresolved-reference"), "{codes:?}");
}

#[test]
fn an_algebraic_loop_gets_a_code_even_without_diagnostics() {
    let g: mm_graph::BlockGraph = serde_json::from_str(
        r#"{"name":"loop_test","inputs":[{"name":"xin","type":"real","unit":"1","description":""}],
            "outputs":[{"name":"xout","type":"real","description":""}],
            "blocks":[{"id":"add","class":"Reals.Add","params":[]}],
            "connections":["xin -> add.u1","add.y -> add.u2","add.y -> xout"]}"#,
    )
    .unwrap();
    let cxf = emit_cxf(&g, &Palette::pinned().unwrap(), &EmitOptions::default()).unwrap();
    let failure = load_cxf(&cxf)
        .err()
        .expect("the engine refuses an algebraic loop");
    assert_eq!(failure.stage, "schedule");
    // The engine gives no diagnostic here, so the code is our own fallback.
    assert_eq!(failure.code, "load-failed-schedule", "{failure:?}");
    assert!(failure.diagnostics.is_empty());
}

#[test]
fn input_and_output_problems_are_scenario_errors() {
    let (cxf, vectors) = rule("AHU-0005");
    let base: serde_json::Value = serde_json::to_value(&vectors).unwrap();
    let variant = |edit: &dyn Fn(&mut serde_json::Value)| {
        let mut v = base.clone();
        v["scenarios"] = serde_json::Value::Array(vec![v["scenarios"][0].clone()]);
        edit(&mut v["scenarios"][0]);
        let file: VectorFile = serde_json::from_value(v).unwrap();
        only_outcome(&cxf, &file)
    };

    let unset = variant(&|s| {
        s["inputs"].as_object_mut().unwrap().remove("sat");
    });
    assert_eq!(error_code(&unset), "unset-input");

    let defaulted = variant(&|s| {
        s["inputs"].as_object_mut().unwrap().remove("sat");
        s["defaults"] = serde_json::json!({"sat": 25.0});
    });
    assert_eq!(defaulted, ScenarioOutcome::Pass);

    let wrong_type = variant(&|s| s["inputs"]["sat"] = serde_json::json!(true));
    assert_eq!(error_code(&wrong_type), "input-type-mismatch");

    let unknown_input = variant(&|s| s["inputs"]["oat"] = serde_json::json!(1.0));
    assert_eq!(error_code(&unknown_input), "unknown-name");

    let unknown_output = variant(&|s| s["expect"][0]["output"] = serde_json::json!("yAlarm"));
    assert_eq!(error_code(&unknown_output), "unknown-name");

    let wrong_expect = variant(&|s| s["expect"][0]["equals"] = serde_json::json!(1.0));
    assert_eq!(error_code(&wrong_expect), "expect-type-mismatch");
}

#[test]
fn bad_files_are_rejected_before_running() {
    let (cxf, mut vectors) = rule("AHU-0005");
    vectors.clock.step_s = 0.0;
    let err = replay(&cxf, &vectors, &ReplayLimits::default()).unwrap_err();
    assert_eq!(err.code(), "bad-clock");

    let (cxf, mut vectors) = rule("AHU-0005");
    vectors.clock.horizon_s = 1e9;
    let err = replay(&cxf, &vectors, &ReplayLimits::default()).unwrap_err();
    assert_eq!(err.code(), "too-many-samples");
}

#[test]
fn engine_refuses_a_frame_missing_an_input() {
    // The stepper never sends a partial frame, so drive the engine directly to pin its code.
    let loaded = load_cxf(&fixture("AHU-0005", "rule.cxf.jsonld")).unwrap();
    let defs = loaded.engine.input_definitions().unwrap();
    let only_first = [(defs[0].path.as_str(), oce_api::Value::Real(20.0))];
    let err = loaded.engine.prepare_frame(0.0, &only_first).unwrap_err();
    assert_eq!(EngineErrorDto::from_error(&err).kind, "frame-missing-input");
}

/// Every rule in a full library checkout, on our pinned engine. Run in release:
/// `MM_LIBRARY_DIR=<open-control-library at 124427b> cargo test --release -p mm-bridge
/// --test replay -- --ignored`.
#[test]
#[ignore = "needs MM_LIBRARY_DIR pointing at an open-control-library checkout"]
fn whole_library_replays_on_the_pinned_engine() {
    let dir = std::env::var("MM_LIBRARY_DIR").expect("set MM_LIBRARY_DIR");
    let mut rules: Vec<PathBuf> = std::fs::read_dir(format!("{dir}/faults"))
        .unwrap()
        .flat_map(|family| {
            std::fs::read_dir(family.unwrap().path())
                .into_iter()
                .flatten()
        })
        .map(|rule| rule.unwrap().path())
        .filter(|path| path.join("vectors.json").exists())
        .collect();
    rules.sort();
    assert_eq!(rules.len(), 137);
    let (mut scenarios, mut passed, mut failures) = (0, 0, Vec::new());
    for path in &rules {
        let cxf = std::fs::read(path.join("rule.cxf.jsonld")).unwrap();
        let vectors: VectorFile =
            serde_json::from_slice(&std::fs::read(path.join("vectors.json")).unwrap()).unwrap();
        let report = replay(&cxf, &vectors, &ReplayLimits::default())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        scenarios += report.scenarios.len();
        passed += report.pass_count();
        for s in report
            .scenarios
            .iter()
            .filter(|s| s.outcome != ScenarioOutcome::Pass)
        {
            failures.push(format!("{} {}: {:?}", path.display(), s.name, s.outcome));
        }
    }
    eprintln!(
        "{passed}/{scenarios} scenarios passed across {} rules",
        rules.len()
    );
    assert!(
        failures.is_empty(),
        "{} failing scenarios:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Files that would let a verdict rest on checks that never ran are rejected up front.
#[test]
fn vacuous_checks_are_rejected() {
    let reject = |edit: &dyn Fn(&mut serde_json::Value)| {
        let (cxf, vectors) = rule("AHU-0005"); // step_s 300, horizon_s 5400
        let mut v = serde_json::to_value(&vectors).unwrap();
        edit(&mut v);
        let file: VectorFile = serde_json::from_value(v).unwrap();
        replay(&cxf, &file, &ReplayLimits::default())
            .unwrap_err()
            .code()
            .to_owned()
    };
    let window = |from: f64, to: f64| {
        move |v: &mut serde_json::Value| {
            v["scenarios"][0]["expect"][0]["from_s"] = serde_json::json!(from);
            v["scenarios"][0]["expect"][0]["to_s"] = serde_json::json!(to);
        }
    };
    assert_eq!(reject(&window(100.0, 200.0)), "window-misses-ticks");
    assert_eq!(reject(&window(6000.0, 9000.0)), "window-misses-ticks");
    assert_eq!(reject(&window(-900.0, -1.0)), "window-misses-ticks");
    assert_eq!(reject(&window(200.0, 100.0)), "bad-window");
    assert_eq!(
        reject(&|v| v["scenarios"][0]["expect"] = serde_json::json!([])),
        "no-expectations"
    );
    assert_eq!(
        reject(&|v| v["scenarios"] = serde_json::json!([])),
        "no-scenarios"
    );
    assert_eq!(
        reject(&|v| v["schema"] = serde_json::json!("bogus/v9")),
        "unknown-schema"
    );
    assert_eq!(
        reject(&|v| v["scenarios"][0]["expect"][0]["tolerance"] = serde_json::json!(0.5)),
        "bad-tolerance",
        "tolerance on a Boolean expectation"
    );
}

#[test]
fn windows_on_tick_edges_are_accepted() {
    // A one-tick window exactly at t = 300 and one at the horizon both contain a tick.
    let (cxf, vectors) = rule("AHU-0005");
    for (from, to) in [(300.0, 300.0), (5400.0, 5400.0), (0.0, 0.0)] {
        let mut v = serde_json::to_value(&vectors).unwrap();
        v["scenarios"][0]["expect"][0]["from_s"] = serde_json::json!(from);
        v["scenarios"][0]["expect"][0]["to_s"] = serde_json::json!(to);
        let file: VectorFile = serde_json::from_value(v).unwrap();
        assert!(
            replay(&cxf, &file, &ReplayLimits::default()).is_ok(),
            "{from}..{to}"
        );
    }
}

#[test]
fn an_empty_report_is_not_a_pass() {
    let report = mm_bridge::ReplayReport {
        content_id: "cxf:none".into(),
        scenarios: vec![],
    };
    assert!(!report.passed());
}
