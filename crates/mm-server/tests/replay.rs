//! Replaying a recorded run through the normal run routes and event stream.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mm_graph::Palette;
use mm_server::{AppState, PlantSource, RunsConfig, SpecStore, app};
use serde_json::{Value, json};
use tower::ServiceExt;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// A replay folder holding the trimmed live record used by the web tests (10 events, with a
/// deliberate hole in its sequence numbers) under the name `short`.
fn replays() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "replays-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        repo("fixtures/runs/live-chw-requests.jsonl"),
        dir.join("short.jsonl"),
    )
    .unwrap();
    std::fs::write(dir.join("broken.jsonl"), "{not json\n").unwrap();
    dir
}

fn state(replays: Option<PathBuf>, pace: Duration) -> AppState {
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(&repo("specs"), &palette).unwrap();
    AppState::with_runs(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: Vec::new(),
                pace: Duration::ZERO,
            },
            records: None,
            replays,
            replay_pace: pace,
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

async fn events(state: &AppState, id: &str) -> Vec<Value> {
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
        .expect("the replay did not end within 60 s")
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .split("\n\n")
        .filter_map(|c| c.lines().find_map(|l| l.strip_prefix("data:")))
        .map(|d| serde_json::from_str(d.trim()).unwrap())
        .collect()
}

fn recorded() -> Vec<Value> {
    std::fs::read_to_string(repo("fixtures/runs/live-chw-requests.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[tokio::test]
async fn a_replay_resends_the_record_under_a_new_run_that_says_so() {
    let state = state(Some(replays()), Duration::ZERO);
    let (status, list) = call(&state, "GET", "/api/replays", None).await;
    assert_eq!(status, StatusCode::OK);
    // The broken record is skipped, not fatal.
    assert_eq!(list.as_array().unwrap().len(), 1, "{list}");
    let original = recorded();
    let recorded_run = original[0]["run_id"].as_str().unwrap();
    assert_eq!(list[0]["name"], "short");
    assert_eq!(list[0]["recorded_run"], recorded_run);
    assert_eq!(list[0]["ticks"], 3);

    let (status, run) = call(
        &state,
        "POST",
        "/api/runs/replay",
        Some(json!({"record": "short"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["config"]["replay"]["record"], "short");
    assert_eq!(run["config"]["replay"]["recorded_run"], recorded_run);
    let id = run["id"].as_str().unwrap().to_owned();
    assert_ne!(id, recorded_run);

    let all = events(&state, &id).await;
    // Fresh, gap-free sequence numbers under the new id.
    for (i, e) in all.iter().enumerate() {
        assert_eq!(e["seq"], i);
        assert_eq!(e["run_id"], id.as_str());
    }
    // The same events in the same order, with the started config naming the record.
    let kinds = |v: &[Value]| {
        v.iter()
            .map(|e| e["kind"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let mut expected = kinds(&original);
    if expected.last().map(String::as_str) != Some("ended") {
        expected.push("ended".into());
    }
    assert_eq!(kinds(&all), expected);
    let started = all.iter().find(|e| e["kind"] == "started").unwrap();
    assert_eq!(started["config"]["replay"]["record"], "short");
    let orig_started = original.iter().find(|e| e["kind"] == "started").unwrap();
    assert_eq!(
        started["sequences"].as_array().unwrap().len(),
        orig_started["sequences"].as_array().unwrap().len()
    );
    let ticks: Vec<&Value> = all.iter().filter(|e| e["kind"] == "tick").collect();
    let orig_ticks: Vec<&Value> = original.iter().filter(|e| e["kind"] == "tick").collect();
    for (a, b) in ticks.iter().zip(&orig_ticks) {
        assert_eq!(a["sim_time_s"], b["sim_time_s"]);
        assert_eq!(a["writes"], b["writes"]);
        assert_eq!(a["readings"], b["readings"]);
    }
    let (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
    assert_eq!(info["ticks"], 3);
    assert_eq!(info["state"], all.last().unwrap()["state"]);
}

#[tokio::test]
async fn a_replay_can_be_stopped_and_holds_the_one_run_slot() {
    let state = state(Some(replays()), Duration::from_millis(300));
    let (_, run) = call(
        &state,
        "POST",
        "/api/runs/replay",
        Some(json!({"record": "short"})),
    )
    .await;
    let id = run["id"].as_str().unwrap().to_owned();
    let (status, body) = call(&state, "POST", "/api/runs", Some(json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, _) = call(&state, "POST", &format!("/api/runs/{id}/stop"), None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let all = events(&state, &id).await;
    assert_eq!(all.last().unwrap()["state"], "stopped");
    assert!(all.iter().filter(|e| e["kind"] == "tick").count() < 3);
}

#[tokio::test]
async fn bad_replay_requests_are_refused() {
    let state = state(Some(replays()), Duration::ZERO);
    for (body, status) in [
        (json!({"record": "nope"}), StatusCode::NOT_FOUND),
        (json!({"record": "../short"}), StatusCode::NOT_FOUND),
        (json!({"record": "broken"}), StatusCode::NOT_FOUND),
        (
            json!({"record": "short", "speed": 2}),
            StatusCode::BAD_REQUEST,
        ),
        (json!({}), StatusCode::BAD_REQUEST),
    ] {
        let (got, err) = call(&state, "POST", "/api/runs/replay", Some(body.clone())).await;
        assert_eq!(got, status, "{body}: {err}");
    }
    let (_, runs) = call(&state, "GET", "/api/runs", None).await;
    assert_eq!(runs, json!([]));

    let none = self::state(None, Duration::ZERO);
    let (status, err) = call(&none, "GET", "/api/replays", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err["code"], "no-replays");
}

#[tokio::test]
async fn the_committed_demo_record_is_replayable() {
    let state = state(Some(repo("fixtures/replays")), Duration::ZERO);
    let (_, list) = call(&state, "GET", "/api/replays", None).await;
    let demo = list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "day185-noon-requests")
        .expect("the demo record is listed");
    assert_eq!(demo["ticks"], 300);
    assert_eq!(demo["state"], "completed");
    assert_eq!(demo["baseline"], false);
    // The record's BOPTEST KPIs are listed, so the web app can set a run beside a baseline.
    let ener = demo["kpis"]["ener_tot"].as_f64().unwrap();
    assert!((ener - 0.3101).abs() < 1e-4, "{ener}");
    // Its baseline (same window, BOPTEST's own controllers) is committed beside it.
    let base = list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "day185-noon-baseline")
        .expect("the baseline record is listed");
    assert_eq!(base["baseline"], true);
    assert_eq!(
        (base["ticks"].clone(), base["start_s"].clone()),
        (demo["ticks"].clone(), demo["start_s"].clone())
    );
    let ener = base["kpis"]["ener_tot"].as_f64().unwrap();
    assert!((ener - 0.3255).abs() < 1e-4, "{ener}");
}

/// A replay of an engineer's sequence names which graphs were drafted, so the web app does
/// not call its hand-built plant reset a Spark draft.
#[tokio::test]
async fn replays_name_the_specs_they_drafted() {
    let state = state(Some(repo("fixtures/replays")), Duration::ZERO);
    let (_, list) = call(&state, "GET", "/api/replays", None).await;
    let find = |name: &str| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("{name} is listed"))
    };
    let own = find("day185-noon-your-sequence");
    assert_eq!(own["drafted"], true);
    assert_eq!(own["drafts"], json!(["ahu1_sat_stpt"]));
    assert_eq!(own["own"], json!(["ahu1_sat_stpt"]));
    let plant = find("day185-noon-spark-draft");
    assert_eq!(plant["drafts"], json!(["chw_plant_reset"]));
    assert_eq!(find("day185-noon-baseline")["drafts"], json!([]));
}

/// A record cut short (no `ended` line) must not replay as completed.
#[tokio::test]
async fn a_record_without_an_end_replays_as_failed() {
    let dir = replays();
    let text = std::fs::read_to_string(dir.join("short.jsonl")).unwrap();
    let cut: Vec<&str> = text
        .lines()
        .filter(|l| !l.contains("\"kind\":\"ended\""))
        .collect();
    std::fs::write(dir.join("cut.jsonl"), cut.join("\n")).unwrap();
    let state = state(Some(dir), Duration::ZERO);
    let (_, list) = call(&state, "GET", "/api/replays", None).await;
    let cut_info = list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "cut")
        .unwrap();
    assert!(cut_info.get("state").is_none());
    let (_, run) = call(
        &state,
        "POST",
        "/api/runs/replay",
        Some(json!({"record": "cut"})),
    )
    .await;
    let all = events(&state, run["id"].as_str().unwrap()).await;
    let ended = all.last().unwrap();
    assert_eq!(ended["state"], "failed");
    assert!(ended["message"].as_str().unwrap().contains("no end"));
}
