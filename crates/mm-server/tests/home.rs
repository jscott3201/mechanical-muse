//! The home on a scripted plant: a run with `building: home` loads the price-response sequence
//! from its receipt, reads the forecast values the runtime adds, writes the zone setpoint, and
//! hands the setpoint back to BOPTEST when a forecast value is missing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mm_graph::Palette;
use mm_runtime::Measurements;
use mm_runtime::home::{CEILING, FLOOR, PRICE, add_forecast};
use mm_server::{AppState, PlantSource, RunsConfig, SpecStore, app};
use serde_json::{Value, json};
use tower::ServiceExt;

/// Occupied (21 to 24 degC), the room at 21.2 degC, and hourly prices with a peak `peak_in`
/// hours ahead (0.25 per kWh, 0.125 otherwise).
fn readings(time: f64, peak_in: usize) -> Measurements {
    let mut m: Measurements = [
        ("time".to_owned(), time),
        ("reaTZon_y".to_owned(), 294.35),
        ("reaPHeaPum_y".to_owned(), 0.0),
        ("reaPFan_y".to_owned(), 0.0),
        ("reaPPumEmi_y".to_owned(), 0.0),
    ]
    .into_iter()
    .collect();
    let mut prices = vec![0.125; 24];
    prices[peak_in] = 0.25;
    let forecast = BTreeMap::from([
        (PRICE.to_owned(), prices),
        (FLOOR.to_owned(), vec![294.15; 24]),
        (CEILING.to_owned(), vec![297.15; 24]),
    ]);
    add_forecast(&mut m, &forecast);
    m
}

