//! Run sessions end to end through the router, on a scripted plant (no BOPTEST): start, the
//! event stream and its resume, the run record, refusals and stop.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mm_graph::Palette;
use mm_runtime::Measurements;
use mm_server::run_dto::RunRequest;
use mm_server::{AppState, PlantSource, RunsConfig, SpecStore, app};
use serde_json::{Value, json};
use tower::ServiceExt;

/// Occupied floors, air handlers 0.55 K warm with valves at 40 %: no requests, so the reset
/// trims after its start delay.
fn readings(time: f64) -> Measurements {
    let mut m: Measurements = [("time".to_owned(), time)].into_iter().collect();
    for f in 1..=3 {
        m.insert(format!("floor{f}_reaAHU_TSup_y"), 286.5);
        m.insert(format!("floor{f}_TSupAirSet_u"), 285.95);
        m.insert(format!("floor{f}_reaAHU_yCooVal_y"), 0.4);
        m.insert(format!("floor{f}_reaAHU_occ_y"), 1.0);
    }
    m.insert("reaChiWatSys_TCHWSup_y".into(), 278.7);
    m
}

fn state(records: Option<PathBuf>) -> AppState {
    paced(records, std::time::Duration::ZERO)
}

fn paced(records: Option<PathBuf>, pace: std::time::Duration) -> AppState {
    let palette = Palette::pinned().unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../specs");
    let specs = SpecStore::load(&dir, &palette).unwrap();
    let script = (0..=40).map(|k| readings(60.0 * f64::from(k))).collect();
    AppState::with_runs(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: script,
                pace,
            },
            records,
            replays: None,
            replay_pace: std::time::Duration::ZERO,
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

/// The SSE body as (id, event, data) triples; the stream ends after `ended`. Bounded, so a
/// run that never ends fails the test instead of hanging it.
async fn events(state: &AppState, id: &str, last: Option<u64>) -> Vec<(u64, String, Value)> {
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        events_unbounded(state, id, last),
    )
    .await
    .expect("event stream did not end within 60 s")
}

async fn events_unbounded(
    state: &AppState,
    id: &str,
    last: Option<u64>,
) -> Vec<(u64, String, Value)> {
    let mut req = Request::builder().uri(format!("/api/runs/{id}/events"));
    if let Some(l) = last {
        req = req.header("last-event-id", l.to_string());
    }
    let response = app(state.clone())
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let text = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
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
                field("id").parse().unwrap(),
                field("event"),
                serde_json::from_str(&field("data")).unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_run_streams_verified_ticks_and_ends() {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("runs-{}", std::process::id()));
    let state = state(Some(dir.clone()));
    let (status, info) = call(&state, "POST", "/api/runs", Some(json!({"steps": 25}))).await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    let id = info["id"].as_str().unwrap().to_owned();
    assert_eq!(info["config"]["steps"], 25);
    assert_eq!(info["config"]["requests"], true);

    let all = events(&state, &id, None).await;
    let kinds: Vec<&str> = all.iter().map(|(_, k, _)| k.as_str()).collect();
    assert_eq!(kinds[..3], ["phase", "started", "phase"]);
    assert_eq!(kinds.iter().filter(|k| **k == "tick").count(), 25);
    assert_eq!(*kinds.last().unwrap(), "ended");
    for (i, (seq, _, data)) in all.iter().enumerate() {
        assert_eq!(*seq, i as u64, "ids count up from 0");
        assert_eq!(data["seq"], i as u64);
        assert_eq!(data["run_id"], id.as_str());
    }
    // Every sequence was loaded from a receipt; the request instances share one content id.
    let started = &all[1].2;
    let seqs = started["sequences"].as_array().unwrap();
    assert_eq!(seqs.len(), 4);
    assert_eq!(seqs[0]["content_id"], seqs[2]["content_id"]);
    assert_ne!(seqs[0]["content_id"], seqs[3]["content_id"]);
    // The first trim lands at 1200 s, as in the spec's vectors.
    let tick_at = |t: f64| {
        all.iter()
            .find(|(_, k, d)| k == "tick" && d["sim_time_s"] == t)
            .map(|(_, _, d)| d.clone())
            .unwrap()
    };
    let t0 = tick_at(0.0);
    assert_eq!(t0["writes"]["oveChiWatSys_TWSet_u"], 278.71);
    assert_eq!(t0["outputs"]["ahu1"]["yChiWatResReq"], 0);
    assert_eq!(t0["readings"]["reaChiWatSys_TCHWSup_y"], 278.7);
    assert!(t0["events"].as_array().unwrap().is_empty());
    let trimmed = tick_at(1200.0)["writes"]["oveChiWatSys_TWSet_u"]
        .as_f64()
        .unwrap();
    assert!((trimmed - 278.8476).abs() < 1e-6, "{trimmed}");
    assert_eq!(all.last().unwrap().2["state"], "completed");

    let (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
    assert_eq!(info["state"], "completed");
    assert_eq!(info["ticks"], 25);
    assert_eq!(info["events"], all.len());

    // Resume after id 5: the rest, in order, with nothing repeated.
    let rest = events(&state, &id, Some(5)).await;
    assert_eq!(rest.first().unwrap().0, 6);
    assert_eq!(rest.len(), all.len() - 6);

    // The record on disk has the same events, one JSON object per line.
    let record = std::fs::read_to_string(dir.join(format!("{id}.jsonl"))).unwrap();
    let lines: Vec<Value> = record
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), all.len());
    assert_eq!(lines[1], all[1].2);
}

