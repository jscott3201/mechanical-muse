//! HTTP routes. Handlers stay thin: specs are loaded at startup, verification runs on the
//! blocking pool behind a small semaphore so a burst of requests cannot pin every core.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mm_graph::Palette;
use tokio::sync::Semaphore;

use crate::authoring::{Authoring, AuthoringConfig};
use crate::dto::{ApiError, GraphView, Health, Receipt, SpecDetail, SpecSummary};
use crate::runs::{PlantSource, Runs, RunsConfig};
use crate::specs::SpecStore;

/// open-control-engine commit pinned in the workspace (checked against `Cargo.lock` in tests).
pub const ENGINE_REV: &str = "fa5cb19d4b2b8831fb6a2979bc1d1099f69f3693";

/// Verifications allowed to run at once.
const VERIFY_SLOTS: usize = 2;

/// Accepted engineer sequences one server keeps.
const MAX_WORKBENCH_SPECS: usize = 50;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    specs: SpecStore,
    /// Engineers' own sequences, added at run time from the workbench.
    workbench: std::sync::RwLock<std::collections::BTreeMap<String, Arc<crate::specs::Spec>>>,
    /// One model reading at a time.
    workbench_slot: tokio::sync::Mutex<()>,
    /// Confirmed BOPTEST bindings of engineers' sequences, by spec name.
    bindings: std::sync::RwLock<std::collections::BTreeMap<String, crate::bind::Binding>>,
    palette: Palette,
    verify_slots: Arc<Semaphore>,
    runs: Runs,
    authoring: Authoring,
    explanations: crate::explain::Explanations,
    shutdown: tokio::sync::watch::Sender<bool>,
    bacnet: std::sync::OnceLock<Arc<crate::bacnet::BacnetMirror>>,
    web: std::sync::OnceLock<std::path::PathBuf>,
}

impl AppState {
    /// No live plant (starting a run is refused) and no records on disk.
    pub fn new(specs: SpecStore, palette: Palette) -> Self {
        Self::with_runs(
            specs,
            palette,
            RunsConfig {
                plant: PlantSource::Off,
                records: None,
                replays: None,
                replay_pace: std::time::Duration::ZERO,
            },
        )
    }

    /// No live model and no recordings: authoring requests are refused.
    pub fn with_runs(specs: SpecStore, palette: Palette, runs: RunsConfig) -> Self {
        Self::build(specs, palette, runs, AuthoringConfig::default())
    }

