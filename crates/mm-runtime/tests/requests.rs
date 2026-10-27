//! The air handler request sequence chained into the plant reset, as the live run wires it:
//! three verified request instances run first each tick, and the reset reads their sum.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use common::*;
use mm_bridge::WireValue;
use mm_runtime::{
    FailSafe, InputBinding, Measurements, RunningSequence, Scalar, Source, Supervisor,
};

fn requests(floor: u8) -> RunningSequence {
    let (cxf, receipt) = verified("ahu_chw_requests");
    let read = |input: &str, point: &str| InputBinding {
        input: input.into(),
        source: Source::Measurement {
            point: format!("f{floor}_{point}"),
        },
    };
    let bindings = vec![
        read("TAirSup", "tsup"),
        read("TAirSupSet", "tset"),
        read("uCooCoi", "valve"),
    ];
    RunningSequence::load(&format!("ahu{floor}"), &cxf, &receipt, bindings).unwrap()
}

fn chained() -> Supervisor {
    let sum = Source::SumInt {
        of: (1..=3)
            .map(|f| Source::Output {
                sequence: format!("ahu{f}"),
                output: "yChiWatResReq".into(),
            })
            .collect(),
    };
    let reset = vec![
        InputBinding {
            input: "nReqRes".into(),
            source: sum,
        },
        InputBinding {
            input: "u1Ena".into(),
            source: Source::Constant {
                value: Scalar::Bool(true),
            },
        },
        InputBinding {
            input: "u1StaPro".into(),
            source: Source::Constant {
                value: Scalar::Bool(false),
            },
        },
    ];
    let mut seqs: Vec<_> = (1..=3).map(requests).collect();
    seqs.push(sequence(SEQ, reset));
    Supervisor::new(seqs, outputs(None, 288.15), FailSafe::HoldLast).unwrap()
}

/// Every air handler `error` K too warm, valve half open.
fn readings(time: f64, error: f64) -> Measurements {
    let mut m: Measurements = [("time".to_owned(), time)].into_iter().collect();
    for f in 1..=3 {
        m.insert(format!("f{f}_tset"), 286.0);
        m.insert(format!("f{f}_tsup"), 286.0 + error);
        m.insert(format!("f{f}_valve"), 0.5);
    }
    m
}

/// Supply temperature setpoint written at 1200 s, the reset's first update after enable.
fn tsup_at_first_update(error: f64) -> (f64, WireValue) {
    let mut sup = chained();
    let mut last = None;
    for k in 0..=20 {
        last = Some(sup.tick(&readings(60.0 * f64::from(k), error)));
    }
    let report = last.unwrap();
    assert!(report.events.is_empty(), "{:?}", report.events);
    (
        report.writes[&u(TSUP)],
        report.outputs["ahu2"]["yChiWatResReq"].clone(),
    )
}

#[test]
fn satisfied_air_handlers_let_the_reset_trim() {
    let (tsup, req) = tsup_at_first_update(0.5);
    assert_eq!(req, WireValue::Integer(0));
    assert!((tsup - 278.8476).abs() < 1e-6, "{tsup}");
}

#[test]
fn warm_air_handlers_hold_the_reset_at_design() {
    // Each sends 3 requests after 120 s; 9 in total is well past the 2 ignored, so the
    // respond part (capped at +0.07) outweighs the trim and the reset stays at its ceiling.
    let (tsup, req) = tsup_at_first_update(3.5);
    assert_eq!(req, WireValue::Integer(3));
    assert!((tsup - 278.71).abs() < 1e-6, "{tsup}");
}
