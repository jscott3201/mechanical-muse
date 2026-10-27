//! The built web app served by the API process: files, app routes, and `/api` left alone.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mm_graph::Palette;
use mm_server::{AppState, SpecStore, app};
use tower::ServiceExt;

async fn get(state: &AppState, uri: &str) -> (StatusCode, String, String) {
    let response = app(state.clone())
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let kind = response
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    (status, kind, body)
}

#[tokio::test]
async fn the_app_is_served_beside_the_api() {
    let dist =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("dist-{}", std::process::id()));
    std::fs::create_dir_all(dist.join("assets")).unwrap();
    std::fs::write(dist.join("index.html"), "<!doctype html><title>MM</title>").unwrap();
    std::fs::write(dist.join("assets/app.js"), "console.log(1)").unwrap();
    let palette = Palette::pinned().unwrap();
    let specs = SpecStore::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../specs"),
        &palette,
    )
    .unwrap();
    let state = AppState::new(specs, palette);

    // Without a web folder, unknown paths are JSON 404s as before.
    let (status, kind, _) = get(&state, "/").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(kind.starts_with("application/json"), "{kind}");

    state.set_web(dist.clone());
    let (status, kind, body) = get(&state, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(kind.starts_with("text/html") && body.contains("<title>MM</title>"));
    let (_, kind, body) = get(&state, "/assets/app.js").await;
    assert!(kind.starts_with("text/javascript") && body.contains("console.log"));
    // An app route is the app.
    let (status, _, body) = get(&state, "/runs/run-1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<title>MM</title>"));
    // The API is unchanged: real routes answer, unknown ones are JSON 404s, not the app.
    let (status, _, _) = get(&state, "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    let (status, kind, _) = get(&state, "/api/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(kind.starts_with("application/json"), "{kind}");
    // Nothing outside the folder.
    let (_, _, body) = get(&state, "/../Cargo.toml").await;
    assert!(!body.contains("[package]"));
    let _ = std::fs::remove_dir_all(dist);
}