#[tokio::test]
async fn a_baseline_run_writes_nothing() {
    let state = state(None);
    let (status, info) = call(
        &state,
        "POST",
        "/api/runs",
        Some(json!({"baseline": true, "steps": 3})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let all = events(&state, info["id"].as_str().unwrap(), None).await;
    let started = &all.iter().find(|(_, k, _)| k == "started").unwrap().2;
    assert!(started["sequences"].as_array().unwrap().is_empty());
    for (_, _, d) in all.iter().filter(|(_, k, _)| k == "tick") {
        assert!(d["writes"].as_object().unwrap().is_empty());
    }
    assert_eq!(
        started["config"]["requests"], false,
        "a baseline uses no requests"
    );
}

#[tokio::test]
async fn refuses_bad_requests_and_a_second_active_run() {
    let state = state(None);
    for body in [
        json!({"steps": 0}),
        json!({"steps": 999_999}),
        json!({"start_s": -1.0}),
        json!({"warmup_s": 1e9}),
        json!({"start_s": 3600.0, "warmup_s": 86400.0}),
        json!({"nope": 1}),
    ] {
        let (status, err) = call(&state, "POST", "/api/runs", Some(body.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(err["code"], "bad-request");
    }
    // A run registered but not yet finished blocks another (BOPTEST has one worker).
    let held = state.runs().create(&RunRequest::default()).unwrap();
    let (status, err) = call(&state, "POST", "/api/runs", Some(json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(err["code"], "run-active");
    assert!(err["message"].as_str().unwrap().contains(&held.id));
    let (status, err) = call(&state, "GET", "/api/runs/nope/events", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err["code"], "unknown-run");
}

#[tokio::test]
async fn stop_ends_the_run_between_steps() {
    // 50 ms per step: the run is still going when the stop arrives.
    let state = paced(None, std::time::Duration::from_millis(50));
    let (_, info) = call(&state, "POST", "/api/runs", Some(json!({"steps": 30}))).await;
    let id = info["id"].as_str().unwrap().to_owned();
    // Wait for the first tick (verification comes first and can be slow on CI), then stop.
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
            if info["ticks"].as_u64().unwrap() >= 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("no tick within 60 s");
    let (status, _) = call(&state, "POST", &format!("/api/runs/{id}/stop"), None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let all = events(&state, &id, None).await;
    assert_eq!(all.last().unwrap().2["state"], "stopped");
    let ticks = all.iter().filter(|(_, k, _)| k == "tick").count();
    assert!((1..30).contains(&ticks), "{ticks} ticks");
    // Once it has stopped, another run may start.
    let (status, _) = call(&state, "POST", "/api/runs", Some(json!({"steps": 1}))).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn a_reconnect_after_the_end_gets_no_content() {
    let state = state(None);
    let (_, info) = call(&state, "POST", "/api/runs", Some(json!({"steps": 2}))).await;
    let id = info["id"].as_str().unwrap().to_owned();
    let all = events(&state, &id, None).await;
    let last = all.last().unwrap().0;
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/runs/{id}/events"))
                .header("last-event-id", last.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_mid_run_subscriber_gets_backlog_then_live_without_gaps() {
    let state = paced(None, std::time::Duration::from_millis(20));
    let (_, info) = call(&state, "POST", "/api/runs", Some(json!({"steps": 15}))).await;
    let id = info["id"].as_str().unwrap().to_owned();
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
            if info["ticks"].as_u64().unwrap() >= 3 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let all = events(&state, &id, None).await;
    for (i, (seq, _, _)) in all.iter().enumerate() {
        assert_eq!(*seq, i as u64);
    }
    assert_eq!(all.iter().filter(|(_, k, _)| k == "tick").count(), 15);
    assert_eq!(all.last().unwrap().2["state"], "completed");
}

#[tokio::test]
async fn shutdown_ends_open_streams_and_stops_the_run() {
    let state = paced(None, std::time::Duration::from_millis(50));
    let (_, info) = call(&state, "POST", "/api/runs", Some(json!({"steps": 30}))).await;
    let id = info["id"].as_str().unwrap().to_owned();
    let reader = {
        let state = state.clone();
        let id = id.clone();
        tokio::spawn(async move { events(&state, &id, None).await })
    };
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    state.shutdown();
    // The stream closes promptly, whether or not it saw the ended event.
    let seen = tokio::time::timeout(std::time::Duration::from_secs(10), reader)
        .await
        .expect("stream still open after shutdown")
        .unwrap();
    assert!(!seen.is_empty());
    // The run itself stops at its next step.
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while state.runs().active().is_some() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("run still active after shutdown");
    let (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
    assert_eq!(info["state"], "stopped");
}

/// A stop sent before the run task holds a stop receiver (here: before it has even started,
/// since the test runtime is single-threaded) must still be seen. With watch::send it was
/// dropped and the run completed.
#[tokio::test]
async fn a_stop_before_the_run_task_starts_is_not_lost() {
    let state = state(None);
    let (_, info) = call(&state, "POST", "/api/runs", Some(json!({"steps": 30}))).await;
    let id = info["id"].as_str().unwrap().to_owned();
    let (status, _) = call(&state, "POST", &format!("/api/runs/{id}/stop"), None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let all = events(&state, &id, None).await;
    assert_eq!(all.last().unwrap().2["state"], "stopped");
    assert_eq!(all.iter().filter(|(_, k, _)| k == "tick").count(), 0);
}

/// Shutdown before any stream or run task is listening still takes effect.
#[tokio::test]
async fn shutdown_before_anyone_listens_still_stops_the_run() {
    let state = state(None);
    let (_, info) = call(&state, "POST", "/api/runs", Some(json!({"steps": 30}))).await;
    let id = info["id"].as_str().unwrap().to_owned();
    state.shutdown();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while state.runs().active().is_some() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("run still active after shutdown");
    let (_, info) = call(&state, "GET", &format!("/api/runs/{id}"), None).await;
    assert_eq!(info["state"], "stopped");
}

/// A NaN on one air handler's supply temperature degrades that handler's sequence, the reset
/// loses its request input, and both setpoints are handed back to BOPTEST; when the fault
/// ends, the sequences write again. Every faulted step says it was injected.
#[tokio::test]
async fn an_injected_fault_degrades_releases_and_recovers() {
    let state = state(None);
    let body = json!({"steps": 14, "fault": {"point": "floor1_reaAHU_TSup_y", "from_step": 4, "steps": 3}});
    let (status, info) = call(&state, "POST", "/api/runs", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    assert_eq!(info["config"]["fault"]["point"], "floor1_reaAHU_TSup_y");
    let all = events(&state, info["id"].as_str().unwrap(), None).await;
    let ticks: Vec<&Value> = all
        .iter()
        .filter(|(_, k, _)| k == "tick")
        .map(|(_, _, d)| d)
        .collect();
    assert_eq!(ticks.len(), 14);
    let kinds = |t: &Value| -> Vec<String> {
        t["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                format!(
                    "{} {}",
                    e["kind"].as_str().unwrap(),
                    e.get("sequence")
                        .or(e.get("point"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                )
            })
            .collect()
    };
    for (i, t) in ticks.iter().enumerate() {
        let k = kinds(t);
        let faulted = (4..7).contains(&i);
        assert_eq!(
            k.contains(&"injected floor1_reaAHU_TSup_y".to_owned()),
            faulted,
            "step {i}: {k:?}"
        );
        if faulted {
            assert!(k.contains(&"degraded ahu1".to_owned()), "step {i}: {k:?}");
            assert!(k.contains(&"degraded chw".to_owned()), "step {i}: {k:?}");
            assert!(
                k.iter()
                    .any(|e| e.starts_with("released oveChiWatSys_TWSet")),
                "step {i}: {k:?}"
            );
            assert!(
                t["readings"].get("floor1_reaAHU_TSup_y").is_none(),
                "a NaN shows as no data"
            );
            assert_eq!(t["writes"]["oveChiWatSys_TWSet_activate"], 0.0, "step {i}");
        }
    }
    // After the fault the reset writes again.
    assert_eq!(ticks[8]["writes"]["oveChiWatSys_TWSet_activate"], 1.0);
    assert!(
        kinds(ticks[8]).iter().all(|e| !e.starts_with("degraded")),
        "{:?}",
        kinds(ticks[8])
    );
}

#[tokio::test]
async fn bad_faults_are_refused() {
    let state = state(None);
    for (fault, extra, why) in [
        (
            json!({"point": "reaChiWatSys_dp_y", "from_step": 1, "steps": 2}),
            json!({}),
            "not a sequence input",
        ),
        (
            json!({"point": "floor1_reaAHU_TSup_y", "from_step": 1, "steps": 0}),
            json!({}),
            "zero steps",
        ),
        (
            json!({"point": "floor1_reaAHU_TSup_y", "from_step": 1, "steps": 61}),
            json!({}),
            "too long",
        ),
        (
            json!({"point": "floor1_reaAHU_TSup_y", "from_step": 1, "steps": 2, "value": 5}),
            json!({}),
            "unknown field",
        ),
        (
            json!({"point": "floor1_reaAHU_TSup_y", "from_step": 1, "steps": 2}),
            json!({"baseline": true}),
            "baseline",
        ),
        (
            json!({"point": "floor1_reaAHU_TSup_y", "from_step": 20, "steps": 2}),
            json!({"steps": 20}),
            "starts after the run",
        ),
        (
            json!({"point": "floor1_reaAHU_TSup_y", "from_step": 1, "steps": 2}),
            json!({"requests": false}),
            "unused without requests",
        ),
    ] {
        let mut body = extra.clone();
        body["fault"] = fault;
        let (status, err) = call(&state, "POST", "/api/runs", Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}: {err}");
    }
}

/// Without the request sequences, the reset reads occupancy: a NaN there degrades it too.
#[tokio::test]
async fn an_occupancy_fault_without_requests_degrades_the_reset() {
    let state = state(None);
    let body = json!({"steps": 6, "requests": false,
        "fault": {"point": "floor2_reaAHU_occ_y", "from_step": 2, "steps": 2}});
    let (status, info) = call(&state, "POST", "/api/runs", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{info}");
    let all = events(&state, info["id"].as_str().unwrap(), None).await;
    let ticks: Vec<&Value> = all
        .iter()
        .filter(|(_, k, _)| k == "tick")
        .map(|(_, _, d)| d)
        .collect();
    assert_eq!(ticks.len(), 6);
    for (i, t) in ticks.iter().enumerate() {
        let degraded = t["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "degraded" && e["sequence"] == "chw");
        assert_eq!(degraded, (2..4).contains(&i), "step {i}: {}", t["events"]);
    }
}
