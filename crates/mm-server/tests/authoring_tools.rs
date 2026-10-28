//! Authoring with Spark's own checks (function tools), through the router, with a recorded
//! session as the model: tool calls appear as their own events, labelled apart from the
//! receipt, and the receipt still decides.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mm_agent::{RecordedCall, RecordedReply, Recording, Usage};
use mm_graph::Palette;
use mm_server::{AppState, AuthoringConfig, PlantSource, RunsConfig, SpecStore, app};
use serde_json::{Value, json};
use tower::ServiceExt;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn state(recordings: &Path) -> AppState {
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
            recordings: Some(recordings.to_path_buf()),
            pace: Duration::ZERO,
            ..AuthoringConfig::default()
        },
    )
}

async fn call(state: &AppState, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app(state.clone()).oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn events(state: &AppState, id: &str) -> Vec<Value> {
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/authoring/{id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = tokio::time::timeout(Duration::from_secs(60), response.into_body().collect())
        .await
        .expect("the stream did not end within 60 s")
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .split("\n\n")
        .filter_map(|chunk| {
            let data = chunk.lines().find_map(|l| l.strip_prefix("data:"))?;
            serde_json::from_str(data.trim()).ok()
        })
        .collect()
}

/// The reference graph with a wrong respond cap: it loads, but fails a scenario.
fn graphs() -> (String, String) {
    let right = std::fs::read_to_string(repo("specs/chw_plant_reset/graph.json")).unwrap();
    let mut wrong: Value = serde_json::from_str(&right).unwrap();
    for block in wrong["blocks"].as_array_mut().unwrap() {
        if block["id"] == "respCapValue" {
            block["params"][0]["value"] = json!(0.10);
        }
    }
    (right, wrong.to_string())
}

fn write_recordings(dir: &Path) {
    let (right, wrong) = graphs();
    let args = |graph: &str, scenario: &str| {
        json!({"graph": serde_json::from_str::<Value>(graph).unwrap(), "scenario": scenario})
            .to_string()
    };
    let recording = Recording {
        model: "muse-spark-1.3".into(),
        task: "chw_plant_reset".into(),
        note: "synthetic test recording".into(),
        task_digest: None,
        replies: vec![
            RecordedReply::ToolCalls {
                calls: vec![RecordedCall {
                    name: "replay_scenario".into(),
                    arguments: args(&wrong, "ignore_respond_cap"),
                }],
                usage: Usage::default(),
            },
            RecordedReply::ToolCalls {
                calls: vec![RecordedCall {
                    name: "replay_scenario".into(),
                    arguments: args(&right, "ignore_respond_cap"),
                }],
                usage: Usage::default(),
            },
            RecordedReply::Reply {
                text: right,
                usage: Usage::default(),
            },
        ],
    };
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("chw_plant_reset.tools.json"),
        serde_json::to_string(&recording).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn a_tools_session_shows_sparks_checks_apart_from_the_receipt() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("authoring-tools-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write_recordings(&dir);
    let state = state(&dir);

    // Without tools there is no plain recording in this folder: refused, not mixed up.
    let (status, _) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        json!({"source": "recorded"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, info) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        json!({"source": "recorded", "tools": true}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    assert_eq!(info["config"]["tools"], true);
    assert_eq!(
        info["config"]["max_attempts"], 1,
        "tool turns are not attempts"
    );
    let id = info["id"].as_str().unwrap().to_owned();
    let all = events(&state, &id).await;
    let kinds: Vec<&str> = all.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "started",
            "drafting",
            "tool-call",
            "tool-call",
            "checked",
            "ended"
        ]
    );
    let first = &all[2]["call"];
    assert_eq!(first["name"], "replay_scenario");
    assert_eq!(first["ok"], false);
    let summary = first["summary"].as_str().unwrap();
    assert!(
        summary.starts_with("scenario ignore_respond_cap failed at "),
        "{summary}"
    );
    assert!(summary.contains("expected") && summary.contains("got"));
    assert!(
        first["args"].as_str().unwrap().contains("blocks"),
        "a short summary, not the draft"
    );
    assert!(first.get("arguments").is_none());
    assert_eq!(all[3]["call"]["ok"], true);
    let attempt = &all[4]["attempt"];
    assert_eq!(attempt["result"], "passed");
    assert_eq!(attempt["calls"], 3);
    assert_eq!(attempt["tool_calls"].as_array().unwrap().len(), 2);
    assert_eq!(all[5]["state"], "verified");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn tools_need_a_key_or_a_tools_recording() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("authoring-tools-none-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let state = state(&dir);
    let (status, body) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        json!({"tools": true}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.to_string().contains("with tools"), "{body}");
    let (status, _) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        json!({"tools": "yes"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_committed_tools_session_plays_back_red_then_green() {
    let state = state(&repo("fixtures/recordings"));
    let (status, info) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        json!({"source": "recorded", "tools": true}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    assert_eq!(info["config"]["tools"], true);
    let id = info["id"].as_str().unwrap().to_owned();
    let all = events(&state, &id).await;
    let results: Vec<&str> = all
        .iter()
        .filter(|e| e["kind"] == "checked")
        .map(|e| e["attempt"]["result"].as_str().unwrap())
        .collect();
    assert_eq!(results, ["replay-failed", "passed"]);
    let checks: Vec<&Value> = all.iter().filter(|e| e["kind"] == "tool-call").collect();
    assert!(checks.len() >= 10, "{}", checks.len());
    // Spark's own replays in attempt 1 all passed, yet the receipt failed that draft (Spark
    // had not replayed the scenarios it missed): the tools never stand in for the full replay.
    assert!(
        checks
            .iter()
            .any(|e| e["call"]["summary"].as_str().unwrap().contains("failed at"))
    );
    assert_eq!(all.last().unwrap()["state"], "verified");
    // The plain recording is still what a session without tools plays.
    let (status, plain) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        json!({"source": "recorded"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{plain}");
    assert_eq!(plain["config"]["tools"], false);
}
