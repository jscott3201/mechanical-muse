//! The read-only BACnet mirror during a run on a scripted plant: its objects, and the values a
//! live run leaves on them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::net::Ipv4Addr;
use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mm_bacnet::{Mirror, MirrorConfig, quantize};
use mm_graph::Palette;
use mm_runtime::Measurements;
use mm_server::bacnet::{BacnetMirror, info, point_map};
use mm_server::{AppState, PlantSource, RunsConfig, SpecStore, app};
use serde_json::{Value, json};
use tower::ServiceExt;

fn readings(time: f64) -> Measurements {
    let mut m: Measurements = [("time".to_owned(), time)].into_iter().collect();
    for f in 1..=3 {
        m.insert(format!("floor{f}_reaAHU_TSup_y"), 286.5);
        m.insert(format!("floor{f}_TSupAirSet_u"), 285.95);
        m.insert(format!("floor{f}_reaAHU_yCooVal_y"), 0.4);
        m.insert(format!("floor{f}_reaAHU_occ_y"), 1.0);
    }
    m.insert("reaChiWatSys_TCHWSup_y".into(), 278.7);
    m
}

fn state() -> AppState {
    with_replays(None)
}

fn with_replays(replays: Option<PathBuf>) -> AppState {
    let palette = Palette::pinned().unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../specs");
    let specs = SpecStore::load(&dir, &palette).unwrap();
    let script = (0..=40).map(|k| readings(60.0 * f64::from(k))).collect();
    AppState::with_runs(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: script,
                pace: std::time::Duration::ZERO,
            },
            records: None,
            replays,
            replay_pace: std::time::Duration::ZERO,
        },
    )
}

async fn call(
    state: &AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let response = app(state.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn a_live_run_updates_the_mirror() {
    let state = state();
    let (status, _) = call(&state, "GET", "/api/bacnet", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "off until started");

    attach(&state).await;

    let (status, body) = call(&state, "GET", "/api/bacnet", None).await;
    assert_eq!(status, StatusCode::OK);
    let points = body["points"].as_array().unwrap();
    // 18 watched readings and 6 for the pressure resets (duct pressure and fan power per
    // floor); 8 chilled-water sequence outputs (3 floors x 2 + 2) and 18 pressure ones (15
    // zones + 3 floors); 2 plant setpoints and 3 floor static pressure setpoints sent.
    let count = |s: &str| points.iter().filter(|p| p["source"] == s).count();
    assert_eq!(
        (count("measured"), count("derived"), count("commanded")),
        (24, 26, 5)
    );

    let (status, run) = call(&state, "POST", "/api/runs", Some(json!({"steps": 25}))).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_owned();
    loop {
        let (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
        if info["state"] == "completed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // After the run every point keeps its last value but reads as no data: nothing produces
    // values any more.
    let mirror = &state.bacnet().unwrap().mirror;
    assert_eq!(
        mirror.value("reaChiWatSys_TCHWSup_y").await,
        Some((quantize(278.7), false))
    );
    // The last tick (1440 s) still holds the first trim (1200 s); the next is at 1500 s.
    let (sent, reliable) = mirror.value("oveChiWatSys_TWSet_u").await.unwrap();
    assert!(!reliable);
    assert!((f64::from(sent) - 278.8476).abs() < 1e-4, "{sent}");
    assert_eq!(mirror.value("chw.TSupSet").await.map(|v| v.0), Some(sent));
    assert_eq!(mirror.value("ahu1.yChiWatResReq").await, Some((0.0, false)));
}

async fn attach(state: &AppState) {
    let map = point_map(state.specs()).unwrap();
    let config = MirrorConfig {
        device_instance: 599,
        device_name: "MM-TEST".into(),
        interface: Ipv4Addr::LOCALHOST,
        port: 0,
        broadcast: Ipv4Addr::LOCALHOST,
    };
    let mirror = Mirror::start(&config, map.clone()).await.unwrap();
    state.set_bacnet(BacnetMirror {
        mirror,
        info: info(&map, 599, 0),
    });
}

#[tokio::test]
async fn a_replay_never_updates_the_mirror() {
    let replays = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/replays");
    let state = with_replays(Some(replays));
    attach(&state).await;
    let (status, run) = call(
        &state,
        "POST",
        "/api/runs/replay",
        Some(json!({"record": "day185-noon-live-spark-draft"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_owned();
    loop {
        let (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
        if info["state"] == "completed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let mirror = &state.bacnet().unwrap().mirror;
    for name in [
        "reaChiWatSys_TCHWSup_y",
        "oveChiWatSys_TWSet_u",
        "chw.TSupSet",
    ] {
        assert_eq!(mirror.value(name).await, Some((0.0, false)), "{name}");
    }
}
