//! The router against the repo's real specs, driven in-process (no socket).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mm_graph::Palette;
use mm_server::{AppState, ENGINE_REV, SpecStore, app};
use serde_json::Value;
use tower::ServiceExt;

fn specs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../specs")
}

fn router() -> axum::Router {
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(&specs_dir(), &palette).unwrap();
    app(AppState::new(specs, palette))
}

async fn call(method: &str, uri: &str) -> (StatusCode, Value) {
    let response = router()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn health_reports_the_pinned_engine() {
    let (status, body) = call("GET", "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["engine_rev"], ENGINE_REV);
    assert!(body["specs"].as_u64().unwrap() >= 2);
}

#[test]
fn engine_rev_matches_the_lockfile() {
    let lock =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"))
            .unwrap();
    assert!(
        lock.contains(ENGINE_REV),
        "Cargo.lock pins another engine rev"
    );
}

#[tokio::test]
async fn lists_specs_sorted_with_their_interfaces() {
    let (status, body) = call("GET", "/api/specs").await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
    assert!(names.contains(&"chw_plant_reset") && names.contains(&"ahu_chw_requests"));
    let chw = body
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "chw_plant_reset")
        .unwrap();
    assert_eq!(chw["title"], "Chilled-water plant reset");
    assert_eq!(chw["inputs"][0]["name"], "nReqRes");
    assert_eq!(chw["inputs"][0]["type"], "int");
    assert!(
        chw["inputs"][0].get("value").is_none(),
        "points carry no value"
    );
    assert_eq!(chw["scenarios"], 10);
}

#[tokio::test]
async fn detail_has_params_and_scenarios() {
    let (status, body) = call("GET", "/api/specs/ahu_chw_requests").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["step_s"], 60.0);
    let params = body["params"].as_array().unwrap();
    let high3 = params.iter().find(|p| p["name"] == "high3_error").unwrap();
    assert_eq!(high3["value"], 3.0);
    assert!(
        high3.get("type").is_none(),
        "parameters carry no signal type"
    );
    assert_eq!(body["scenarios"].as_array().unwrap().len(), 15);
    assert!(
        body["description"]
            .as_str()
            .unwrap()
            .contains("persistence")
    );
}

#[tokio::test]
async fn graph_ports_come_from_the_catalog() {
    let (status, body) = call("GET", "/api/specs/ahu_chw_requests/graph").await;
    assert_eq!(status, StatusCode::OK);
    let block = |id: &str| {
        body["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == id)
            .unwrap()
            .clone()
    };
    // A threshold takes a Real and gives a Boolean; a latch has both inputs, in catalog order.
    let high3 = block("high3");
    assert_eq!(high3["inputs"][0]["signal"], "Real");
    assert_eq!(high3["outputs"][0]["signal"], "Boolean");
    let latch = block("resetLatch");
    let names: Vec<&str> = latch["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["u", "clr"]);
    // Connections carry their source's type; boundary ends have no port.
    let conns = body["connections"].as_array().unwrap();
    let from_input = conns
        .iter()
        .find(|c| c["from"]["node"] == "uCooCoi")
        .unwrap();
    assert!(from_input["from"].get("port").is_none());
    assert_eq!(from_input["signal"], "Real");
    let to_output = conns
        .iter()
        .find(|c| c["to"]["node"] == "yChiWatResReq")
        .unwrap();
    assert_eq!(to_output["signal"], "Integer");
    assert_eq!(body["outputs"][0]["signal"], "Integer");
}

#[tokio::test]
async fn verify_returns_a_passing_receipt() {
    for name in ["chw_plant_reset", "ahu_chw_requests"] {
        let (status, body) = call("POST", &format!("/api/specs/{name}/verify")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["passed"], true, "{body}");
        assert!(!body["content_id"].as_str().unwrap().is_empty());
        let scenarios = body["scenarios"].as_array().unwrap();
        assert!(scenarios.iter().all(|s| s["result"] == "pass"));
        assert!(scenarios[0]["ticks"].as_u64().unwrap() > 0);
    }
}

#[tokio::test]
async fn verify_is_deterministic() {
    let (_, a) = call("POST", "/api/specs/chw_plant_reset/verify").await;
    let (_, b) = call("POST", "/api/specs/chw_plant_reset/verify").await;
    assert_eq!(a, b);
}

#[tokio::test]
async fn unknown_spec_and_route_are_404_with_codes() {
    for uri in ["/api/specs/nope", "/api/specs/nope/graph"] {
        let (status, body) = call("GET", uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "unknown-spec");
    }
    let (status, body) = call("POST", "/api/specs/nope/verify").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "unknown-spec");
    let (status, body) = call("GET", "/api/nothing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not-found");
}
