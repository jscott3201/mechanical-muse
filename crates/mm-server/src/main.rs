//! `mm-server`: serves the API on `MM_ADDR` (default `127.0.0.1:8080`) from the specs in
//! `MM_SPECS_DIR` (default `specs`). Runs drive BOPTEST at `MM_BOPTEST_URL` (default
//! `http://127.0.0.1:80`) and write JSON-lines records to `MM_RUNS_DIR` (default `runs`, which
//! is gitignored). Stops cleanly on Ctrl-C.
//!
//! Authoring calls Muse Spark when `MODEL_API_KEY` is set (`make server-live` maps it from the
//! local `.env`; the key never leaves this process). Recorded runs for the "recorded" source are
//! read from `MM_RECORDINGS_DIR` (default `fixtures/recordings`). Verified drafts are kept in
//! `<MM_RUNS_DIR>/drafts` and session records in `MM_RUNS_DIR`.
//!
//! Recorded runs in `MM_REPLAYS_DIR` (default `fixtures/replays`) can be replayed through the
//! same event stream, one tick every `MM_REPLAY_PACE_MS` (default 250 ms), labelled as replays.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use mm_graph::Palette;
use mm_server::{AppState, AuthoringConfig, PlantSource, RunsConfig, SpecStore, app};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let addr: SocketAddr = std::env::var("MM_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()
        .context("MM_ADDR must be host:port")?;
    let dir = PathBuf::from(std::env::var("MM_SPECS_DIR").unwrap_or_else(|_| "specs".into()));
    let palette = Palette::pinned().context("loading the engine catalog")?;
    let specs = SpecStore::load(&dir, &palette).context("loading specs")?;
    tracing::info!(specs = specs.len(), %addr, "serving");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    let records = PathBuf::from(std::env::var("MM_RUNS_DIR").unwrap_or_else(|_| "runs".into()));
    let runs = RunsConfig {
        plant: PlantSource::Boptest {
            url: std::env::var("MM_BOPTEST_URL").unwrap_or_else(|_| "http://127.0.0.1:80".into()),
        },
        records: Some(records.clone()),
        replays: Some(PathBuf::from(
            std::env::var("MM_REPLAYS_DIR").unwrap_or_else(|_| "fixtures/replays".into()),
        )),
        replay_pace: std::time::Duration::from_millis(
            std::env::var("MM_REPLAY_PACE_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(250),
        ),
    };
    let spark = match std::env::var("MODEL_API_KEY") {
        Ok(key) if !key.trim().is_empty() => Some(std::sync::Arc::new(
            mm_agent::SparkClient::new(key).context("building the model client")?,
        )),
        _ => None,
    };
    tracing::info!(live_model = spark.is_some(), "authoring");
    let authoring = AuthoringConfig {
        spark,
        recordings: Some(PathBuf::from(
            std::env::var("MM_RECORDINGS_DIR").unwrap_or_else(|_| "fixtures/recordings".into()),
        )),
        records: Some(records.clone()),
        drafts: Some(records.join("drafts")),
        ..AuthoringConfig::default()
    };
    let state = AppState::build(specs, palette, runs, authoring);
    let on_signal = state.clone();
    axum::serve(listener, app(state.clone()))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("stopping: closing event streams and asking the active run to stop");
            on_signal.shutdown();
        })
        .await
        .context("serving")?;
    // Give an active run time to finish its step and stop its BOPTEST test, so the worker is
    // not left holding an orphan. An initializing run cannot be stopped safely; say so.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(600);
    while let Some(run) = state.runs().active() {
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(run = %run.id, "run still active at exit; its BOPTEST test may need stopping");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Ok(())
}