    pub fn build(
        specs: SpecStore,
        palette: Palette,
        runs: RunsConfig,
        authoring: AuthoringConfig,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                specs,
                workbench: std::sync::RwLock::default(),
                workbench_slot: tokio::sync::Mutex::new(()),
                bindings: std::sync::RwLock::default(),
                palette,
                verify_slots: Arc::new(Semaphore::new(VERIFY_SLOTS)),
                runs: Runs::new(runs),
                explanations: crate::explain::Explanations::new(
                    authoring.records.as_ref().map(|d| d.join("explanations")),
                    authoring.explanations.clone(),
                ),
                authoring: Authoring::new(authoring),
                shutdown: tokio::sync::watch::channel(false).0,
                bacnet: std::sync::OnceLock::new(),
                web: std::sync::OnceLock::new(),
            }),
        }
    }

    pub fn explanations(&self) -> &crate::explain::Explanations {
        &self.inner.explanations
    }

    pub fn authoring(&self) -> &Authoring {
        &self.inner.authoring
    }

    pub fn specs(&self) -> &SpecStore {
        &self.inner.specs
    }

    /// A spec by name: one of ours, or an engineer's from the workbench.
    pub fn spec(&self, name: &str) -> Option<crate::specs::Spec> {
        if let Some(s) = self.inner.specs.get(name) {
            return Some(s.clone());
        }
        let wb = self
            .inner
            .workbench
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        wb.get(name).map(|s| s.as_ref().clone())
    }

    /// Adds or replaces an engineer's sequence. Never shadows one of ours.
    /// Adds an engineer's sequence. Never shadows one of ours, and never replaces an accepted
    /// one: drafts verified against its scenarios would then claim a gate they never passed.
    pub fn add_workbench_spec(&self, spec: crate::specs::Spec) -> Result<(), String> {
        let name = spec.detail.summary.name.clone();
        if self.inner.specs.get(&name).is_some() {
            return Err(format!("`{name}` is the name of a built-in sequence"));
        }
        let mut wb = self
            .inner
            .workbench
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if wb.contains_key(&name) {
            return Err(format!(
                "a sequence named `{name}` was already accepted; give this one another name"
            ));
        }
        if wb.len() >= MAX_WORKBENCH_SPECS {
            return Err(format!(
                "at most {MAX_WORKBENCH_SPECS} accepted sequences per server"
            ));
        }
        wb.insert(name, Arc::new(spec));
        Ok(())
    }

    /// An engineer's sequence's confirmed binding to BOPTEST.
    pub fn binding(&self, spec: &str) -> Option<crate::bind::Binding> {
        self.inner
            .bindings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(spec)
            .cloned()
    }

    /// Every confirmed binding, by spec name.
    pub fn bindings(&self) -> std::collections::BTreeMap<String, crate::bind::Binding> {
        self.inner
            .bindings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn set_binding(&self, spec: &str, binding: crate::bind::Binding) {
        self.inner
            .bindings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(spec.to_owned(), binding);
    }

    pub(crate) fn workbench_slot(&self) -> &tokio::sync::Mutex<()> {
        &self.inner.workbench_slot
    }

    /// Engineers' sequences, sorted by name.
    pub fn workbench_specs(&self) -> Vec<crate::dto::SpecSummary> {
        self.inner
            .workbench
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .map(|s| s.detail.summary.clone())
            .collect()
    }

    pub fn palette(&self) -> &Palette {
        &self.inner.palette
    }

    pub fn runs(&self) -> &Runs {
        &self.inner.runs
    }

    /// Attaches the BACnet mirror that live runs update. Only the first call takes effect.
    pub fn set_bacnet(&self, mirror: crate::bacnet::BacnetMirror) {
        let _ = self.inner.bacnet.set(Arc::new(mirror));
    }

    /// Serves the built web app from `dir` on every path outside `/api`. First call wins.
    pub fn set_web(&self, dir: std::path::PathBuf) {
        let _ = self.inner.web.set(dir);
    }

    pub fn bacnet(&self) -> Option<&Arc<crate::bacnet::BacnetMirror>> {
        self.inner.bacnet.get()
    }

    /// Ends every open event stream and asks every active run (live or replay) to stop at its
    /// next step.
    pub fn shutdown(&self) {
        // Stored even with no open stream, so a stream opened afterwards ends at once.
        self.inner.shutdown.send_replace(true);
        for run in self.inner.runs.all_active() {
            run.request_stop();
        }
    }

    pub(crate) fn shutdown_signal(&self) -> tokio::sync::watch::Receiver<bool> {
        self.inner.shutdown.subscribe()
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/bacnet", get(bacnet_info))
        .route("/api/specs", get(list_specs))
        .route("/api/specs/{name}", get(spec_detail))
        .route("/api/specs/{name}/graph", get(spec_graph))
        .route("/api/specs/{name}/verify", post(verify_spec))
        .merge(crate::run_routes::routes())
        .merge(crate::ledger_routes::routes())
        .merge(crate::author_routes::routes())
        .merge(crate::explain::routes())
        .merge(crate::run_explain::routes())
        .merge(crate::fix_explain::routes())
        .merge(crate::workbench::routes())
        .merge(crate::bind_routes::routes())
        .fallback(fallback)
        .with_state(state)
}

/// An error response with a stable code.
pub struct Failure {
    status: StatusCode,
    body: ApiError,
}

impl Failure {
    pub(crate) fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ApiError {
                code: code.to_owned(),
                message: message.into(),
            },
        }
    }

    pub(crate) fn unknown_spec(_name: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "unknown-spec",
            "no spec with that name",
        )
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

