//! "Try to break it", offline: revision B of the sample sheet is read (recorded), accepted, and
//! drafted (recorded session, verified). Then Spark's proposals, here a recording written by the
//! test (one copy of an accepted scenario, one deliberately wrong), are replayed against the
//! verified draft: the copy passes and the wrong one fails with the authoring failure detail.
//! Nothing changes the sequence's own scenarios; the chosen proposal becomes a new revision only
//! through the accept endpoint.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
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

fn server(records: &Path, attacks: &Path) -> AppState {
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(&repo("specs"), &palette).unwrap();
    AppState::build(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Off,
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
            bindings: None,
            attacks: Some(attacks.to_path_buf()),
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

/// Reads revision B (recorded), accepts it as `NAME`, drafts it (recorded, verified) and
/// returns the accepted draft and the verified draft's content id.
async fn accepted_and_drafted(state: &AppState) -> (Value, String) {
    let png = std::fs::read(repo("fixtures/sequences/ahu-sat-reset-sample-v2.png")).unwrap();
    let sample = json!({"attachments": [{"name": "ahu-sat-reset-sample-v2.png", "mime": "image/png",
        "data_base64": base64::engine::general_purpose::STANDARD.encode(png)}]});
    let (status, view) = call(state, "POST", "/api/workbench/read", Some(sample)).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    let mut draft = view["draft"].clone();
    draft["name"] = json!(NAME);
    let (status, _) = call(
        state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": draft})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, session) = call(
        state,
        "POST",
        &format!("/api/specs/{NAME}/author"),
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
    assert_eq!(info["state"], "verified", "{info}");
    let content_id = state.authoring().drafts.list()[0].content_id.clone();
    (draft, content_id)
}

/// The same scenario expecting something else at every check: no correct draft passes it.
fn wrong(mut scenario: Value) -> Value {
    scenario["name"] = json!("deliberately_wrong");
    for e in scenario["expect"].as_array_mut().unwrap() {
        e["equals"] = match &e["equals"] {
            Value::Bool(b) => json!(!b),
            v => json!(v.as_f64().unwrap() + 5.0),
        };
    }
    scenario
}

#[tokio::test]
async fn proposals_replay_against_the_verified_draft_and_never_touch_the_gate() {
    let root =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("attack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let attacks = root.join("recorded-attacks");
    let state = server(&root.join("records"), &attacks);
    let (accepted, draft) = accepted_and_drafted(&state).await;
    let uri = format!("/api/workbench/{NAME}/attack");

    let (status, body) = call(
        &state,
        "POST",
        "/api/workbench/no_such/attack",
        Some(json!({"draft": draft})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = call(&state, "POST", &uri, Some(json!({"draft": "cxf:nope"}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "unknown-draft");
    let (status, body) = call(
        &state,
        "POST",
        "/api/workbench/chw_plant_reset/attack",
        Some(json!({"draft": draft})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = call(&state, "POST", &uri, Some(json!({"draft": draft}))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "no-model");

    // A recorded reply for exactly what would be sent: a copy of an accepted scenario (the
    // verified draft passes it by construction) and a deliberately wrong one.
    let key = mm_server::attack::attack_key(&state, NAME).unwrap();
    let mut copy = accepted["scenarios"][0].clone();
    copy["name"] = json!("copy_of_first");
    copy["rationale"] = json!("the same as an accepted scenario");
    let record = json!({"key": key, "model": "muse-spark-1.3", "source": "live",
        "proposals": [copy, wrong(accepted["scenarios"][0].clone())], "problems": ["dropped `x`: off the clock"]});
    std::fs::create_dir_all(&attacks).unwrap();
    std::fs::write(
        attacks.join(format!("{}.json", key.replace(':', "_"))),
        record.to_string(),
    )
    .unwrap();

    let (status, view) = call(&state, "POST", &uri, Some(json!({"draft": draft}))).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["key"], key.as_str());
    assert_eq!(view["source"], "recorded", "a committed reply is labelled");
    assert_eq!(view["draft"], draft.as_str());
    assert_eq!(view["sequence"]["name"], NAME);
    assert_eq!(view["sequence"]["scenarios"], accepted["scenarios"]);
    assert_eq!(view["problems"], json!(["dropped `x`: off the clock"]));
    let proposals = view["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 2);
    assert_eq!(proposals[0]["scenario"]["name"], "copy_of_first");
    assert_eq!(proposals[0]["result"], "passed", "{view}");
    assert_eq!(proposals[1]["result"], "failed", "{view}");
    let failure = &proposals[1]["failures"][0];
    assert_eq!(failure["name"], "deliberately_wrong");
    assert_eq!(failure["result"], "fail");
    assert!(failure["output"].is_string() && failure["t"].is_number());
    assert_ne!(failure["expected"], failure["actual"]);

    // The same request again is served from memory, identically.
    let (_, again) = call(&state, "POST", &uri, Some(json!({"draft": draft}))).await;
    assert_eq!(again, view);

    // The sequence's own gate is unchanged.
    let (_, spec) = call(&state, "GET", &format!("/api/specs/{NAME}"), None).await;
    assert_eq!(
        spec["summary"]["scenarios"],
        accepted["scenarios"].as_array().unwrap().len()
    );

    // Accepting the chosen proposal is a new revision, through the usual checks.
    let mut revision = view["sequence"].clone();
    revision["name"] = json!(format!("{NAME}_r2"));
    revision["scenarios"]
        .as_array_mut()
        .unwrap()
        .push(proposals[0]["scenario"].clone());
    let (status, body) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": revision})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(
        body["spec"]["summary"]["scenarios"],
        accepted["scenarios"].as_array().unwrap().len() + 1
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The committed live attack on revision B (`make live-attack`) plays back without a key. If
/// the prompt or the sequence changes, its key changes and this fails: record it again.
#[tokio::test]
async fn the_committed_attack_on_revision_b_plays_back_without_a_key() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("attack-recorded-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let state = server(&root.join("records"), &repo("fixtures/attacks"));
    let (_, draft) = accepted_and_drafted(&state).await;
    let (status, view) = call(
        &state,
        "POST",
        &format!("/api/workbench/{NAME}/attack"),
        Some(json!({"draft": draft})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["source"], "recorded");
    let results: Vec<&str> = view["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["result"].as_str().unwrap())
        .collect();
    assert_eq!(results, ["passed"; 4], "{view}");
    let _ = std::fs::remove_dir_all(&root);
}
