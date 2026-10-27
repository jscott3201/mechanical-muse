//! The supervisor on the verified chilled-water plant reset, driven by a scripted plant. These
//! cover the paths a live run cannot easily force: bad readings, stale data, closed gates,
//! clamps and chained sequences.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use common::*;
use mm_runtime::{
    Event, FailSafe, FakePlant, InputBinding, Measurements, OutputBinding, Source, SourceError,
    StepFailure, Supervisor,
};

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
    let tsup = |t: usize| plant.writes[t][&u(TSUP)];
    assert_eq!(plant.writes[0][&active(TSUP)], 1.0);
    assert!((tsup(0) - 278.71).abs() < 1e-6);
    assert!((tsup(19) - 278.71).abs() < 1e-6, "still waiting at 1140 s");
    assert!(
        (tsup(20) - 278.8476).abs() < 1e-6,
        "first trim at 1200 s: {}",
        tsup(20)
    );
    assert!((plant.writes[0][&u(DP)] - 239_125.0).abs() < 1e-3);
    assert!(
        reports.iter().all(|r| r.events.is_empty()),
        "{:?}",
        reports[0].events
    );
}

#[test]
fn bad_reading_holds_last_good_value_and_never_reaches_the_engine() {
    let mut sup = supervisor(FailSafe::HoldLast, None, 288.15);
    let held = sup.tick(&reading(0.0, 1.0)).writes[&u(TSUP)];
    let bad = sup.tick(&reading(60.0, f64::NAN));
    assert!(matches!(&bad.events[0], Event::Degraded { sequence, .. } if sequence == SEQ));
    assert_eq!(bad.writes[&u(TSUP)], held);
    assert_eq!(bad.writes[&active(TSUP)], 1.0);
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
    assert_eq!(bad.writes[&active(TSUP)], 0.0);
    assert!(!bad.writes.contains_key(&u(TSUP)));
}

#[test]
fn hold_last_with_no_good_value_yet_releases() {
    let mut sup = supervisor(FailSafe::HoldLast, None, 288.15);
    let r = sup.tick(&Measurements::from([("time".to_owned(), 0.0)])); // occ missing
    assert_eq!(r.writes[&active(TSUP)], 0.0);
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
        assert_eq!(r.writes[&active(TSUP)], 1.0, "held");
    }
}

#[test]
fn closed_gate_releases_forgets_the_held_value_and_reopens_fresh() {
    let gate = Some(Source::MeasurementAbove {
        point: "plant".into(),
        threshold: 0.5,
    });
    let mut sup = supervisor(FailSafe::HoldLast, gate, 288.15);
    let with = |t: f64, plant: f64| {
        let mut r = reading(t, 1.0);
        r.insert("plant".into(), plant);
        r
    };
    assert_eq!(sup.tick(&with(0.0, 1.0)).writes[&active(TSUP)], 1.0);
    let off = sup.tick(&with(60.0, 0.0));
    assert_eq!(off.writes[&active(TSUP)], 0.0);
    assert!(!off.writes.contains_key(&u(TSUP)));
    // A later failure must not bring back the value from before the gate closed.
    let mut bad = with(120.0, 1.0);
    bad.insert("occ".into(), f64::NAN);
    let bad = sup.tick(&bad);
    assert_eq!(bad.writes[&active(TSUP)], 0.0);
    assert!(!bad.writes.contains_key(&u(TSUP)));
    let back = sup.tick(&with(180.0, 1.0));
    assert_eq!(back.writes[&active(TSUP)], 1.0);
    assert!((back.writes[&u(TSUP)] - 278.71).abs() < 1e-6);
}

#[test]
fn gate_that_cannot_be_evaluated_never_sends_the_fresh_value() {
    let gate = Some(Source::MeasurementAbove {
        point: "plant".into(),
        threshold: 0.5,
    });
    let mut sup = supervisor(FailSafe::HoldLast, gate, 288.15);
    // The sequence steps fine, but the gate's reading is missing: no fresh write.
    let r = sup.tick(&reading(0.0, 1.0));
    assert!(r.outputs.contains_key(SEQ));
    assert_eq!(r.writes[&active(TSUP)], 0.0);
    assert!(!r.writes.contains_key(&u(TSUP)));
}

#[test]
fn out_of_range_output_is_clamped_and_reported() {
    let mut sup = supervisor(FailSafe::HoldLast, None, 278.5);
    let r = sup.tick(&reading(0.0, 1.0));
    assert_eq!(r.writes[&u(TSUP)], 278.5);
    assert!(r.events.iter().any(
        |e| matches!(e, Event::Clamped { point, sent, .. } if point == TSUP && *sent == 278.5)
    ));
}

#[test]
fn binding_to_a_missing_output_is_reported() {
    let seq = sequence(SEQ, bindings());
    let out = vec![binding("nope", TSUP, 0.0, 400.0, None)];
    let mut sup = Supervisor::new(vec![seq], out, FailSafe::HoldLast).unwrap();
    let r = sup.tick(&reading(0.0, 1.0));
    assert!(r.events.contains(&Event::NoSuchOutput {
        sequence: SEQ.into(),
        output: "nope".into()
    }));
    assert_eq!(r.writes[&active(TSUP)], 0.0);
}

/// `b` reads `a`'s Real output into a Boolean input: caught before the engine, as an input
/// error. When `a` degrades, `b` degrades with it.
#[test]
fn chained_sequences_degrade_with_their_upstream() {
    let a = sequence("a", bindings());
    let mut b_inputs = bindings();
    // b has its own enable reading, so only a's failure can reach it.
    b_inputs[1].source = Source::MeasurementAbove {
        point: "occ_b".into(),
        threshold: 0.5,
    };
    b_inputs[2] = InputBinding {
        input: "u1StaPro".into(),
        source: Source::Output {
            sequence: "a".into(),
            output: "TSupSet".into(),
        },
    };
    let b = sequence("b", b_inputs);
    let out = vec![(
        "b".to_owned(),
        OutputBinding {
            output: "TSupSet".into(),
            point: TSUP.into(),
            min: 0.0,
            max: 400.0,
            gate: None,
        },
    )];
    let mut sup = Supervisor::new(vec![a, b], out, FailSafe::HoldLast).unwrap();

    let mut first = reading(0.0, 1.0);
    first.insert("occ_b".into(), 1.0);
    let r = sup.tick(&first);
    assert!(r.outputs.contains_key("a"));
    assert!(!r.outputs.contains_key("b"));
    assert!(r.events.iter().any(|e| matches!(e,
        Event::Degraded { sequence, reason: StepFailure::Input { error: SourceError::WrongType { .. }, .. } }
        if sequence == "b")), "{:?}", r.events);

    let mut second = reading(60.0, f64::NAN);
    second.insert("occ_b".into(), 1.0);
    let r = sup.tick(&second);
    assert!(r.events.iter().any(|e| matches!(e,
        Event::Degraded { sequence, reason: StepFailure::Input { error: SourceError::NoOutput { .. }, .. } }
        if sequence == "b")), "{:?}", r.events);
    assert_eq!(r.writes[&active(TSUP)], 0.0, "b never had a good value");
}
