//! HTTP routes. Handlers stay thin: specs are loaded at startup, verification runs on the
//! blocking pool behind a small semaphore so a burst of requests cannot pin every core.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mm_bridge::ScenarioOutcome;
use mm_graph::Palette;
use tokio::sync::Semaphore;

use crate::dto::{
    ApiError, GraphView, Health, Receipt, ScenarioReport, ScenarioResult, SpecDetail, SpecSummary,
};
use crate::runs::{PlantSource, Runs, RunsConfig};
use crate::specs::SpecStore;

/// open-control-engine commit pinned in the workspace (checked against `Cargo.lock` in tests).
pub const ENGINE_REV: &str = "4063202b9a9e50c723bb858d786108571d3d401d";

/// Verifications allowed to run at once.
const VERIFY_SLOTS: usize = 2;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    specs: SpecStore,
    palette: Palette,
    verify_slots: Arc<Semaphore>,
    runs: Runs,
}

impl AppState {
    /// Runs go to BOPTEST on `http://127.0.0.1:80` and keep no records on disk.
    pub fn new(specs: SpecStore, palette: Palette) -> Self {
        Self::with_runs(
            specs,
            palette,
            RunsConfig {
                plant: PlantSource::Boptest {
                    url: "http://127.0.0.1:80".into(),
                },
                records: None,
            },
        )
    }

    pub fn with_runs(specs: SpecStore, palette: Palette, runs: RunsConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                specs,
                palette,
                verify_slots: Arc::new(Semaphore::new(VERIFY_SLOTS)),
                runs: Runs::new(runs),
            }),
        }
    }

    pub fn specs(&self) -> &SpecStore {
        &self.inner.specs
    }

    pub fn palette(&self) -> &Palette {
        &self.inner.palette
    }

    pub fn runs(&self) -> &Runs {
        &self.inner.runs
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/specs", get(list_specs))
        .route("/api/specs/{name}", get(spec_detail))
        .route("/api/specs/{name}/graph", get(spec_graph))
        .route("/api/specs/{name}/verify", post(verify_spec))
        .merge(crate::run_routes::routes())
        .fallback(not_found)
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

    fn unknown_spec(_name: &str) -> Self {
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

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(Health {
        status: "ok".into(),
        engine_rev: ENGINE_REV.into(),
        specs: u32::try_from(state.inner.specs.len()).unwrap_or(u32::MAX),
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
        .inner
        .specs
        .get(&name)
        .ok_or_else(|| Failure::unknown_spec(&name))?;
    Ok(Json(spec.detail.clone()))
}

async fn spec_graph(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<GraphView>, Failure> {
    let spec = state
        .inner
        .specs
        .get(&name)
        .ok_or_else(|| Failure::unknown_spec(&name))?;
    Ok(Json(spec.view.clone()))
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
            .map(|s| ScenarioReport {
                name: s.name,
                ticks: s.ticks,
                result: match s.outcome {
                    ScenarioOutcome::Pass => ScenarioResult::Pass,
                    ScenarioOutcome::Fail {
                        t,
                        output,
                        expected,
                        actual,
                        from_s,
                        to_s,
                    } => ScenarioResult::Fail {
                        t,
                        output,
                        expected: serde_json::to_value(&expected)
                            .unwrap_or(serde_json::Value::Null),
                        actual: serde_json::to_value(&actual).unwrap_or(serde_json::Value::Null),
                        from_s,
                        to_s,
                    },
                    ScenarioOutcome::Error { code, detail, t } => {
                        ScenarioResult::Error { code, detail, t }
                    }
                },
            })
            .collect(),
    }
}
