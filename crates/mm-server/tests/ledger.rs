//! The ledger routes over recorded runs (committed replays and a synthetic full record), runs
//! on this server, and their refusals.

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

fn state(replays: PathBuf) -> AppState {
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
            replays: Some(replays),
            replay_pace: Duration::ZERO,
        },
    )
}

async fn get(state: &AppState, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let response = app(state.clone()).oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn near(v: &Value, expected: f64, tol: f64) -> bool {
    (v.as_f64().unwrap() - expected).abs() <= tol
}

/// The day-143 morning numbers from the committed replays: the exact split of BOPTEST's KPIs.
#[tokio::test]
async fn a_recorded_baseline_splits_into_scopes_that_reprice_to_boptests_cost() {
    let state = state(repo("fixtures/replays"));
    let (status, l) = get(&state, "/api/replays/day143-morning-baseline/ledger").await;
    assert_eq!(status, StatusCode::OK, "{l}");
    assert_eq!(l["subject"]["reference"], "replay:day143-morning-baseline");
    assert!(
        near(&l["scope1"]["kg_co2e"], 1644.0, 0.5),
        "{}",
        l["scope1"]
    );
    assert!(
        near(&l["scope2"]["kg_co2e"], 2705.0, 0.5),
        "{}",
        l["scope2"]
    );
    assert!(near(&l["total"]["cost"], 963.53, 0.005));
    assert!(near(&l["boptest_cost"], 963.53, 0.005));
    assert_eq!(l["factors"]["currency"], "USD");
    assert_eq!(l["reconciles"], true);
    // Recorded before the fan and boiler readings: scope totals only.
    assert_eq!(l["measured"], false);
    assert_eq!(l["end_uses"], json!([]));
    assert_eq!(
        l["missing"].as_array().unwrap().len(),
        11,
        "{}",
        l["missing"]
    );
    assert_eq!(l["factors"]["gas_kg_per_kwh"], 0.18108);
}

#[tokio::test]
async fn two_runs_of_one_window_compare_and_others_are_refused() {
    let state = state(repo("fixtures/replays"));
    let (status, c) = get(
        &state,
        "/api/ledger/compare?a=replay:day143-morning-baseline&b=replay:day143-morning-your-sequence-rev-b",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{c}");
    assert!(near(&c["b"]["scope1"]["kg_co2e"], 796.0, 0.5));
    assert!(near(&c["b"]["scope2"]["kg_co2e"], 2138.0, 0.5));
    assert!(near(&c["b"]["total"]["cost"], 694.97, 0.005));
    let d = &c["delta"];
    let expect = c["b"]["scope1"]["kg_co2e"].as_f64().unwrap()
        - c["a"]["scope1"]["kg_co2e"].as_f64().unwrap();
    assert!(near(&d["scope1"]["kg_co2e"], expect, 1e-9));
    assert!(d["total"]["cost"].as_f64().unwrap() < -268.0, "{d}");
    assert!(
        d["tdis_tot"].as_f64().unwrap() < 0.0,
        "comfort improved too"
    );

    for (uri, status, code) in [
        (
            "/api/ledger/compare?a=replay:day143-morning-baseline&b=replay:day185-noon-baseline",
            StatusCode::BAD_REQUEST,
            "window-mismatch",
        ),
        (
            "/api/ledger/compare?a=day143-morning-baseline&b=replay:day143-morning-requests",
            StatusCode::BAD_REQUEST,
            "bad-reference",
        ),
        (
            "/api/ledger/compare?a=replay:x",
            StatusCode::BAD_REQUEST,
            "bad-request",
        ),
        (
            "/api/ledger/compare?a=replay:nope&b=replay:day143-morning-requests",
            StatusCode::NOT_FOUND,
            "unknown-record",
        ),
        (
            "/api/replays/..%2Fsecret/ledger",
            StatusCode::NOT_FOUND,
            "unknown-record",
        ),
        (
            "/api/runs/nope/ledger",
            StatusCode::NOT_FOUND,
            "unknown-run",
        ),
    ] {
        let (s, body) = get(&state, uri).await;
        assert_eq!(
            (s, body["code"].as_str()),
            (status, Some(code)),
            "{uri}: {body}"
        );
    }
}

/// A record with every KPI power signal in every tick: end uses, and the remainder unattributed.
fn full_record(dir: &std::path::Path) {
    let config = json!({"baseline": false, "requests": true, "start_s": 0.0, "warmup_s": 0.0,
        "steps": 3, "step_s": 120.0, "case": "multizone_office_complex_air", "drafts": {}});
    let mut lines = vec![
        json!({"run_id": "r", "seq": 0, "kind": "started", "config": config, "sequences": []}),
    ];
    for k in 0..3 {
        let mut readings = serde_json::Map::new();
        for p in mm_runtime::ledger::kpi_points() {
            readings.insert(p, json!(1000.0));
        }
        lines.push(
            json!({"run_id": "r", "seq": k + 1, "sim_time_s": 120.0 * f64::from(k),
            "kind": "tick", "outputs": {}, "writes": {}, "readings": readings, "events": []}),
        );
    }
    lines.push(json!({"run_id": "r", "seq": 4, "kind": "kpi", "values": {
        "ener_tot": 0.39785664748724936, "emis_tot": 0.10171230167699279,
        "cost_tot": 0.022534986481290514, "tdis_tot": 0.1192}}));
    lines.push(json!({"run_id": "r", "seq": 5, "kind": "ended", "state": "completed"}));
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(dir.join("full.jsonl"), text).unwrap();
}

#[tokio::test]
async fn a_full_record_has_end_uses_and_a_replayed_run_has_the_same_ledger() {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "ledger-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    full_record(&dir);
    let state = state(dir);
    let (status, l) = get(&state, "/api/replays/full/ledger").await;
    assert_eq!(status, StatusCode::OK, "{l}");
    assert_eq!(l["measured"], true);
    let rows = l["end_uses"].as_array().unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[7]["id"], "boiler");
    assert_eq!(rows[7]["scope"], 1);
    assert_eq!(rows[9]["id"], "unattributed_gas");
    assert_eq!(rows[9]["measured"], false);
    let gas_rows: f64 = rows
        .iter()
        .filter(|r| r["scope"] == 1)
        .map(|r| r["amounts"]["kwh"].as_f64().unwrap())
        .sum();
    assert!(near(&l["scope1"]["kwh"], gas_rows, 1e-9));

    // Replayed on this server, the run's own ledger matches the record's.
    let req = Request::builder()
        .method("POST")
        .uri("/api/runs/replay")
        .header("content-type", "application/json")
        .body(Body::from(json!({"record": "full"}).to_string()))
        .unwrap();
    let response = app(state.clone()).oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let id = serde_json::from_slice::<Value>(&bytes).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut run = Value::Null;
    for _ in 0..500 {
        let (s, body) = get(&state, &format!("/api/runs/{id}/ledger")).await;
        if s == StatusCode::OK {
            run = body;
            break;
        }
        assert!(
            matches!(body["code"].as_str(), Some("no-kpis" | "not-started")),
            "{body}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(run["subject"]["reference"], format!("run:{id}"));
    assert_eq!(run["subject"]["replay_of"], "full");
    assert_eq!(run["end_uses"], l["end_uses"]);
    assert_eq!(run["scope2"], l["scope2"]);
    let (s, c) = get(
        &state,
        &format!("/api/ledger/compare?a=replay:full&b=run:{id}"),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["delta"]["total"]["kwh"], 0.0);
}

/// The home's ledger: all electric (no Scope 1), BOPTEST's euro cost and its constant factor,
/// and a typical-window comparison against the home baseline.
#[tokio::test]
async fn the_home_ledger_is_scope_2_only_in_euros() {
    let state = state(repo("fixtures/replays"));
    let (status, l) = get(&state, "/api/replays/home-day108-sequence/ledger").await;
    assert_eq!(status, StatusCode::OK, "{l}");
    assert_eq!(l["factors"]["currency"], "EUR");
    assert_eq!(l["factors"]["electricity_kg_per_kwh"], 0.167);
    assert!(
        l["factors"].get("gas_kg_per_kwh").is_none(),
        "{}",
        l["factors"]
    );
    assert!(l["factors"].get("electricity_price_per_kwh").is_none());
    assert_eq!(l["scope1"]["kwh"], 0.0);
    assert_eq!(l["reconciles"], true);
    assert!(near(&l["scope2"]["kwh"], 314.3, 0.05), "{}", l["scope2"]);
    assert!(near(&l["boptest_cost"], 72.59, 0.005));
    assert!(near(&l["average_price_per_kwh"], 0.2310, 0.0001));
    // Three end uses and one unattributed electricity row.
    assert_eq!(
        l["end_uses"].as_array().unwrap().len(),
        4,
        "{}",
        l["end_uses"]
    );

    let (status, c) = get(
        &state,
        "/api/ledger/compare?a=replay:home-day108-baseline&b=replay:home-day108-sequence",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{c}");
    let d = &c["delta"];
    assert!(near(&d["total"]["kwh"], 314.3 - 340.4, 0.1), "{d}");
    assert!(near(&d["total"]["cost"], 72.59 - 79.21, 0.01), "{d}");
    assert!(
        near(&d["total"]["kg_co2e"], 0.167 * (314.3 - 340.4), 0.02),
        "{d}"
    );
}
