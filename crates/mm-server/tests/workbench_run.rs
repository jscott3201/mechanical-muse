//! An engineer's own sequence running on the (scripted) building: the sample sheet's recorded
//! reading, accepted; Spark's recorded draft, verified; Spark's recorded BOPTEST binding,
//! confirmed; then a run that loads one instance per floor and writes each floor's supply air
//! temperature setpoint within the binding's limits. No model key, no BOPTEST.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use http_body_util::BodyExt;
use mm_graph::Palette;
use mm_runtime::Measurements;
use mm_server::{AppState, AuthoringConfig, PlantSource, RunsConfig, SpecStore, app};
use serde_json::{Value, json};
use tower::ServiceExt;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// A mild morning: occupied, 18 °C (64.4 °F) outside, dry return air.
fn readings(time: f64) -> Measurements {
    let mut m: Measurements = [("time".to_owned(), time)].into_iter().collect();
    for f in 1..=3 {
        m.insert(format!("floor{f}_reaAHU_TSup_y"), 286.5);
        m.insert(format!("floor{f}_TSupAirSet_u"), 285.95);
        m.insert(format!("floor{f}_reaAHU_yCooVal_y"), 0.4);
        m.insert(format!("floor{f}_reaAHU_occ_y"), 1.0);
        m.insert(format!("floor{f}_reaAHU_phiAHURet_y"), 0.45);
    }
    m.insert("weaSta_reaWeaTDryBul_y".into(), 291.15);
    m.insert("reaChiWatSys_TCHWSup_y".into(), 278.7);
    m
}

