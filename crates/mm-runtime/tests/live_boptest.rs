//! The verified chilled-water plant reset driving BOPTEST live. Ignored by default: it needs a
//! BOPTEST v0.9.0 service (`make boptest-up`) and takes tens of minutes under emulation.
//!
//!   cargo test -p mm-runtime --test live_boptest -- --ignored --nocapture
//!
//! Environment (all optional): `BOPTEST_URL` (default `http://127.0.0.1:80`), `MM_LIVE_STEPS`
//! (120 s steps, default 30), `MM_LIVE_START` (simulation seconds, default 05:30 on day 185, the
//! Monday after the case's peak cooling day; day 184 is a Sunday, unoccupied), `MM_LIVE_WARMUP` (seconds, default one day), `MM_LIVE_OUT` (path
//! for a JSON run record, relative to the workspace root).
//!
//! No reset requests are wired yet (the AHU request sequence comes next), so the request count
//! is a constant 0: after the start delay the reset trims, warming the supply water first.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
// Progress goes to stderr: a live run is watched with --nocapture.
#![allow(clippy::print_stderr)]

mod common;

use common::*;
use mm_runtime::{
    BoptestClient, Event, FailSafe, InputBinding, Plant, PlantError, Scalar, Source, Supervisor,
};
use serde_json::json;

const CASE: &str = "multizone_office_complex_air";
const STEP: f64 = 120.0;
const WATCH: [&str; 6] = [
    "reaChiWatSys_TCHWSup_y",
    "reaChiWatSys_dp_y",
    "reaChiWatSys_mCHWTot_y",
    "reaChiWatSys_reaPChi_y",
    "reaChiWatSys_reaPPum_y",
    "floor2_reaAHU_occ_y",
];

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn live_bindings() -> Vec<InputBinding> {
    let occupied = |floor: u8| Source::MeasurementAbove {
        point: format!("floor{floor}_reaAHU_occ_y"),
        threshold: 0.5,
    };
    vec![
        InputBinding {
            input: "nReqRes".into(),
            source: Source::Constant {
                value: Scalar::Int(0),
            },
        },
        InputBinding {
            input: "u1Ena".into(),
            source: Source::AnyTrue {
                of: vec![occupied(1), occupied(2), occupied(3)],
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

#[tokio::test]
#[ignore = "needs BOPTEST (make boptest-up); slow"]
async fn chw_reset_drives_boptest() {
    let url = std::env::var("BOPTEST_URL").unwrap_or_else(|_| "http://127.0.0.1:80".into());
    let steps = env_f64("MM_LIVE_STEPS", 30.0) as usize;
    let start = env_f64("MM_LIVE_START", 185.0 * 86_400.0 + 5.5 * 3_600.0);
    let warmup = env_f64("MM_LIVE_WARMUP", 86_400.0);

    let cxf = cxf();
    let seq = mm_runtime::RunningSequence::load(SEQ, &cxf, &receipt(&cxf), live_bindings())
        .expect("verified sequence loads with live bindings");
    let mut sup = Supervisor::new(vec![seq], outputs(None, 288.15), FailSafe::Release).unwrap();

    let mut plant = BoptestClient::select(&url, CASE).await.expect("select");
    let outcome = drive(&mut plant, &mut sup, start, warmup, steps).await;
    // Free the worker whatever happened; v0.9.0 may answer stop with a non-JSON body.
    if let Err(e) = plant.stop().await {
        eprintln!("stop: {e}");
    }
    let record = outcome.expect("live run");

    if let Ok(out) = std::env::var("MM_LIVE_OUT") {
        // cargo runs tests from the crate directory; resolve relative paths from the workspace.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(out);
        std::fs::write(&path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
        eprintln!("run record: {}", path.display());
    }
    let ticks = record["ticks"].as_array().unwrap();
    assert!(!ticks.is_empty(), "no ticks ran");
    let degraded = ticks
        .iter()
        .flat_map(|t| t["events"].as_array().unwrap())
        .filter(|e| e["event"] != "clamped")
        .count();
    assert_eq!(
        degraded, 0,
        "unexpected fail-safe events; see the run record"
    );
}

async fn drive(
    plant: &mut BoptestClient,
    sup: &mut Supervisor,
    start: f64,
    warmup: f64,
    steps: usize,
) -> Result<serde_json::Value, PlantError> {
    let t0 = std::time::Instant::now();
    let mut readings = plant.initialize(start, warmup).await?;
    let init_s = t0.elapsed().as_secs_f64();
    plant.set_step(STEP).await?;
    eprintln!("initialized in {init_s:.0} s; {} points", readings.len());

    let mut ticks = Vec::new();
    let t1 = std::time::Instant::now();
    for _ in 0..steps {
        let report = sup.tick(&readings);
        let seen: serde_json::Map<_, _> = WATCH
            .iter()
            .filter_map(|p| Some(((*p).to_owned(), json!(readings.get(*p)?))))
            .collect();
        eprintln!(
            "t={:>9.0} out={:?} seen={}",
            report.time,
            report.outputs.get(SEQ),
            serde_json::Value::Object(seen.clone())
        );
        let has_bad = report
            .events
            .iter()
            .any(|e| !matches!(e, Event::Clamped { .. }));
        ticks.push(json!({"report": report, "seen": seen, "events": report.events}));
        if has_bad {
            eprintln!("events: {:?}", report.events);
        }
        readings = match plant.advance(&report.writes).await {
            Ok(next) => next,
            Err(PlantError::Ended) => break,
            Err(e) => return Err(e),
        };
    }
    let wall = t1.elapsed().as_secs_f64();
    let kpi = plant.kpi().await.unwrap_or_default();
    Ok(json!({
        "case": CASE,
        "start": start,
        "warmup": warmup,
        "step": STEP,
        "init_wall_s": init_s,
        "steps_wall_s": wall,
        "ticks": ticks,
        "kpi": kpi,
    }))
}
