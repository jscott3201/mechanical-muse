//! Ledger routes: one run's or one recorded run's Scope 1 and 2 ledger, and two of them
//! compared. A run is named `run:<id>` (a run on this server, replays included) or
//! `replay:<record>` (a recorded run file).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::ledger::{self, LedgerComparison, LedgerFailure, RecordLedgerError, RunLedger};
use crate::routes::{AppState, Failure};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/runs/{id}/ledger", get(run_ledger))
        .route("/api/replays/{name}/ledger", get(replay_ledger))
        .route("/api/ledger/compare", get(compare))
}

fn failure(e: &LedgerFailure) -> Failure {
    let status = match e {
        LedgerFailure::NotStarted | LedgerFailure::NoKpis => StatusCode::CONFLICT,
        LedgerFailure::Kpis(_) => StatusCode::UNPROCESSABLE_ENTITY,
        LedgerFailure::WindowMismatch(_) => StatusCode::BAD_REQUEST,
    };
    Failure::new(status, e.code(), e.to_string())
}

fn of_run(state: &AppState, id: &str) -> Result<RunLedger, Failure> {
    let handle = state
        .runs()
        .get(id)
        .ok_or_else(|| Failure::new(StatusCode::NOT_FOUND, "unknown-run", "no run with that id"))?;
    let events = handle.journal().since(None);
    ledger::from_events(&format!("run:{id}"), &events).map_err(|e| failure(&e))
}

async fn of_replay(state: &AppState, name: &str) -> Result<RunLedger, Failure> {
    let dir = state.runs().config.replays.clone().ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "no-replays",
            "this server has no recorded runs",
        )
    })?;
    let name = name.to_owned();
    tokio::task::spawn_blocking(move || ledger::of_record(&dir, &name))
        .await
        .map_err(|e| {
            Failure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ledger-crashed",
                e.to_string(),
            )
        })?
        .map_err(|e| match e {
            RecordLedgerError::Record(m) => {
                Failure::new(StatusCode::NOT_FOUND, "unknown-record", m)
            }
            RecordLedgerError::Ledger(l) => failure(&l),
        })
}

/// A `run:<id>` or `replay:<name>` reference.
async fn of_reference(state: &AppState, reference: &str) -> Result<RunLedger, Failure> {
    match reference.split_once(':') {
        Some(("run", id)) if !id.is_empty() => of_run(state, id),
        Some(("replay", name)) if !name.is_empty() => of_replay(state, name).await,
        _ => Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "bad-reference",
            format!("{reference:?} is not run:<id> or replay:<record name>"),
        )),
    }
}

async fn run_ledger(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunLedger>, Failure> {
    of_run(&state, &id).map(Json)
}

async fn replay_ledger(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<RunLedger>, Failure> {
    of_replay(&state, &name).await.map(Json)
}

#[derive(Debug, Deserialize)]
struct CompareQuery {
    a: String,
    b: String,
}

/// `b − a` for two runs of the same window (`?a=replay:day143-morning-baseline&b=run:...`).
async fn compare(
    State(state): State<AppState>,
    query: Result<Query<CompareQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<LedgerComparison>, Failure> {
    let Query(q) = query.map_err(|e| {
        Failure::new(
            StatusCode::BAD_REQUEST,
            "bad-request",
            format!("{e}; expected ?a=<run:id|replay:name>&b=<run:id|replay:name>"),
        )
    })?;
    let a = of_reference(&state, &q.a).await?;
    let b = of_reference(&state, &q.b).await?;
    ledger::compare(a, b).map(Json).map_err(|e| failure(&e))
}