fn server(records: &std::path::Path) -> AppState {
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(&repo("specs"), &palette).unwrap();
    AppState::build(
        specs,
        palette,
        RunsConfig {
            plant: PlantSource::Scripted {
                readings: (0..=40).map(|k| readings(120.0 * f64::from(k))).collect(),
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

async fn until_done(state: &AppState, path: &str) -> Value {
    for _ in 0..1000 {
        let (_, info) = call(state, "GET", path, None).await;
        if !matches!(info["state"].as_str(), Some("running" | "starting")) {
            return info;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{path} did not finish");
}

#[tokio::test]
async fn your_own_sequence_runs_on_every_floor_once_bound() {
    let records = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("workbench-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&records);
    let state = server(&records);

    let png = std::fs::read(repo("fixtures/sequences/ahu-sat-reset-sample.png")).unwrap();
    let sample = json!({"attachments": [{"name": "s.png", "mime": "image/png",
        "data_base64": base64::engine::general_purpose::STANDARD.encode(png)}]});
    let (_, view) = call(&state, "POST", "/api/workbench/read", Some(sample)).await;
    let name = view["draft"]["name"].as_str().unwrap().to_owned();
    let (status, _) = call(
        &state,
        "POST",
        "/api/workbench/accept",
        Some(json!({"draft": view["draft"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, session) = call(
        &state,
        "POST",
        &format!("/api/specs/{name}/author"),
        Some(json!({"source": "recorded"})),
    )
    .await;
    let ended = until_done(
        &state,
        &format!("/api/authoring/{}", session["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(ended["state"], "verified", "{ended}");
    let (_, drafts) = call(&state, "GET", "/api/drafts", None).await;
    let draft = drafts
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["spec"] == name.as_str())
        .unwrap()["content_id"]
        .clone();

    // Not bound yet: a run cannot use it.
    let request = json!({"drafts": {name.clone(): draft}, "requests": false, "steps": 10});
    let (status, body) = call(&state, "POST", "/api/runs", Some(request.clone())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"].as_str().unwrap().contains("not bound"),
        "{body}"
    );

    // Spark's recorded proposal, labelled; a broken edit is refused; the proposal is confirmed.
    let (status, proposal) = call(
        &state,
        "POST",
        &format!("/api/workbench/{name}/binding/propose"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposal}");
    assert_eq!(proposal["proposed_by"], "recorded");
    assert_eq!(proposal["problems"], json!([]));
    let mut broken = proposal["binding"].clone();
    broken["outputs"][0]["max"] = json!(400.0);
    let (status, _) = call(
        &state,
        "POST",
        &format!("/api/workbench/{name}/binding"),
        Some(json!({"binding": broken})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, confirmed) = call(
        &state,
        "POST",
        &format!("/api/workbench/{name}/binding"),
        Some(json!({"binding": proposal["binding"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{confirmed}");
    assert_eq!(confirmed["confirmed"], true);
    let (_, bound) = call(&state, "GET", "/api/workbench/bindings", None).await;
    assert!(bound[name.as_str()].is_object(), "{bound}");

    // A binding that would write the chilled-water setpoint our own reset writes is refused at
    // the start, not on the first tick.
    let mut clash = proposal["binding"].clone();
    clash["floors"] = json!([1]);
    clash["outputs"][0]["point"] = json!("oveChiWatSys_TWSet");
    clash["outputs"][0]["measured"] = json!("");
    clash["outputs"][0]["min"] = json!(278.15);
    clash["outputs"][0]["max"] = json!(288.15);
    let (status, body) = call(
        &state,
        "POST",
        &format!("/api/workbench/{name}/binding"),
        Some(json!({"binding": clash})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the clash is only visible with the run: {body}"
    );
    let (status, body) = call(&state, "POST", "/api/runs", Some(request.clone())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("own sequences write"),
        "{body}"
    );
    let (status, _) = call(
        &state,
        "POST",
        &format!("/api/workbench/{name}/binding"),
        Some(json!({"binding": proposal["binding"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, run) = call(&state, "POST", "/api/runs", Some(request)).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let id = run["id"].as_str().unwrap().to_owned();
    let info = until_done(&state, &format!("/api/runs/{id}")).await;
    assert_eq!(info["state"], "completed", "{info}");

    let events = state.runs().get(&id).unwrap().journal().since(None);
    let all: Vec<Value> = events
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
    let started = all.iter().find(|e| e["kind"] == "started").unwrap();
    assert!(
        started["config"]["bindings"][name.as_str()].is_object(),
        "the run records the binding"
    );
    let ours: Vec<&Value> = started["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["spec"] == name.as_str())
        .collect();
    assert_eq!(ours.len(), 3, "one instance per floor");
    assert!(
        ours.iter()
            .all(|s| s["origin"] == "drafted" && s["content_id"] == draft)
    );

    // 64.4 °F outside is inside the sheet's 60–70 °F band: after the 10-minute hold each floor
    // sends the reset line's value, 65 °F - 4.4 °F = 60.6 °F (289.039 K), exactly as computed and
    // without a clamp.
    let ticks: Vec<&Value> = all.iter().filter(|e| e["kind"] == "tick").collect();
    let last = ticks.last().unwrap();
    for f in 1..=3 {
        let sent = last["writes"][format!("floor{f}_TSupAirSet_u")]
            .as_f64()
            .unwrap();
        let computed = last["outputs"][format!("{name}_floor{f}")]["datStpt"]
            .as_f64()
            .unwrap();
        assert!((sent - 289.039).abs() < 0.01, "floor {f} sent {sent}");
        assert!(
            (sent - computed).abs() < 1e-9,
            "floor {f}: sent {sent}, computed {computed}"
        );
        assert!(last["readings"][format!("floor{f}_reaAHU_phiAHURet_y")].is_number());
    }
    let clamped = ticks
        .iter()
        .flat_map(|t| t["events"].as_array().unwrap())
        .filter(|e| {
            e["kind"] == "clamped"
                && e["sequence"]
                    .as_str()
                    .is_some_and(|s| s.starts_with(name.as_str()))
        })
        .count();
    assert_eq!(
        clamped, 0,
        "the recorded binding's limits include every value it sends"
    );
    let _ = std::fs::remove_dir_all(&records);
}
