//! `mm-server`: serves the API on `MM_ADDR` (default `127.0.0.1:8080`) from the specs in
//! `MM_SPECS_DIR` (default `specs`). Runs drive BOPTEST at `MM_BOPTEST_URL` (default
//! `http://127.0.0.1:80`) and write JSON-lines records to `MM_RUNS_DIR` (default `runs`, which
//! is gitignored). Stops cleanly on Ctrl-C.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use mm_graph::Palette;
use mm_server::{AppState, PlantSource, RunsConfig, SpecStore, app};
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
    let runs = RunsConfig {
        plant: PlantSource::Boptest {
            url: std::env::var("MM_BOPTEST_URL").unwrap_or_else(|_| "http://127.0.0.1:80".into()),
        },
        records: Some(PathBuf::from(
            std::env::var("MM_RUNS_DIR").unwrap_or_else(|_| "runs".into()),
        )),
    };
    let state = AppState::with_runs(specs, palette, runs);
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
