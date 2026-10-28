//! Muse Spark explains what a repair changed between two attempts (advisory; the receipt is the verdict).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

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

/// A fresh directory per call: `cargo test` runs a file's tests as threads of one process.
fn scratch(tag: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("authoring-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn state(drafts: Option<PathBuf>, pace: Duration) -> AppState {
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(&repo("specs"), &palette).unwrap();
    AppState::build(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: Vec::new(),
                pace: Duration::ZERO,
            },
            records: None,
            replays: None,
            replay_pace: Duration::ZERO,
        },
        AuthoringConfig {
            spark: None,
            recordings: Some(repo("fixtures/recordings")),
            pace,
            drafts,
            explanations: Some(repo("fixtures/explanations")),
            ..AuthoringConfig::default()
        },
    )
}

async fn call(
    state: &AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// The recorded chilled-water session (refused for a loop, then 8 of 10, then 10 of 10): its two
/// repairs are explained from the committed recordings, without a model key.
#[tokio::test]
async fn a_recorded_repair_is_explained_without_a_key() {
    let state = state(Some(scratch("fix")), Duration::ZERO);
    let (status, info) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        Some(json!({"source": "recorded"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    let id = info["id"].as_str().unwrap().to_owned();
    for _ in 0..600 {
        let (_, i) = call(&state, "GET", &format!("/api/authoring/{id}"), None).await;
        if i["state"] != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let uri = |n: u32| format!("/api/authoring/{id}/attempts/{n}/explain");
    let (status, e) = call(&state, "POST", &uri(2), None).await;
    assert_eq!(status, StatusCode::OK, "{e}");
    assert_eq!(e["source"], "recorded");
    assert_eq!(
        (e["before"].as_u64(), e["after"].as_u64()),
        (Some(1), Some(2))
    );
    assert!(e["summary"].as_str().unwrap().contains("loop"), "{e}");
    let (status, e) = call(&state, "POST", &uri(3), None).await;
    assert_eq!(status, StatusCode::OK, "{e}");
    assert!(e["summary"].as_str().unwrap().contains("stageHold"), "{e}");
    // Attempt 1 has nothing before it; attempt 4 was never made.
    for n in [1, 4] {
        let (status, e) = call(&state, "POST", &uri(n), None).await;
        assert_eq!(status, StatusCode::CONFLICT, "{e}");
        assert_eq!(e["code"], "no-repair");
    }
    let (status, _) = call(
        &state,
        "POST",
        "/api/authoring/nope/attempts/2/explain",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Without a key and without a recording of this repair, the server says so.
#[tokio::test]
async fn an_unrecorded_repair_needs_a_model() {
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(&repo("specs"), &palette).unwrap();
    let state = AppState::build(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: Vec::new(),
                pace: Duration::ZERO,
            },
            records: None,
            replays: None,
            replay_pace: Duration::ZERO,
        },
        AuthoringConfig {
            recordings: Some(repo("fixtures/recordings")),
            pace: Duration::ZERO,
            ..AuthoringConfig::default()
        },
    );
    let (_, info) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        Some(json!({"source": "recorded"})),
    )
    .await;
    let id = info["id"].as_str().unwrap().to_owned();
    for _ in 0..600 {
        let (_, i) = call(&state, "GET", &format!("/api/authoring/{id}"), None).await;
        if i["state"] != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, e) = call(
        &state,
        "POST",
        &format!("/api/authoring/{id}/attempts/2/explain"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{e}");
    assert_eq!(e["code"], "no-model");
}
