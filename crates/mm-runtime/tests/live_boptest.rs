//! The verified chilled-water plant reset driving BOPTEST live. Ignored by default: it needs a
//! BOPTEST v0.9.0 service (`make boptest-up`) and takes tens of minutes under emulation.
//!
//!   cargo test -p mm-runtime --test live_boptest -- --ignored --nocapture
//!
//! Use `cargo test`, not nextest: nextest's 2 min limit would kill the test mid-initialize and
//! leave the BOPTEST test holding the only worker until its idle timeout.
//!
//! Environment (all optional):
//! - `BOPTEST_URL` (default `http://127.0.0.1:80`);
//! - `MM_LIVE_STEPS`: 120 s steps (default 30);
//! - `MM_LIVE_START`: simulation seconds (default 05:30 on day 185, the Monday after the case's
//!   peak cooling day; day 184 is a Sunday, unoccupied);
//! - `MM_LIVE_WARMUP`: seconds (default one day);
//! - `MM_LIVE_OUT`: path for a JSON run record, relative to the workspace root;
//! - `MM_LIVE_BASELINE=1`: no sequence and no writes (the baseline controllers, same window);
//! - `MM_LIVE_REQUESTS=0`: request count fixed at 0, as in the first live run.
//!
//! By default three instances of the verified `ahu_chw_requests` sequence (one per floor's air
//! handler) run first each tick, and the plant reset reads the sum of their requests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
// Progress goes to stderr: a live run is watched with --nocapture.
#![allow(clippy::print_stderr)]

mod common;

use common::*;
use mm_runtime::preset::{
    self, CASE, FLOORS, REQUESTS_SPEC, RESET, RESET_SPEC, STEP_S as STEP, request_bindings,
    requests_name, reset_bindings,
};
use mm_runtime::{BoptestClient, Event, FailSafe, Plant, PlantError, RunningSequence, Supervisor};
use serde_json::json;

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::test]
#[ignore = "needs BOPTEST (make boptest-up); slow"]
async fn chw_reset_drives_boptest() {
    let url = std::env::var("BOPTEST_URL").unwrap_or_else(|_| "http://127.0.0.1:80".into());
    let steps = env_f64("MM_LIVE_STEPS", 30.0) as usize;
    let start = env_f64("MM_LIVE_START", 185.0 * 86_400.0 + 5.5 * 3_600.0);
    let warmup = env_f64("MM_LIVE_WARMUP", 86_400.0);

    // Baseline mode runs no sequence and writes nothing: the same window under BOPTEST's own
    // controllers, for KPI comparison.
    let baseline = std::env::var("MM_LIVE_BASELINE").is_ok_and(|v| v == "1");
    let mut sup = if baseline {
        Supervisor::new(Vec::new(), Vec::new(), FailSafe::Release).unwrap()
    } else {
        let requests = std::env::var("MM_LIVE_REQUESTS").map_or(true, |v| v != "0");
        let mut sequences = Vec::new();
        if requests {
            let (cxf, receipt) = verified(REQUESTS_SPEC);
            for floor in FLOORS {
                sequences.push(
                    RunningSequence::load(
                        &requests_name(floor),
                        &cxf,
                        &receipt,
                        request_bindings(floor),
                    )
                    .expect("verified request sequence loads"),
                );
            }
        }
        let (cxf, receipt) = verified(RESET_SPEC);
        sequences.push(
            RunningSequence::load(RESET, &cxf, &receipt, reset_bindings(requests))
                .expect("verified sequence loads with live bindings"),
        );
        Supervisor::new(sequences, preset::reset_outputs(), FailSafe::Release).unwrap()
    };

    let mut plant = BoptestClient::select(&url, CASE).await.expect("select");
    let outcome = drive(&mut plant, &mut sup, start, warmup, steps, baseline).await;
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
    let degraded = record["degraded_ticks"].as_u64().unwrap();
    assert_eq!(
        degraded, 0,
        "fail-safe events on {degraded} ticks; see the run record"
    );
}

async fn drive(
    plant: &mut BoptestClient,
    sup: &mut Supervisor,
    start: f64,
    warmup: f64,
    steps: usize,
    baseline: bool,
) -> Result<serde_json::Value, PlantError> {
    let t0 = std::time::Instant::now();
    let mut readings = plant.initialize(start, warmup).await?;
    let init_s = t0.elapsed().as_secs_f64();
    plant.set_step(STEP).await?;
    eprintln!("initialized in {init_s:.0} s; {} points", readings.len());

    let mut ticks = Vec::new();
    let mut degraded = 0u64;
    let t1 = std::time::Instant::now();
    for _ in 0..steps {
        let report = sup.tick(&readings);
        let seen: serde_json::Map<_, _> = preset::watch_points()
            .into_iter()
            .filter_map(|p| {
                let v = *readings.get(&p)?;
                Some((p, json!(v)))
            })
            .collect();
        eprintln!(
            "t={:>9.0} out={:?} seen={}",
            report.time,
            report.outputs,
            serde_json::Value::Object(seen.clone())
        );
        if report
            .events
            .iter()
            .any(|e| !matches!(e, Event::Clamped { .. }))
        {
            degraded += 1;
            eprintln!("events: {:?}", report.events);
        }
        ticks.push(json!({"report": report, "seen": seen}));
        readings = match plant.advance(&report.writes).await {
            Ok(next) => next,
            Err(PlantError::Ended) => break,
            Err(e) => return Err(e),
        };
    }
    let wall = t1.elapsed().as_secs_f64();
    let kpi = match plant.kpi().await {
        Ok(k) => json!(k),
        Err(e) => json!({"error": e.to_string()}),
    };
    Ok(json!({
        "case": CASE,
        "baseline": baseline,
        "start": start,
        "warmup": warmup,
        "step": STEP,
        "init_wall_s": init_s,
        "steps_wall_s": wall,
        "degraded_ticks": degraded,
        "ticks": ticks,
        "kpi": kpi,
    }))
}
