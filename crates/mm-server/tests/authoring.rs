//! Authoring sessions end to end through the router, with the recorded plant-reset run as the
//! model (no API call): the event stream, the stored draft and its receipt, and refusals.

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

/// The SSE body as (event, data) pairs, bounded so a session that never ends fails the test.
async fn events(state: &AppState, id: &str) -> Vec<(String, Value)> {
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/authoring/{id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = tokio::time::timeout(Duration::from_secs(60), response.into_body().collect())
        .await
        .expect("the stream did not end within 60 s")
        .unwrap()
        .to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    text.split("\n\n")
        .filter(|chunk| chunk.contains("data:"))
        .map(|chunk| {
            let field = |name: &str| {
                chunk
                    .lines()
                    .find_map(|l| l.strip_prefix(&format!("{name}:")))
                    .unwrap()
                    .trim()
                    .to_owned()
            };
            (
                field("event"),
                serde_json::from_str(&field("data")).unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_recorded_session_goes_red_amber_green_and_stores_the_draft() {
    let drafts = scratch("drafts");
    let state = state(Some(drafts.clone()), Duration::ZERO);
    let (status, info) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    // No key configured, so the default source is the recording, and it says so.
    assert_eq!(info["config"]["source"], "recorded");
    assert_eq!(info["config"]["model"], "muse-spark-1.3");
    assert_eq!(info["config"]["feedback"], "with-vectors");
    let id = info["id"].as_str().unwrap().to_owned();

    let all = events(&state, &id).await;
    let kinds: Vec<&str> = all.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "started", "drafting", "checked", "drafting", "checked", "drafting", "checked", "ended"
        ]
    );
    for (seq, (_, data)) in all.iter().enumerate() {
        assert_eq!(data["seq"], seq);
        assert_eq!(data["session_id"], id.as_str());
    }
    assert_eq!(all[1].1["attempt"], 1);
    // A playback has only as many attempts as recorded replies, at the effort it ran at.
    assert_eq!(all[1].1["of"], 3);
    assert_eq!(info["config"]["max_attempts"], 3);
    assert_eq!(info["config"]["effort"], "as recorded");

    // 1: the engine refuses an algebraic loop, and each problem names a block of the draft.
    let first = &all[2].1["attempt"];
    assert_eq!(first["result"], "load-failed");
    assert_eq!(first["code"], "algebraic-loop");
    let problems = first["problems"].as_array().unwrap();
    assert!(!problems.is_empty());
    let blocks: Vec<&str> = first["graph"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    for p in problems {
        let node = p["node"]
            .as_str()
            .expect("an engine subject maps to a block");
        assert!(blocks.contains(&node), "{node} not in {blocks:?}");
    }
    // 2: loads, 8 of 10 scenarios.
    let second = &all[4].1["attempt"];
    assert_eq!(second["result"], "replay-failed");
    assert_eq!(
        (second["passed"].as_u64(), second["total"].as_u64()),
        (Some(8), Some(10))
    );
    assert_eq!(second["failures"].as_array().unwrap().len(), 2);
    // 3: green.
    let third = &all[6].1["attempt"];
    assert_eq!(third["result"], "passed");
    let content_id = third["content_id"].as_str().unwrap().to_owned();

    let ended = &all[7].1;
    assert_eq!(ended["state"], "verified");
    assert_eq!(ended["draft"]["content_id"], content_id.as_str());
    assert_eq!(ended["draft"]["source"], "recorded");
    assert_eq!(ended["draft"]["attempts_used"], 3);
    assert!(ended["usage"]["output"].as_u64().unwrap() > 0);

    let (_, info) = call(&state, "GET", &format!("/api/authoring/{id}"), None).await;
    assert_eq!(info["state"], "verified");
    let (_, listed) = call(&state, "GET", "/api/drafts", None).await;
    assert_eq!(listed[0]["content_id"], content_id.as_str());
    let (status, graph) = call(
        &state,
        "GET",
        &format!("/api/drafts/{content_id}/graph"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(graph["name"], "chw_plant_reset");

    // The draft survives a restart.
    let again = self::state(Some(drafts), Duration::ZERO);
    let (_, listed) = call(&again, "GET", "/api/drafts", None).await;
    assert_eq!(listed[0]["content_id"], content_id.as_str());
}

#[tokio::test]
async fn too_small_a_budget_ends_unverified_with_no_draft() {
    let state = state(None, Duration::ZERO);
    let (status, info) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        Some(json!({"attempts": 2})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    let all = events(&state, info["id"].as_str().unwrap()).await;
    let ended = &all.last().unwrap().1;
    assert_eq!(ended["kind"], "ended");
    assert_eq!(ended["state"], "unverified");
    assert!(ended.get("draft").is_none());
    let (_, listed) = call(&state, "GET", "/api/drafts", None).await;
    assert_eq!(listed, json!([]));
}

#[tokio::test]
async fn one_session_at_a_time() {
    let state = state(None, Duration::from_millis(500));
    let uri = "/api/specs/chw_plant_reset/author";
    let (status, first) = call(&state, "POST", uri, Some(json!({}))).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = call(&state, "POST", uri, Some(json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "author-active");
    events(&state, first["id"].as_str().unwrap()).await;
    let (status, _) = call(&state, "POST", uri, Some(json!({}))).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "a finished session frees the slot"
    );
}

#[tokio::test]
async fn bad_requests_are_refused_before_any_model_call() {
    let state = state(None, Duration::ZERO);
    let uri = "/api/specs/chw_plant_reset/author";
    for (body, why) in [
        (json!({"source": "live"}), "no key configured"),
        (json!({"attempts": 0}), "attempts below 1"),
        (json!({"attempts": 6}), "attempts above 5"),
        (json!({"effort": "max"}), "effort outside low/medium/high"),
        (json!({"model": "other"}), "unknown field"),
    ] {
        let (status, err) = call(&state, "POST", uri, Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}: {err}");
        assert_eq!(err["code"], "bad-request", "{why}");
    }
    let (status, err) = call(
        &state,
        "POST",
        "/api/specs/ahu_chw_requests/author",
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no recording for this spec");
    assert_eq!(err["code"], "no-recording", "{err}");
    assert!(err["message"].as_str().unwrap().contains("no recorded run"));
    let (status, _) = call(&state, "POST", "/api/specs/nope/author", Some(json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&state, "GET", "/api/authoring/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&state, "GET", "/api/drafts/nope/graph", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, sessions) = call(&state, "GET", "/api/authoring", None).await;
    assert_eq!(sessions, json!([]), "a refused request starts nothing");
}

/// A keyless server can only play back recordings: the client must learn which specs have one
/// before a Draft click, not from a red error after it.
#[tokio::test]
async fn recorded_sessions_are_listed_per_spec() {
    let state = state(None, Duration::ZERO);
    let (status, got) = call(&state, "GET", "/api/specs/chw_plant_reset/recordings", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got, json!({"plain": true, "tools": true}));
    let (status, got) = call(
        &state,
        "GET",
        "/api/specs/ahu_chw_requests/recordings",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got, json!({"plain": false, "tools": false}));
    let (status, _) = call(&state, "GET", "/api/specs/nope/recordings", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, err) = call(
        &state,
        "POST",
        "/api/specs/ahu_chw_requests/author",
        Some(json!({"tools": true})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err["code"], "no-recording", "{err}");
}

/// Occupied floors with air handlers slightly warm: the plant reset runs normally.
fn readings(time: f64) -> mm_runtime::Measurements {
    let mut m: mm_runtime::Measurements = [("time".to_owned(), time)].into_iter().collect();
    for f in 1..=3 {
        m.insert(format!("floor{f}_reaAHU_TSup_y"), 286.5);
        m.insert(format!("floor{f}_TSupAirSet_u"), 285.95);
        m.insert(format!("floor{f}_reaAHU_yCooVal_y"), 0.4);
        m.insert(format!("floor{f}_reaAHU_occ_y"), 1.0);
    }
    m.insert("reaChiWatSys_TCHWSup_y".into(), 278.7);
    m
}

/// Authoring and runs in one server, with a scripted plant.
fn with_plant() -> AppState {
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(&repo("specs"), &palette).unwrap();
    AppState::build(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: (0..=10).map(|k| readings(120.0 * f64::from(k))).collect(),
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
    )
}

async fn run_events(state: &AppState, id: &str) -> Vec<Value> {
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/runs/{id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = tokio::time::timeout(Duration::from_secs(60), response.into_body().collect())
        .await
        .expect("the run did not end within 60 s")
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .split("\n\n")
        .filter_map(|c| c.lines().find_map(|l| l.strip_prefix("data:")))
        .map(|d| serde_json::from_str(d.trim()).unwrap())
        .collect()
}

fn stored(state: &AppState, content_id: &str, graph: mm_graph::BlockGraph) {
    state.authoring().drafts.insert(mm_server::drafts::Draft {
        info: mm_server::author_dto::DraftInfo {
            content_id: content_id.to_owned(),
            spec: "chw_plant_reset".into(),
            scenarios: 10,
            model: "muse-spark-1.3".into(),
            source: mm_server::author_dto::ModelSource::Recorded,
            session: "test".into(),
            attempts_used: 1,
        },
        graph,
    });
}

fn reference() -> mm_graph::BlockGraph {
    serde_json::from_slice(&std::fs::read(repo("specs/chw_plant_reset/graph.json")).unwrap())
        .unwrap()
}

#[tokio::test]
async fn a_run_can_use_the_verified_draft_and_says_so() {
    let state = with_plant();
    let (_, info) = call(
        &state,
        "POST",
        "/api/specs/chw_plant_reset/author",
        Some(json!({})),
    )
    .await;
    let ended = events(&state, info["id"].as_str().unwrap()).await;
    let content_id = ended.last().unwrap().1["draft"]["content_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let body = json!({"steps": 5, "drafts": {"chw_plant_reset": content_id}});
    let (status, run) = call(&state, "POST", "/api/runs", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(
        run["config"]["drafts"]["chw_plant_reset"],
        content_id.as_str()
    );
    let all = run_events(&state, run["id"].as_str().unwrap()).await;
    let started = all.iter().find(|e| e["kind"] == "started").unwrap();
    let origin = |name: &str| {
        let s = started["sequences"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()
            .clone();
        (
            s["origin"].clone(),
            s["content_id"].clone(),
            s.get("model").cloned(),
        )
    };
    assert_eq!(
        origin("chw"),
        (
            json!("drafted"),
            json!(content_id),
            Some(json!("muse-spark-1.3"))
        )
    );
    let chw = started["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "chw")
        .unwrap();
    assert_eq!(
        chw["source"], "recorded",
        "a run says when its draft was a playback"
    );
    assert_eq!(origin("ahu1").0, json!("reference"));
    assert_eq!(origin("ahu1").2, None);
    assert_eq!(all.last().unwrap()["state"], "completed");
}

#[tokio::test]
async fn a_draft_is_replayed_again_before_it_runs() {
    let state = with_plant();
    // Stored under an id its graph does not have: the fresh receipt disagrees.
    stored(&state, "sha256:not-this-one", reference());
    // A graph that fails its vectors (trim step doubled), stored as if verified.
    let mut broken = reference();
    let block = broken
        .blocks
        .iter_mut()
        .find(|b| b.id == "respGain")
        .unwrap();
    block.params[0].value = mm_graph::ParamValue::Number(0.06);
    stored(&state, "sha256:broken", broken);

    for (id, expect) in [
        ("sha256:not-this-one", "now has content id"),
        ("sha256:broken", "did not pass its vectors"),
    ] {
        let body = json!({"steps": 3, "drafts": {"chw_plant_reset": id}});
        let (status, run) = call(&state, "POST", "/api/runs", Some(body)).await;
        assert_eq!(status, StatusCode::CREATED, "{run}");
        let all = run_events(&state, run["id"].as_str().unwrap()).await;
        assert!(
            all.iter().all(|e| e["kind"] != "tick"),
            "{id} reached the plant"
        );
        let ended = all.last().unwrap();
        assert_eq!(ended["state"], "failed", "{id}");
        assert!(
            ended["message"].as_str().unwrap().contains(expect),
            "{id}: {ended}"
        );
    }
}

#[tokio::test]
async fn run_requests_with_bad_drafts_are_refused() {
    let state = with_plant();
    stored(&state, "sha256:x", reference());
    for (body, why) in [
        (
            json!({"drafts": {"chw_plant_reset": "sha256:missing"}}),
            "unknown draft",
        ),
        (
            json!({"drafts": {"ahu_chw_requests": "sha256:x"}}),
            "draft of another spec",
        ),
        (
            json!({"requests": false, "drafts": {"ahu_chw_requests": "sha256:x"}}),
            "spec not used by this run",
        ),
        (
            json!({"baseline": true, "drafts": {"chw_plant_reset": "sha256:x"}}),
            "baseline takes no drafts",
        ),
    ] {
        let (status, err) = call(&state, "POST", "/api/runs", Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}: {err}");
    }
    let (_, runs) = call(&state, "GET", "/api/runs", None).await;
    assert_eq!(runs, json!([]), "a refused request starts nothing");
}

#[tokio::test]
async fn a_draft_that_no_longer_validates_is_refused_before_replay() {
    let state = with_plant();
    let mut bad = reference();
    // An extra block nothing reads: the validator refuses it.
    bad.blocks.push(mm_graph::Block {
        id: "orphan".into(),
        class: "Reals.Add".into(),
        params: Vec::new(),
    });
    stored(&state, "sha256:orphan", bad);
    let body = json!({"steps": 3, "drafts": {"chw_plant_reset": "sha256:orphan"}});
    let (_, run) = call(&state, "POST", "/api/runs", Some(body)).await;
    let all = run_events(&state, run["id"].as_str().unwrap()).await;
    assert!(all.iter().all(|e| e["kind"] != "tick"));
    let ended = all.last().unwrap();
    assert_eq!(ended["state"], "failed");
    assert!(
        ended["message"]
            .as_str()
            .unwrap()
            .contains("fails validation"),
        "{ended}"
    );
}

#[tokio::test]
async fn only_drafts_filed_under_their_own_content_id_are_loaded() {
    let dir = scratch("filed");
    std::fs::create_dir_all(&dir).unwrap();
    let draft = json!({
        "info": {"content_id": "sha256:abc", "spec": "chw_plant_reset", "scenarios": 10,
                 "model": "muse-spark-1.3", "source": "live", "session": "s", "attempts_used": 1},
        "graph": serde_json::to_value(reference()).unwrap()
    });
    std::fs::write(dir.join("sha256_abc.json"), draft.to_string()).unwrap();
    // The same id under another name, claiming another source: ignored.
    let mut copy = draft.clone();
    copy["info"]["source"] = json!("recorded");
    std::fs::write(dir.join("copy.json"), copy.to_string()).unwrap();
    std::fs::write(dir.join("broken.json"), "{not json").unwrap();
    let state = state(Some(dir), Duration::ZERO);
    let (_, listed) = call(&state, "GET", "/api/drafts", None).await;
    assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed[0]["source"], "live");
}
