//! `mm-server`: serves the API on `MM_ADDR` (default `127.0.0.1:8080`) from the specs in
//! `MM_SPECS_DIR` (default `specs`). Live runs drive BOPTEST at `MM_BOPTEST_URL` (no default:
//! unset, live runs are refused and only replays are offered; `make boptest-up` serves
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
        // No default: without a BOPTEST named, live runs are refused rather than sent to
        // whatever happens to listen on port 80. Replays work either way.
        plant: match std::env::var("MM_BOPTEST_URL") {
            Ok(url) if !url.trim().is_empty() => PlantSource::Boptest {
                url: url.trim().to_owned(),
            },
            _ => PlantSource::Off,
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
            mm_agent::SparkClient::new(key.trim().to_owned())
                .context("building the model client")?,
        )),
        _ => None,
    };
    tracing::info!(live_model = spark.is_some(), "authoring");
    if !runs.plant.is_live() {
        tracing::info!("no MM_BOPTEST_URL: live runs are off, replays are available");
    }
    let authoring = AuthoringConfig {
        spark,
        recordings: Some(PathBuf::from(
            std::env::var("MM_RECORDINGS_DIR").unwrap_or_else(|_| "fixtures/recordings".into()),
        )),
        records: Some(records.clone()),
        drafts: Some(records.join("drafts")),
        explanations: Some(PathBuf::from(
            std::env::var("MM_EXPLANATIONS_DIR").unwrap_or_else(|_| "fixtures/explanations".into()),
        )),
        readings: Some(PathBuf::from(
            std::env::var("MM_READINGS_DIR").unwrap_or_else(|_| "fixtures/readings".into()),
        )),
        bindings: Some(PathBuf::from(
            std::env::var("MM_BINDINGS_DIR").unwrap_or_else(|_| "fixtures/bindings".into()),
        )),
        attacks: Some(PathBuf::from(
            std::env::var("MM_ATTACKS_DIR").unwrap_or_else(|_| "fixtures/attacks".into()),
        )),
        ..AuthoringConfig::default()
    };
    let state = AppState::build(specs, palette, runs, authoring);
    mm_server::workbench::restore(&state);
    mm_server::bind_routes::restore(&state);
    if let Ok(dir) = std::env::var("MM_WEB_DIR") {
        tracing::info!(%dir, "serving the web app");
        state.set_web(PathBuf::from(dir));
    }
    if let Ok(bacnet) = std::env::var("MM_BACNET_ADDR") {
        start_bacnet(&state, &bacnet).await?;
    }
    let on_signal = state.clone();
    let (signalled, first) = tokio::sync::oneshot::channel::<()>();
    let serve = axum::serve(listener, app(state.clone())).with_graceful_shutdown(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!(
            "stopping: closing event streams and asking active runs to stop (Ctrl-C again to exit now)"
        );
        on_signal.shutdown();
        let _ = signalled.send(());
    });
    // Tokio's handler replaces the default SIGINT behaviour for good, so a second Ctrl-C is only
    // heard if something listens for it after the first.
    let second = async move {
        if first.await.is_ok() {
            let _ = tokio::signal::ctrl_c().await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    tokio::pin!(second);
    tokio::select! {
        served = serve => served.context("serving")?,
        () = &mut second => {
            let left = mm_server::shutdown::unfinished(state.runs());
            mm_server::shutdown::report(&mm_server::shutdown::Drained::Interrupted(left));
            return Ok(());
        }
    }
    // Give active runs time to finish their step and stop their BOPTEST test, so the worker is
    // not left holding an orphan. An initializing run cannot be stopped safely; say so.
    let outcome =
        mm_server::shutdown::drain(state.runs(), std::time::Duration::from_secs(600), second).await;
    mm_server::shutdown::report(&outcome);
    Ok(())
}

/// Starts the read-only BACnet mirror on `ip:port` (47808 for BACnet/IP), as device 599.
async fn start_bacnet(state: &AppState, addr: &str) -> anyhow::Result<()> {
    let addr: std::net::SocketAddrV4 = addr
        .parse()
        .context("MM_BACNET_ADDR must be an IPv4 ip:port")?;
    let map =
        mm_server::bacnet::point_map(state.specs()).context("building the BACnet point map")?;
    let config = mm_bacnet::MirrorConfig {
        device_instance: 599,
        device_name: "MECHANICAL-MUSE".into(),
        interface: *addr.ip(),
        port: addr.port(),
        broadcast: std::net::Ipv4Addr::BROADCAST,
    };
    let mirror = mm_bacnet::Mirror::start(&config, map.clone())
        .await
        .context("starting the BACnet mirror")?;
    let info = mm_server::bacnet::info(&map, config.device_instance, addr.port());
    tracing::info!(device = config.device_instance, %addr, objects = info.points.len(), "BACnet mirror (read-only)");
    state.set_bacnet(mm_server::bacnet::BacnetMirror { mirror, info });
    Ok(())
}
