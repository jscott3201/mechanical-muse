//! `mm-mcp`: the MCP server on stdin/stdout. Specs come from `MM_SPECS_DIR` (default `specs`
//! in the working directory, else this repository's `specs/`); recorded runs for the ledger
//! tools from `MM_REPLAYS_DIR` (default `fixtures/replays`, found the same way). Logs go to stderr only
//! (`RUST_LOG`, default `warn`), because stdout is the protocol channel.

use std::path::PathBuf;

use anyhow::Context;
use mm_graph::Palette;
use mm_mcp::{McpServer, Tools};
use mm_server::SpecStore;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

/// `$var`, else `rel` in the working directory, else `rel` in this repository (an MCP client
/// may launch the server from another working directory).
fn repo_dir(var: &str, rel: &str) -> PathBuf {
    if let Ok(dir) = std::env::var(var) {
        return PathBuf::from(dir);
    }
    let local = PathBuf::from(rel);
    if local.is_dir() {
        return local;
    }
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).join(rel)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
        .init();
    let dir = repo_dir("MM_SPECS_DIR", "specs");
    let palette = Palette::pinned().context("loading the engine catalog")?;
    let specs = SpecStore::load(&dir, &palette)
        .with_context(|| format!("loading specs from {}", dir.display()))?;
    tracing::info!(specs = specs.len(), dir = %dir.display(), "mm-mcp ready on stdio");
    let replays = repo_dir("MM_REPLAYS_DIR", "fixtures/replays");
    let server = McpServer::new(Tools::new(palette, specs).with_replays(replays))?;
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .context("starting the MCP session on stdio")?;
    let reason = running.waiting().await.context("MCP session")?;
    tracing::info!(?reason, "mm-mcp stopped");
    Ok(())
}
