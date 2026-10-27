//! `mm-server`: serves the API on `MM_ADDR` (default `127.0.0.1:8080`) from the specs in
//! `MM_SPECS_DIR` (default `specs`). Stops cleanly on Ctrl-C.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use mm_graph::Palette;
use mm_server::{AppState, SpecStore, app};
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
    axum::serve(listener, app(AppState::new(specs, palette)))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("stopping");
        })
        .await
        .context("serving")?;
    Ok(())
}
