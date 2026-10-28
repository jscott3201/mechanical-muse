//! A run with the duct static pressure resets on a scripted plant: every zone and floor
//! sequence loads from its receipt, the floors' setpoints are written and trim on schedule.

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

const ZONES: [&str; 5] = ["Cor", "Sou", "Eas", "Nor", "Wes"];

/// Occupied floors with fans running and every zone at its airflow setpoint with dampers half
/// open: no pressure requests, so each floor's setpoint trims after its start delay.
fn readings(time: f64) -> Measurements {
    let mut m: Measurements = [("time".to_owned(), time)].into_iter().collect();
    for f in 1..=3 {
        m.insert(format!("floor{f}_reaAHU_TSup_y"), 286.5);
        m.insert(format!("floor{f}_TSupAirSet_u"), 285.95);
        m.insert(format!("floor{f}_reaAHU_yCooVal_y"), 0.4);
        m.insert(format!("floor{f}_reaAHU_occ_y"), 1.0);
        m.insert(format!("floor{f}_reaAHU_V_flowSup_y"), 5.0);
        for z in ZONES {
            m.insert(format!("floor{f}_reaZon{z}_yDam_y"), 0.5);
            m.insert(format!("floor{f}_reaZon{z}_V_flow_y"), 0.4);
            m.insert(format!("floor{f}_reaZon{z}_V_flowSet_y"), 0.4);
        }
    }
    m.insert("reaChiWatSys_TCHWSup_y".into(), 278.7);
    m
}

fn state(records: PathBuf) -> AppState {
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

#[tokio::test]
async fn the_pressure_resets_load_write_and_trim() {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("pressure-{}", std::process::id()));
    let state = state(dir.clone());
    let (status, run) = call(
        &state,
        "POST",
        "/api/runs",
        Some(json!({"steps": 25, "pressure": true})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["config"]["pressure"], true);
    let id = run["id"].as_str().unwrap().to_owned();
    let mut info = Value::Null;
    for _ in 0..600 {
        (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
        if info["state"] != "starting" && info["state"] != "running" && info["state"] != "verifying"
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(info["state"], "completed", "{info}");

    let events: Vec<Value> = std::fs::read_to_string(dir.join(format!("{id}.jsonl")))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let started = events.iter().find(|e| e["kind"] == "started").unwrap();
    let specs: Vec<&str> = started["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["spec"].as_str().unwrap())
        .collect();
    // 3 air handler requests, the plant reset, 15 zones and 3 floor resets.
    assert_eq!(specs.len(), 22, "{specs:?}");
    assert_eq!(
        specs
            .iter()
            .filter(|s| **s == "zone_pressure_requests")
            .count(),
        15
    );
    assert_eq!(
        specs
            .iter()
            .filter(|s| **s == "ahu_static_pressure_reset")
            .count(),
        3
    );

    let tick = |t: f64| {
        events
            .iter()
            .find(|e| e["kind"] == "tick" && e["sim_time_s"] == t)
            .unwrap()
    };
    for f in 1..=3 {
        let point = format!("floor{f}_dpSet_u");
        assert_eq!(tick(0.0)["writes"][&point], 400.0, "{point} at 0 s");
        assert_eq!(tick(660.0)["writes"][&point], 400.0, "{point} at 660 s");
        // The first trim lands at the 720 s update, as in the spec's vectors.
        assert_eq!(tick(720.0)["writes"][&point], 388.0, "{point} at 720 s");
        assert_eq!(tick(840.0)["writes"][&point], 376.0, "{point} at 840 s");
    }
    assert_eq!(tick(0.0)["outputs"]["zone2eas"]["yPreReq"], 0);
    assert!(
        events
            .iter()
            .filter(|e| e["kind"] == "tick")
            .all(|e| e["events"].as_array().unwrap().is_empty()),
        "no sequence degraded"
    );
    let _ = std::fs::remove_dir_all(dir);
}

fn reference(spec: &str) -> mm_graph::BlockGraph {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs")
        .join(spec)
        .join("graph.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// The pressure sequences load verified drafts when `pressure` is on, so a run must accept
/// drafts for them then, and refuse them when the run does not use them.
#[tokio::test]
async fn pressure_drafts_run_when_pressure_is_on() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("pressure-drafts-{}", std::process::id()));
    let state = state(dir.clone());
    let mut drafts = serde_json::Map::new();
    for spec in ["ahu_static_pressure_reset", "zone_pressure_requests"] {
        let graph = reference(spec);
        let (_, report) = state
            .spec(spec)
            .unwrap()
            .replay_graph(&graph, state.palette())
            .unwrap();
        drafts.insert(spec.into(), json!(report.content_id));
        state.authoring().drafts.insert(mm_server::drafts::Draft {
            info: mm_server::author_dto::DraftInfo {
                content_id: report.content_id.clone(),
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
    }
    let (status, body) = call(
        &state,
        "POST",
        "/api/runs",
        Some(json!({"steps": 3, "drafts": drafts})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"].as_str().unwrap().contains("does not use"),
        "{body}"
    );

    let (status, run) = call(
        &state,
        "POST",
        "/api/runs",
        Some(json!({"steps": 3, "pressure": true, "drafts": drafts})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_owned();
    let mut info = Value::Null;
    for _ in 0..600 {
        (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
        if info["state"] != "starting" && info["state"] != "running" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(info["state"], "completed", "{info}");
    let events: Vec<Value> = std::fs::read_to_string(dir.join(format!("{id}.jsonl")))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let started = events.iter().find(|e| e["kind"] == "started").unwrap();
    for s in started["sequences"].as_array().unwrap() {
        let drafted = matches!(
            s["spec"].as_str().unwrap(),
            "ahu_static_pressure_reset" | "zone_pressure_requests"
        );
        assert_eq!(s["origin"] == "drafted", drafted, "{s}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
