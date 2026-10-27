//! The supervisor on the verified chilled-water plant reset, driven by a scripted plant. These
//! cover the paths a live run cannot easily force: bad readings, stale data, closed gates,
//! clamps and refusal to load unverified CXF.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use mm_bridge::load_cxf;
use mm_graph::{BlockGraph, EmitOptions, Palette, emit_cxf};
use mm_runtime::{
    Event, FailSafe, FakePlant, InputBinding, LoadError, Measurements, OutputBinding,
    RunningSequence, Scalar, Source, Supervisor,
};

const SEQ: &str = "chw";
const TSUP: &str = "oveChiWatSys_TWSet";
const DP: &str = "oveChiWatSys_dpSet";

fn cxf() -> Vec<u8> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../specs/chw_plant_reset/graph.json");
    let graph: BlockGraph = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    emit_cxf(&graph, &Palette::pinned().unwrap(), &EmitOptions::default()).unwrap()
}

fn verified_id(cxf: &[u8]) -> String {
    load_cxf(cxf).ok().unwrap().content_id
}

fn bindings() -> Vec<InputBinding> {
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

fn outputs(gate: Option<Source>, tsup_max: f64) -> Vec<(String, OutputBinding)> {
    vec![
        (
            SEQ.into(),
            OutputBinding {
                output: "TSupSet".into(),
                point: TSUP.into(),
                min: 278.15,
                max: tsup_max,
                gate: gate.clone(),
            },
        ),
        (
            SEQ.into(),
            OutputBinding {
                output: "dpSet".into(),
                point: DP.into(),
                min: 0.0,
                max: 478_250.0,
                gate,
            },
        ),
    ]
}

fn supervisor(policy: FailSafe, gate: Option<Source>, tsup_max: f64) -> Supervisor {
    let cxf = cxf();
    let seq = RunningSequence::load(SEQ, &cxf, &verified_id(&cxf), bindings()).unwrap();
    Supervisor::new(vec![seq], outputs(gate, tsup_max), policy)
}

fn reading(time: f64, occ: f64) -> Measurements {
    [("time".to_owned(), time), ("occ".to_owned(), occ)]
        .into_iter()
        .collect()
}

#[test]
fn refuses_cxf_that_is_not_the_verified_one() {
    let cxf = cxf();
    let err = RunningSequence::load(SEQ, &cxf, "some-other-content-id", bindings())
        .err()
        .unwrap();
    assert!(matches!(err, LoadError::NotVerified { .. }), "{err:?}");
}

#[test]
fn refuses_unbound_and_unknown_inputs() {
    let cxf = cxf();
    let id = verified_id(&cxf);
    let mut short = bindings();
    short.pop();
    let err = RunningSequence::load(SEQ, &cxf, &id, short).err().unwrap();
    assert_eq!(
        err,
        LoadError::Unbound {
            sequence: SEQ.into(),
            input: "u1StaPro".into()
        }
    );

    let mut extra = bindings();
    extra.push(InputBinding {
        input: "nope".into(),
        source: Source::Constant {
            value: Scalar::Bool(true),
        },
    });
    let err = RunningSequence::load(SEQ, &cxf, &id, extra).err().unwrap();
    assert!(matches!(err, LoadError::UnknownInput { .. }), "{err:?}");
}

/// End to end through the real engine: the setpoint holds for the 900 s delay plus one 300 s
/// sample, then trims (the same numbers as the spec's `trim_from_enable` vector).
#[tokio::test]
async fn runs_the_verified_reset_and_writes_setpoints() {
    let mut sup = supervisor(FailSafe::HoldLast, None, 288.15);
    let script: Vec<_> = (1..=25)
        .map(|k| reading(60.0 * f64::from(k), 1.0))
        .collect();
    let mut plant = FakePlant::new(script);
    let mut reports = Vec::new();
    let n = sup
        .run(&mut plant, reading(0.0, 1.0), 100, |r| {
            reports.push(r.clone())
        })
        .await
        .unwrap();
    assert_eq!(n, 26, "stops when the script ends");
    let tsup = |t: usize| plant.writes[t][&format!("{TSUP}_u")];
    assert_eq!(plant.writes[0][&format!("{TSUP}_activate")], 1.0);
    assert!((tsup(0) - 278.71).abs() < 1e-6);
    assert!((tsup(19) - 278.71).abs() < 1e-6, "still waiting at 1140 s");
    assert!(
        (tsup(20) - 278.8476).abs() < 1e-6,
        "first trim at 1200 s: {}",
        tsup(20)
    );
    assert!((plant.writes[0][&format!("{DP}_u")] - 239_125.0).abs() < 1e-3);
    assert!(
        reports.iter().all(|r| r.events.is_empty()),
        "{:?}",
        reports[0].events
    );
}

#[test]
fn bad_reading_holds_last_good_value_and_never_reaches_the_engine() {
    let mut sup = supervisor(FailSafe::HoldLast, None, 288.15);
    let first = sup.tick(&reading(0.0, 1.0));
    let held = first.writes[&format!("{TSUP}_u")];
    let bad = sup.tick(&reading(60.0, f64::NAN));
    assert!(matches!(&bad.events[0], Event::Degraded { sequence, .. } if sequence == SEQ));
    assert_eq!(bad.writes[&format!("{TSUP}_u")], held);
    assert_eq!(bad.writes[&format!("{TSUP}_activate")], 1.0);
    assert!(bad.events.contains(&Event::Held {
        point: TSUP.into(),
        value: held
    }));
    // The engine never saw the bad frame, so the next good one at 120 s is accepted.
    let next = sup.tick(&reading(120.0, 1.0));
    assert!(next.events.is_empty(), "{:?}", next.events);
}

#[test]
fn release_policy_hands_points_back_to_the_baseline() {
    let mut sup = supervisor(FailSafe::Release, None, 288.15);
    sup.tick(&reading(0.0, 1.0));
    let bad = sup.tick(&reading(60.0, f64::INFINITY));
    assert_eq!(bad.writes[&format!("{TSUP}_activate")], 0.0);
    assert!(!bad.writes.contains_key(&format!("{TSUP}_u")));
}

#[test]
fn hold_last_with_no_good_value_yet_releases() {
    let mut sup = supervisor(FailSafe::HoldLast, None, 288.15);
    let r = sup.tick(&Measurements::from([("time".to_owned(), 0.0)])); // occ missing
    assert_eq!(r.writes[&format!("{TSUP}_activate")], 0.0);
    assert!(r.events.contains(&Event::Released { point: TSUP.into() }));
}

#[test]
fn stale_or_repeated_time_does_not_step_the_engine() {
    let mut sup = supervisor(FailSafe::HoldLast, None, 288.15);
    sup.tick(&reading(60.0, 1.0));
    for t in [60.0, 0.0, f64::NAN] {
        let r = sup.tick(&reading(t, 1.0));
        assert!(
            matches!(r.events[0], Event::Stale { .. }),
            "t={t}: {:?}",
            r.events
        );
        assert!(r.outputs.is_empty());
        assert_eq!(r.writes[&format!("{TSUP}_activate")], 1.0, "held");
    }
}

#[test]
fn closed_gate_releases_and_forgets_the_held_value() {
    let gate = Some(Source::MeasurementAbove {
        point: "occ".into(),
        threshold: 0.5,
    });
    let mut sup = supervisor(FailSafe::HoldLast, gate, 288.15);
    sup.tick(&reading(0.0, 1.0));
    let off = sup.tick(&reading(60.0, 0.0));
    assert_eq!(off.writes[&format!("{TSUP}_activate")], 0.0);
    // A later failure must not resurrect the value from before the gate closed.
    let bad = sup.tick(&reading(120.0, f64::NAN));
    assert_eq!(bad.writes[&format!("{TSUP}_activate")], 0.0);
}

#[test]
fn out_of_range_output_is_clamped_and_reported() {
    let mut sup = supervisor(FailSafe::HoldLast, None, 278.5);
    let r = sup.tick(&reading(0.0, 1.0));
    assert_eq!(r.writes[&format!("{TSUP}_u")], 278.5);
    assert!(r.events.iter().any(
        |e| matches!(e, Event::Clamped { point, sent, .. } if point == TSUP && *sent == 278.5)
    ));
}
