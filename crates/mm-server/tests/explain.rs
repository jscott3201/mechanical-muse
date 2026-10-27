//! Graph explanations without a model key: recorded ones are served (labelled), anything else is
//! refused rather than made up.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mm_graph::Palette;
use mm_server::{AppState, AuthoringConfig, PlantSource, RunsConfig, SpecStore, app};
use serde_json::{Value, json};
use tower::ServiceExt;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn state(explanations: Option<PathBuf>) -> AppState {
    with_replays(explanations, None)
}

fn with_replays(explanations: Option<PathBuf>, replays: Option<PathBuf>) -> AppState {
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(&repo("specs"), &palette).unwrap();
    AppState::build(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: Vec::new(),
                pace: std::time::Duration::ZERO,
            },
            records: None,
            replays,
            replay_pace: std::time::Duration::ZERO,
        },
        AuthoringConfig {
            explanations,
            ..AuthoringConfig::default()
        },
    )
}

async fn explain(state: &AppState, spec: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/specs/{spec}/explain"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
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
async fn a_recorded_explanation_is_served_and_labelled() {
    let state = state(Some(repo("fixtures/explanations")));
    let (status, body) = explain(&state, "chw_plant_reset", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["source"], "recorded");
    assert_eq!(body["spec"], "chw_plant_reset");
    assert!(body["content_id"].as_str().unwrap().starts_with("cxf:"));
    // Every block a part names is a block of our reference graph.
    let graph: Value =
        serde_json::from_slice(&std::fs::read(repo("specs/chw_plant_reset/graph.json")).unwrap())
            .unwrap();
    let ids: Vec<&str> = graph["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    let parts = body["parts"].as_array().unwrap();
    assert!(!parts.is_empty());
    for p in parts {
        for b in p["blocks"].as_array().unwrap() {
            assert!(
                ids.contains(&b.as_str().unwrap()),
                "{b} is not in the graph"
            );
        }
    }
}

#[tokio::test]
async fn without_a_model_or_a_recording_it_is_refused() {
    let state = state(None);
    let (status, body) = explain(&state, "chw_plant_reset", json!({})).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "no-model");
}

#[tokio::test]
async fn an_unknown_draft_or_spec_is_not_found() {
    let state = state(Some(repo("fixtures/explanations")));
    let (status, body) = explain(
        &state,
        "chw_plant_reset",
        json!({"graph": "cxf:fnv1a128:0"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "unknown-draft");
    let (status, _) = explain(&state, "nope", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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

/// The fault demo's events, replayed, have a committed explanation: without a key it is served,
/// labelled recorded. This also pins the event context: if its format changes, the digest does
/// too and this fails rather than the demo silently losing its explanation.
#[tokio::test]
async fn a_replayed_runs_events_have_a_recorded_explanation() {
    let state = with_replays(
        Some(repo("fixtures/explanations")),
        Some(repo("fixtures/replays")),
    );
    let (status, run) = call(
        &state,
        "POST",
        "/api/runs/replay",
        Some(json!({"record": "day186-midnight-fault-demo"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_owned();
    for _ in 0..500 {
        let (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
        if info["state"] == "completed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let (status, body) = call(&state, "POST", &format!("/api/runs/{id}/explain"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["source"], "recorded");
    assert_eq!(body["key"], "run:b516292f3857089b");
    assert!(!body["items"].as_array().unwrap().is_empty());
    let (status, body) = call(&state, "POST", "/api/runs/nope/explain", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
