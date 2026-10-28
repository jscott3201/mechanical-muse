//! Revision B of the sample sheet, offline: its recorded reading, accepted under the name Spark
//! gave it (not the one it was recorded under), still finds Spark's recorded drafting session
//! (a red attempt, then green) and recorded binding, because both are found by the task's
//! content when the name differs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
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

fn server(records: &std::path::Path) -> AppState {
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
            pace: Duration::ZERO,
            records: Some(records.to_path_buf()),
            drafts: Some(records.join("drafts")),
            explanations: None,
            readings: Some(repo("fixtures/readings")),
            bindings: Some(repo("fixtures/bindings")),
            attacks: Some(repo("fixtures/attacks")),
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
async fn revision_b_plays_back_under_its_own_name() {
    let records =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("rev-b-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&records);
    let state = server(&records);
    let png = std::fs::read(repo("fixtures/sequences/ahu-sat-reset-sample-v2.png")).unwrap();
    assert_eq!(
        png,
        std::fs::read(repo("web/public/samples/ahu-sat-reset-sample-v2.png")).unwrap()
    );
    let sample = json!({"attachments": [{"name": "ahu-sat-reset-sample-v2.png", "mime": "image/png",
        "data_base64": base64::engine::general_purpose::STANDARD.encode(png)}]});
    let (status, view) = call(&state, "POST", "/api/workbench/read", Some(sample)).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["source"], "recorded");
    let name = view["draft"]["name"].as_str().unwrap().to_owned();
    assert_ne!(name, "ahu1_sat_rev_b", "the reading keeps Spark's own name");
    let thresholds: Vec<f64> = view["draft"]["params"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"].as_str().unwrap().to_lowercase().contains("dehum"))
        .filter_map(|p| p["value"].as_f64())
        .filter(|v| *v < 1.0)
        .collect();
    assert_eq!(
        thresholds,
        vec![0.7, 0.65],
        "revision B's thresholds, as fractions"
    );

    let (status, _) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": view["draft"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, session) = call(
        &state,
        "POST",
        &format!("/api/specs/{name}/author"),
        Some(json!({"source": "recorded"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap().to_owned();
    let mut info = Value::Null;
    for _ in 0..500 {
        (_, info) = call(&state, "GET", &format!("/api/authoring/{id}"), None).await;
        if info["state"] != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(info["state"], "verified", "{info}");
    let attempts: Vec<String> = state
        .authoring()
        .get(&id)
        .unwrap()
        .journal()
        .since(None)
        .iter()
        .filter_map(|e| {
            let v = serde_json::to_value(e).ok()?;
            (v["kind"] == "checked").then(|| {
                v["attempt"]["result"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            })
        })
        .collect();
    assert_eq!(attempts, ["replay-failed", "passed"], "red, then green");

    let (status, proposal) = call(
        &state,
        "POST",
        &format!("/api/workbench/{name}/binding/propose"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposal}");
    assert_eq!(proposal["proposed_by"], "recorded");
    assert_eq!(proposal["spec"], name.as_str());
    assert_eq!(proposal["problems"], json!([]));
    let _ = std::fs::remove_dir_all(&records);
}