async fn not_found() -> Failure {
    Failure::new(StatusCode::NOT_FOUND, "not-found", "no such route")
}

/// An unknown `/api` route is a JSON 404; anything else is the web app, when it is served.
async fn fallback(State(state): State<AppState>, uri: axum::http::Uri) -> Response {
    match state.inner.web.get() {
        Some(dir) if !uri.path().starts_with("/api") => crate::web::serve(dir.clone(), uri).await,
        _ => not_found().await.into_response(),
    }
}

async fn bacnet_info(
    State(state): State<AppState>,
) -> Result<Json<crate::bacnet::BacnetInfo>, Failure> {
    state.bacnet().map(|b| Json(b.info.clone())).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "bacnet-off",
            "the BACnet mirror is not running (set MM_BACNET_ADDR)",
        )
    })
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(Health {
        status: "ok".into(),
        engine_rev: ENGINE_REV.into(),
        specs: u32::try_from(state.inner.specs.len()).unwrap_or(u32::MAX),
        live_model: state
            .authoring()
            .config
            .spark
            .is_some()
            .then(|| crate::authoring::MODEL.to_owned()),
        recorded_model: state.authoring().config.recordings.is_some(),
        bacnet_device: state.bacnet().map(|b| b.info.device_instance),
        boptest: state.runs().config.plant.is_live(),
    })
}

async fn list_specs(State(state): State<AppState>) -> Json<Vec<SpecSummary>> {
    Json(
        state
            .inner
            .specs
            .iter()
            .map(|s| s.detail.summary.clone())
            .collect(),
    )
}

async fn spec_detail(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<SpecDetail>, Failure> {
    let spec = state
        .spec(&name)
        .ok_or_else(|| Failure::unknown_spec(&name))?;
    Ok(Json(spec.detail))
}

async fn spec_graph(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<GraphView>, Failure> {
    let spec = state
        .spec(&name)
        .ok_or_else(|| Failure::unknown_spec(&name))?;
    spec.view.map(Json).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "no-reference-graph",
            "this sequence has no hand-built graph; see its verified drafts",
        )
    })
}

/// Emits the spec's reference graph and replays its vectors on a fresh engine per scenario.
/// The slot permit moves into the blocking task, so it lasts as long as the work even if the
/// client disconnects; when both slots are busy the request is refused rather than queued.
async fn verify_spec(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Receipt>, Failure> {
    let spec = state
        .inner
        .specs
        .get(&name)
        .ok_or_else(|| Failure::unknown_spec(&name))?
        .clone();
    let permit = Arc::clone(&state.inner.verify_slots)
        .try_acquire_owned()
        .map_err(|_| {
            Failure::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "busy",
                "verifications are already running; try again shortly",
            )
        })?;
    let worker = state.clone();
    let spec_name = name.clone();
    let joined = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let (_, report) = spec
            .replay(&worker.inner.palette)
            .map_err(|e| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, "replay-failed", e))?;
        Ok::<_, Failure>(receipt(spec_name, report))
    })
    .await;
    match joined {
        Ok(result) => result.map(Json),
        Err(e) => {
            tracing::error!(error = %e, spec = %name, "verification task failed");
            Err(Failure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "verify-crashed",
                "verification stopped unexpectedly",
            ))
        }
    }
}

fn receipt(spec: String, report: mm_bridge::ReplayReport) -> Receipt {
    Receipt {
        spec,
        passed: report.passed(),
        content_id: report.content_id,
        scenarios: report
            .scenarios
            .into_iter()
            .map(crate::dto::scenario_report)
            .collect(),
    }
}
