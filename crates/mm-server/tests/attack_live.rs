//! Live: Muse Spark tries to break revision B of the sample sheet. The reading and the drafting
//! session are the committed recordings; only the attack calls the model. Run with
//! `make live-attack`. The reply (checked proposals only, no transcript) is copied to
//! `fixtures/attacks/`, where a keyless server serves it labelled recorded.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use http_body_util::BodyExt;
use mm_graph::Palette;
use mm_server::{AppState, AuthoringConfig, PlantSource, RunsConfig, SpecStore, app};
use serde_json::{Value, json};
use tower::ServiceExt;

const NAME: &str = "ahu1_sat_rev_b";

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
#[ignore = "calls the Meta Model API; needs MODEL_API_KEY"]
async fn live_spark_tries_to_break_revision_b() {
    let key = std::env::var("MODEL_API_KEY").expect("set MODEL_API_KEY");
    let spark = Arc::new(mm_agent::SparkClient::new(key).unwrap());
    let records = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("attack-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&records);
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(&repo("specs"), &palette).unwrap();
    let state = AppState::build(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Off,
            records: None,
            replays: None,
            replay_pace: Duration::ZERO,
        },
        AuthoringConfig {
            spark: Some(spark),
            recordings: Some(repo("fixtures/recordings")),
            pace: Duration::ZERO,
            records: Some(records.clone()),
            drafts: Some(records.join("drafts")),
            explanations: None,
            readings: Some(repo("fixtures/readings")),
            bindings: None,
            attacks: None,
        },
    );
    let png = std::fs::read(repo("fixtures/sequences/ahu-sat-reset-sample-v2.png")).unwrap();
    let sample = json!({"attachments": [{"name": "ahu-sat-reset-sample-v2.png", "mime": "image/png",
        "data_base64": base64::engine::general_purpose::STANDARD.encode(png)}]});
    let (status, view) = call(&state, "POST", "/api/workbench/read", Some(sample)).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(
        view["source"], "recorded",
        "the reading is the committed one"
    );
    let mut draft = view["draft"].clone();
    draft["name"] = json!(NAME);
    let (status, _) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": draft, "key": view["key"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, session) = call(
        &state,
        "POST",
        &format!("/api/specs/{NAME}/author"),
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
    let content_id = state.authoring().drafts.list()[0].content_id.clone();

    let started = std::time::Instant::now();
    let (status, attack) = call(
        &state,
        "POST",
        &format!("/api/workbench/{NAME}/attack"),
        Some(json!({"draft": content_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{attack}");
    assert_eq!(attack["source"], "live");
    eprintln!(
        "attack {} in {:.0} s against {content_id}",
        attack["key"],
        started.elapsed().as_secs_f64()
    );
    for p in attack["proposals"].as_array().unwrap() {
        let s = &p["scenario"];
        eprintln!("- {} [{}]: {}", s["name"], p["result"], s["rationale"]);
        eprintln!("    {}", s["description"]);
        if let Some(f) = p["failures"].get(0) {
            eprintln!(
                "    {} at {} s: expected {}, got {} (window {}–{} s)",
                f["output"], f["t"], f["expected"], f["actual"], f["from_s"], f["to_s"]
            );
        }
    }
    for problem in attack["problems"].as_array().unwrap() {
        eprintln!("  dropped: {problem}");
    }
    let file = format!("{}.json", attack["key"].as_str().unwrap().replace(':', "_"));
    let out = repo("fixtures/attacks");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::copy(records.join("attacks").join(&file), out.join(&file)).unwrap();
    eprintln!("recorded as fixtures/attacks/{file}");
    let _ = std::fs::remove_dir_all(&records);
}
