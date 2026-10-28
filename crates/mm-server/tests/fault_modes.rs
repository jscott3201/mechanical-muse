//! Stuck and biased test faults on a scripted plant: the sequences see the faulted reading,
//! BOPTEST is sent nothing for it, and every faulted tick says so.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mm_graph::Palette;
use mm_runtime::Measurements;
use mm_server::{AppState, PlantSource, RunsConfig, SpecStore, app};
use serde_json::{Value, json};
use tower::ServiceExt;

/// Supply air at setpoint + 0.55 K everywhere; floor 1's valve at 40 %, floors 2 and 3 wide
/// open (one request each, two in all: the reset ignores up to two, so it still trims).
fn readings(time: f64) -> Measurements {
    let mut m: Measurements = [("time".to_owned(), time)].into_iter().collect();
    for f in 1..=3 {
        m.insert(format!("floor{f}_reaAHU_TSup_y"), 286.5);
        m.insert(format!("floor{f}_TSupAirSet_u"), 285.95);
        let valve = if f == 1 { 0.4 } else { 0.97 };
        m.insert(format!("floor{f}_reaAHU_yCooVal_y"), valve);
        m.insert(format!("floor{f}_reaAHU_occ_y"), 1.0);
    }
    m
}

fn state() -> AppState {
    let palette = Palette::pinned().unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../specs");
    let specs = SpecStore::load(&dir, &palette).unwrap();
    let script = (0..=60).map(|k| readings(120.0 * f64::from(k))).collect();
    AppState::with_runs(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: script,
                pace: std::time::Duration::ZERO,
            },
            records: None,
            replays: None,
            replay_pace: std::time::Duration::ZERO,
        },
    )
}

async fn call(
    state: &AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
    let mut req = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let response = app(state.clone())
        .oneshot(req.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

/// The tick payloads of a finished run.
async fn ticks(state: &AppState, body: Value) -> Vec<Value> {
    let (status, info) = call(state, "POST", "/api/runs", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    let id = serde_json::from_str::<Value>(&info).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let text = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        call(state, "GET", &format!("/api/runs/{id}/events"), None)
            .await
            .1
    })
    .await
    .expect("the run did not end");
    text.split("\n\n")
        .filter(|c| c.contains("event: tick"))
        .filter_map(|c| c.lines().find_map(|l| l.strip_prefix("data:")))
        .map(|d| serde_json::from_str(d.trim()).unwrap())
        .collect()
}

fn requests(t: &Value) -> i64 {
    (1..=3)
        .map(|f| {
            t["outputs"][format!("ahu{f}")]["yChiWatResReq"]
                .as_i64()
                .unwrap_or(0)
        })
        .sum()
}

fn tsup_set(t: &Value) -> f64 {
    t["writes"]["oveChiWatSys_TWSet_u"].as_f64().unwrap()
}

/// A cooling valve read as stuck wide open looks like an air handler that needs colder water:
/// its request is the third, over the ignored count, and the plant reset moves toward its cold
/// bound while the healthy run trims warmer.
#[tokio::test]
async fn a_valve_held_open_raises_requests_and_cools_the_plant_reset() {
    let state = state();
    let healthy = ticks(&state, json!({"steps": 50})).await;
    let fault = json!({"point": "floor1_reaAHU_yCooVal_y", "from_step": 0, "steps": 50,
        "mode": {"kind": "hold", "value": 1.0}});
    let faulted = ticks(&state, json!({"steps": 50, "fault": fault})).await;
    assert_eq!((healthy.len(), faulted.len()), (50, 50));

    assert!(healthy.iter().all(|t| requests(t) == 2));
    assert!(faulted.iter().all(|t| requests(t) == 3), "the stuck valve adds one");
    let (last_h, last_f) = (tsup_set(&healthy[49]), tsup_set(&faulted[49]));
    assert!(last_f < last_h, "faulted {last_f} vs healthy {last_h}");
    assert!((278.15..=288.15).contains(&last_f));

    for t in &faulted {
        let injected = &t["events"][0];
        assert_eq!(injected["kind"], "injected");
        assert_eq!(injected["point"], "floor1_reaAHU_yCooVal_y");
        assert_eq!(injected["value"], 1.0);
        assert_eq!(injected["reason"], "test fault: reading held at 1");
        assert_eq!(
            t["readings"]["floor1_reaAHU_yCooVal_y"], 1.0,
            "what the sequences saw"
        );
        let writes = t["writes"].as_object().unwrap();
        assert!(
            writes.keys().all(|k| !k.contains("yCooVal")),
            "a reading fault writes nothing for the reading: {writes:?}"
        );
    }
}

#[tokio::test]
async fn a_bias_offsets_the_reading_and_old_faults_still_mean_nan() {
    let state = state();
    let fault = json!({"point": "floor2_reaAHU_TSup_y", "from_step": 1, "steps": 2,
        "mode": {"kind": "bias", "offset": -1.5}});
    let t = ticks(&state, json!({"steps": 4, "fault": fault})).await;
    assert_eq!(t[1]["readings"]["floor2_reaAHU_TSup_y"], 285.0);
    assert_eq!(
        t[1]["events"][0]["reason"],
        "test fault: reading biased by -1.5"
    );
    assert_eq!(
        t[3]["readings"]["floor2_reaAHU_TSup_y"], 286.5,
        "fault over"
    );

    // A fault without a mode (every record and client from before modes) is a NaN.
    let fault = json!({"point": "floor2_reaAHU_TSup_y", "from_step": 1, "steps": 1});
    let t = ticks(&state, json!({"steps": 3, "fault": fault})).await;
    assert_eq!(
        t[1]["events"][0]["reason"],
        "test fault: reading replaced with NaN"
    );
    assert!(t[1]["events"][0].get("value").is_none());
    assert!(t[1]["readings"].get("floor2_reaAHU_TSup_y").is_none());
}

#[tokio::test]
async fn unknown_fault_modes_are_refused() {
    let state = state();
    for mode in [
        json!({"kind": "stuck", "value": 1.0}),
        json!({"kind": "hold"}),
    ] {
        let fault =
            json!({"point": "floor1_reaAHU_TSup_y", "from_step": 0, "steps": 2, "mode": mode});
        let (status, body) = call(
            &state,
            "POST",
            "/api/runs",
            Some(json!({"steps": 3, "fault": fault})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{mode}: {body}");
    }
}
