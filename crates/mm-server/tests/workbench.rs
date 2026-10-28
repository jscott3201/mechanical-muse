//! Bring your own sequence through the router, with no model key: the recorded reading of the
//! sample sheet, the engineer's acceptance (and its refusals), a recorded Spark session verified
//! against the accepted scenarios, and the accepted sequence restored after a restart.

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

fn sample() -> Value {
    let png = std::fs::read(repo("fixtures/sequences/ahu-sat-reset-sample.png")).unwrap();
    json!({"text": "", "attachments": [{
        "name": "ahu-sat-reset-sample.png",
        "mime": "image/png",
        "data_base64": base64::engine::general_purpose::STANDARD.encode(png),
    }]})
}

#[tokio::test]
async fn the_sample_sheet_becomes_a_sequence_spark_drafts_and_the_engine_verifies() {
    let records = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("workbench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&records);
    let state = server(&records);

    // No key: the sample's recorded reading, labelled. The web app's copy of the sample must be
    // the same bytes, or the demo would ask for a live call.
    assert_eq!(
        std::fs::read(repo("fixtures/sequences/ahu-sat-reset-sample.png")).unwrap(),
        std::fs::read(repo("web/public/samples/ahu-sat-reset-sample.png")).unwrap()
    );
    let (status, view) = call(&state, "POST", "/api/workbench/read", Some(sample())).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["source"], "recorded");
    assert_eq!(view["problems"], json!([]));
    let draft = view["draft"].clone();
    let name = draft["name"].as_str().unwrap().to_owned();
    assert!(draft["scenarios"].as_array().unwrap().len() >= 4);

    // Anything else without a key is refused, not guessed.
    let (status, body) = call(
        &state,
        "POST",
        "/api/workbench/read",
        Some(json!({"text": "Run the fan when hot."})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "no-model");

    // A scenario that names a point the sequence does not have is not accepted.
    let mut broken = draft.clone();
    broken["scenarios"][0]["expect"][0]["output"] = json!("nope");
    let (status, body) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": broken})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["message"].as_str().unwrap().contains("not an output"),
        "{body}"
    );
    // Nor can it take a built-in sequence's name.
    let mut taken = draft.clone();
    taken["name"] = json!("chw_plant_reset");
    let (status, _) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": taken})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, accepted) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": draft, "key": view["key"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{accepted}");
    assert_eq!(accepted["spec"]["reference"], false);
    // The same name again would silently swap the gate under its verified drafts: refused.
    let (status, body) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": draft})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("already accepted"),
        "{body}"
    );
    let (_, list) = call(&state, "GET", "/api/workbench", None).await;
    assert_eq!(list[0]["name"], name.as_str());
    let (status, _) = call(&state, "GET", &format!("/api/specs/{name}/graph"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no hand-built graph");

    // Spark's recorded session for this sequence, checked against the accepted scenarios now.
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
    // The verified draft downloads as CXF.
    let (_, drafts) = call(&state, "GET", "/api/drafts", None).await;
    let content_id = drafts
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["spec"] == name.as_str())
        .unwrap()["content_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/drafts/{content_id}/cxf"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains(".cxf.jsonld")
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(
        serde_json::from_slice::<Value>(&bytes)
            .unwrap()
            .get("@context")
            .is_some()
    );

    // A restarted server restores the accepted sequence.
    let again = server(&records);
    mm_server::workbench::restore(&again);
    let (status, detail) = call(&again, "GET", &format!("/api/specs/{name}"), None).await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    let names = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap().to_owned())
            .collect()
    };
    let accepted: Vec<String> = draft["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names(&detail["scenarios"]),
        accepted,
        "the restored gate is the accepted one"
    );
    let _ = std::fs::remove_dir_all(&records);
}

async fn author_recorded(state: &AppState, name: &str) -> Value {
    let (status, session) = call(
        state,
        "POST",
        &format!("/api/specs/{name}/author"),
        Some(json!({"source": "recorded"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap().to_owned();
    let mut info = Value::Null;
    for _ in 0..500 {
        (_, info) = call(state, "GET", &format!("/api/authoring/{id}"), None).await;
        if info["state"] != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    info
}

/// The accepted scenarios, as edited, are what the draft is checked against: change one expected
/// value and the same recorded draft is no longer verified.
#[tokio::test]
async fn an_edited_expected_value_is_what_the_draft_must_meet() {
    let records = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("workbench-edit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&records);
    let state = server(&records);
    let (_, view) = call(&state, "POST", "/api/workbench/read", Some(sample())).await;
    let mut draft = view["draft"].clone();
    let name = draft["name"].as_str().unwrap().to_owned();
    // The first real-valued check, nudged by 1 K.
    let check = draft["scenarios"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .flat_map(|s| s["expect"].as_array_mut().unwrap().iter_mut())
        .find(|e| e["equals"].is_f64())
        .unwrap();
    check["equals"] = json!(check["equals"].as_f64().unwrap() + 1.0);
    let (status, body) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": draft})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let info = author_recorded(&state, &name).await;
    assert_ne!(info["state"], "verified", "{info}");
    let _ = std::fs::remove_dir_all(&records);
}

/// The limit is stated in characters, so it must count characters: text in a language with
/// accents is not cut at half the length.
#[tokio::test]
async fn the_text_limit_counts_characters_not_bytes() {
    let records = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("workbench-limit-{}", std::process::id()));
    let state = server(&records);
    let (status, err) = call(
        &state,
        "POST",
        "/api/workbench/read",
        Some(json!({"text": "a".repeat(40_001), "attachments": []})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        err["message"].as_str().unwrap().contains("at most"),
        "{err}"
    );
    // 30 000 characters, 60 000 bytes: within the limit.
    let (_, body) = call(
        &state,
        "POST",
        "/api/workbench/read",
        Some(json!({"text": "\u{e9}".repeat(30_000), "attachments": []})),
    )
    .await;
    assert!(!body.to_string().contains("at most"), "{body}");
    let _ = std::fs::remove_dir_all(records);
}
