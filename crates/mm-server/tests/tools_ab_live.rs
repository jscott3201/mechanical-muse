//! Live A/B: authoring sessions with and without Spark's own checks (function tools), through
//! the router and the real Meta Model API. Run with `make live-tools-ab` (needs the key; costs
//! model calls). `MM_AB_SPECS` (comma-separated, default chw_plant_reset and the revision B
//! workbench task) and `MM_AB_RUNS` (default 2) set the size. Sessions run one at a time. Each
//! session's recording (no ids, no reasoning) and a summary go to `runs/tools-ab/` (gitignored).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

async fn call(
    state: &AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let response = app(state.clone()).oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
#[ignore = "calls the Meta Model API; needs MODEL_API_KEY"]
async fn live_tools_ab() {
    let key = std::env::var("MODEL_API_KEY").expect("set MODEL_API_KEY");
    let out = repo("runs/tools-ab");
    std::fs::create_dir_all(&out).unwrap();
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
            spark: Some(Arc::new(mm_agent::SparkClient::new(key).unwrap())),
            recordings: None,
            pace: Duration::ZERO,
            records: Some(out.clone()),
            drafts: None,
            ..AuthoringConfig::default()
        },
    );
    // Revision B of the sample sheet, as Spark read it (committed reading), accepted as is.
    let reading: Value = serde_json::from_slice(
        &std::fs::read(repo("fixtures/readings/read_1a23e63fdf5677b8.json")).unwrap(),
    )
    .unwrap();
    let (status, accepted) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": reading["draft"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{accepted}");
    let rev_b = reading["draft"]["name"].as_str().unwrap().to_owned();

    let names: Vec<String> = std::env::var("MM_AB_SPECS").map_or_else(
        |_| vec!["chw_plant_reset".to_owned(), rev_b.clone()],
        |s| s.split(',').map(str::to_owned).collect(),
    );
    let runs: u32 = std::env::var("MM_AB_RUNS")
        .ok()
        .and_then(|r| r.parse().ok())
        .unwrap_or(2);
    let mut rows = Vec::new();
    for name in &names {
        for run in 1..=runs {
            for tools in [false, true] {
                let started = Instant::now();
                let (status, info) = call(
                    &state,
                    "POST",
                    &format!("/api/specs/{name}/author"),
                    Some(json!({"source": "live", "tools": tools})),
                )
                .await;
                assert_eq!(status, StatusCode::CREATED, "{info}");
                let id = info["id"].as_str().unwrap().to_owned();
                let session = state.authoring().get(&id).unwrap();
                loop {
                    let (_, now) = call(&state, "GET", &format!("/api/authoring/{id}"), None).await;
                    if now["state"] != "running" {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                let wall = started.elapsed().as_secs_f64();
                let events: Vec<Value> = session
                    .journal()
                    .since(None)
                    .iter()
                    .map(|e| serde_json::to_value(e).unwrap())
                    .collect();
                let attempts: Vec<&Value> = events
                    .iter()
                    .filter(|e| e["kind"] == "checked")
                    .map(|e| &e["attempt"])
                    .collect();
                let ended = events.iter().find(|e| e["kind"] == "ended").unwrap();
                let calls: u64 = attempts
                    .iter()
                    .map(|a| a["calls"].as_u64().unwrap_or(0))
                    .sum();
                let tool_calls: Vec<String> = events
                    .iter()
                    .filter(|e| e["kind"] == "tool-call")
                    .map(|e| {
                        format!(
                            "{}: {}",
                            e["call"]["name"].as_str().unwrap_or("?"),
                            e["call"]["summary"].as_str().unwrap_or("")
                        )
                    })
                    .collect();
                let results: Vec<&str> = attempts
                    .iter()
                    .map(|a| a["result"].as_str().unwrap_or("?"))
                    .collect();
                let row = json!({
                    "spec": name, "run": run, "tools": tools,
                    "state": ended["state"], "attempts": attempts.len(), "results": results,
                    "model_calls": calls, "tool_calls": tool_calls.len(),
                    "input_tokens": ended["usage"]["input"], "output_tokens": ended["usage"]["output"],
                    "reasoning_tokens": ended["usage"]["reasoning"],
                    "wall_s": (wall * 10.0).round() / 10.0,
                    "session": id,
                });
                eprintln!("{row}");
                for t in &tool_calls {
                    eprintln!("    {t}");
                }
                rows.push(row);
                std::fs::write(
                    out.join("summary.json"),
                    serde_json::to_string_pretty(&rows).unwrap(),
                )
                .unwrap();
            }
        }
    }
}
