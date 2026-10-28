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

/// The shared demo wiring (`preset`) loads against the real specs and steps without events on
/// plausible readings, so a renamed input or point fails here, not in a live run.
#[test]
fn preset_wiring_loads_and_steps() {
    use mm_runtime::preset;
    let (req_cxf, req_receipt) = verified(preset::REQUESTS_SPEC);
    let mut seqs: Vec<_> = preset::FLOORS
        .iter()
        .map(|f| {
            RunningSequence::load(
                &preset::requests_name(*f),
                &req_cxf,
                &req_receipt,
                preset::request_bindings(*f),
            )
            .unwrap()
        })
        .collect();
    let (cxf, receipt) = verified(preset::RESET_SPEC);
    seqs.push(
        RunningSequence::load(preset::RESET, &cxf, &receipt, preset::reset_bindings(true)).unwrap(),
    );
    let mut sup = Supervisor::new(seqs, preset::reset_outputs(), FailSafe::Release).unwrap();
    let mut m: Measurements = [("time".to_owned(), 0.0)].into_iter().collect();
    for f in preset::FLOORS {
        m.insert(format!("floor{f}_reaAHU_TSup_y"), 286.5);
        m.insert(format!("floor{f}_TSupAirSet_u"), 285.95);
        m.insert(format!("floor{f}_reaAHU_yCooVal_y"), 0.4);
        m.insert(format!("floor{f}_reaAHU_occ_y"), 1.0);
    }
    let report = sup.tick(&m);
    assert!(report.events.is_empty(), "{:?}", report.events);
    assert_eq!(report.writes[&format!("{}_u", preset::TSUP_POINT)], 278.71);
    assert_eq!(
        report.writes[&format!("{}_activate", preset::DP_POINT)],
        1.0
    );
    for p in preset::watch_points() {
        assert!(p.ends_with("_y") || p.ends_with("_u"), "{p}");
    }
    // Energy points are BOPTEST measurements in W, none already watched.
    let csv = include_str!("../../../docs/boptest-points.csv");
    let watched = preset::watch_points();
    for p in preset::energy_points() {
        assert!(!watched.contains(&p), "{p}");
        let row = csv
            .lines()
            .find(|l| l.split(',').next() == Some(p.as_str()))
            .unwrap_or_else(|| panic!("{p} is not a BOPTEST point"));
        assert!(row.contains(",measurement,W,"), "{row}");
    }
    // Watched plus energy points are exactly BOPTEST's 14 KPI power signals, so a recorded run
    // can be split by end use.
    let mut recorded: Vec<String> = watched
        .into_iter()
        .chain(preset::energy_points())
        .filter(|p| p.contains("reaP") || p.contains("_reaAHU_P"))
        .collect();
    recorded.sort();
    assert_eq!(recorded, mm_runtime::ledger::kpi_points());
    assert_eq!(recorded.len(), 14);
}