fn state(records: PathBuf, script: Vec<Measurements>) -> AppState {
    let palette = Palette::pinned().unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../specs");
    let specs = SpecStore::load(&dir, &palette).unwrap();
    AppState::with_runs(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: script,
                pace: std::time::Duration::ZERO,
            },
            records: Some(records),
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

async fn finish(state: &AppState, id: &str) -> Value {
    let mut info = Value::Null;
    for _ in 0..600 {
        (_, info) = call(state, "GET", &format!("/api/runs/{id}"), None).await;
        if !matches!(
            info["state"].as_str(),
            Some("starting" | "running" | "verifying")
        ) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    info
}

fn events(dir: &std::path::Path, id: &str) -> Vec<Value> {
    std::fs::read_to_string(dir.join(format!("{id}.jsonl")))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[tokio::test]
async fn the_home_preheats_then_coasts() {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("home-{}", std::process::id()));
    // Peak 2 h ahead, then 1 h ahead, then now, then gone.
    let script = vec![
        readings(0.0, 2),
        readings(900.0, 1),
        readings(1800.0, 0),
        readings(2700.0, 12),
    ];
    let state = state(dir.clone(), script);
    let (status, run) = call(
        &state,
        "POST",
        "/api/runs",
        Some(json!({"building": "home", "steps": 4})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["config"]["case"], "bestest_hydronic_heat_pump");
    assert_eq!(run["config"]["step_s"], 900.0);
    assert_eq!(run["config"]["requests"], false);
    let id = run["id"].as_str().unwrap().to_owned();
    let info = finish(&state, &id).await;
    assert_eq!(info["state"], "completed", "{info}");

    let events = events(&dir, &id);
    let started = events.iter().find(|e| e["kind"] == "started").unwrap();
    assert_eq!(started["sequences"][0]["spec"], "home_price_response");
    let sent: Vec<f64> = events
        .iter()
        .filter(|e| e["kind"] == "tick")
        .map(|e| {
            assert_eq!(e["writes"]["oveTSet_activate"], 1.0, "{e}");
            e["writes"]["oveTSet_u"].as_f64().unwrap()
        })
        .collect();
    // Preheat (capped 2 K under the 24 degC ceiling), preheat, coast, normal: the spec's values.
    let expected = [295.15, 295.15, 294.25, 294.35];
    assert_eq!(sent.len(), expected.len());
    for (got, want) in sent.iter().zip(expected) {
        assert!((got - want).abs() < 1e-9, "sent {sent:?}");
    }
    // The run ends with its peak-price power figure, before the end event.
    let peak = events.iter().find(|e| e["kind"] == "peak-power").unwrap();
    assert_eq!(peak["peak"]["steps"], 4);
    assert_eq!(peak["peak"]["mean_kw"], 0.0, "{peak}");
    assert_eq!(info["peak"]["steps"], 4, "{info}");
    // The price now and the room are recorded for the charts.
    let first = events.iter().find(|e| e["kind"] == "tick").unwrap();
    assert_eq!(
        first["readings"]["forecast:PriceElectricPowerHighlyDynamic:+0h"],
        0.125
    );
    assert_eq!(first["readings"]["reaTZon_y"], 294.35);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_missing_forecast_hands_the_setpoint_back() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("home-missing-{}", std::process::id()));
    let mut gap = readings(900.0, 2);
    gap.remove("forecast:PriceElectricPowerHighlyDynamic:+23h");
    let state = state(
        dir.clone(),
        vec![readings(0.0, 2), gap, readings(1800.0, 2)],
    );
    let (status, run) = call(
        &state,
        "POST",
        "/api/runs",
        Some(json!({"building": "home", "steps": 3})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_owned();
    assert_eq!(finish(&state, &id).await["state"], "completed");
    let ticks: Vec<Value> = events(&dir, &id)
        .into_iter()
        .filter(|e| e["kind"] == "tick")
        .collect();
    assert_eq!(ticks[0]["writes"]["oveTSet_activate"], 1.0);
    // The gap: the sequence degrades and the point goes back to BOPTEST's own controller.
    assert_eq!(ticks[1]["writes"]["oveTSet_activate"], 0.0, "{}", ticks[1]);
    let kinds: Vec<&str> = ticks[1]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"degraded") && kinds.contains(&"released"),
        "{kinds:?}"
    );
    assert_eq!(ticks[2]["writes"]["oveTSet_activate"], 1.0);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn office_options_are_refused_for_the_home() {
    let state = state(PathBuf::from(env!("CARGO_TARGET_TMPDIR")), Vec::new());
    for body in [
        json!({"building": "home", "requests": true}),
        json!({"building": "home", "pressure": true}),
        json!({"building": "home", "fault": {"point": "floor1_reaAHU_TSup_y", "from_step": 0, "steps": 1}}),
        json!({"building": "home", "drafts": {"chw_plant_reset": "abc"}}),
    ] {
        let (status, reply) = call(&state, "POST", "/api/runs", Some(body.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {reply}");
    }
}

/// A verified draft of the home spec runs on the home (the start check knows the building).
#[tokio::test]
async fn a_home_draft_runs_on_the_home() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("home-draft-{}", std::process::id()));
    let state = state(dir.clone(), vec![readings(0.0, 2), readings(900.0, 1)]);
    let spec = "home_price_response";
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs")
        .join(spec)
        .join("graph.json");
    let graph: mm_graph::BlockGraph =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (_, report) = state
        .spec(spec)
        .unwrap()
        .replay_graph(&graph, state.palette())
        .unwrap();
    let id = report.content_id.clone();
    state.authoring().drafts.insert(mm_server::drafts::Draft {
        info: mm_server::author_dto::DraftInfo {
            content_id: id.clone(),
            spec: spec.into(),
            scenarios: 1,
            model: "muse-spark-1.3".into(),
            source: mm_server::author_dto::ModelSource::Recorded,
            session: "test".into(),
            attempts_used: 1,
            stored_seq: 0,
        },
        graph,
    });
    let (status, run) = call(
        &state,
        "POST",
        "/api/runs",
        Some(json!({"building": "home", "steps": 2, "drafts": {spec: id}})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let run_id = run["id"].as_str().unwrap().to_owned();
    assert_eq!(finish(&state, &run_id).await["state"], "completed");
    let started = events(&dir, &run_id)
        .into_iter()
        .find(|e| e["kind"] == "started")
        .unwrap();
    assert_eq!(started["config"]["drafts"][spec], json!(id));
    assert_eq!(started["sequences"][0]["origin"], "drafted", "{started}");
    let _ = std::fs::remove_dir_all(dir);
}
