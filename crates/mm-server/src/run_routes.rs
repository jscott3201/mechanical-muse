//! Run routes and the server-sent event stream. A subscriber first gets every logged event
//! after its `Last-Event-ID` (or all of them), then live events; one that falls behind the
//! live channel re-reads the log, so no event is skipped or repeated. The stream ends after the
//! run's `ended` event.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};

use mm_runtime::preset::{PRESSURE_SPEC, REQUESTS_SPEC, RESET_SPEC, ZONE_PRESSURE_SPEC};

use crate::journal;
use crate::routes::{AppState, Failure};
use crate::run_dto::{MAX_REPLAY_SPEED, ReplayInfo, ReplayRequest, RunInfo, RunRequest};
use crate::runs::{RunHandle, StartError};

/// Why a live run is refused on a server with no plant; the web app shows it as is.
pub const BOPTEST_OFF: &str =
    "no BOPTEST configured on this server; set MM_BOPTEST_URL (replays still work)";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/runs", get(list_runs).post(start_run))
        .route("/api/runs/replay", post(start_replay))
        .route("/api/replays", get(list_replays))
        .route("/api/runs/{id}", get(run_info))
        .route("/api/runs/{id}/events", get(run_events))
        .route("/api/runs/{id}/stop", post(stop_run))
}

fn find(state: &AppState, id: &str) -> Result<Arc<RunHandle>, Failure> {
    state
        .runs()
        .get(id)
        .ok_or_else(|| Failure::new(StatusCode::NOT_FOUND, "unknown-run", "no run with that id"))
}

async fn list_runs(State(state): State<AppState>) -> Json<Vec<RunInfo>> {
    Json(state.runs().list())
}

async fn start_run(
    State(state): State<AppState>,
    request: Result<Json<RunRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<RunInfo>), Failure> {
    if !state.runs().config.plant.is_live() {
        return Err(Failure::new(
            StatusCode::CONFLICT,
            "boptest-off",
            BOPTEST_OFF,
        ));
    }
    let Json(request) =
        request.map_err(|e| Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.body_text()))?;
    check_drafts(&state, &request)?;
    let mut config = crate::runs::resolve(&request)
        .map_err(|e| Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.to_string()))?;
    // An engineer's sequence runs with the binding confirmed for it, recorded in the run.
    for spec in config.drafts.keys() {
        let own = state.spec(spec).is_some_and(|s| !s.detail.reference);
        if let Some(binding) = state.binding(spec).filter(|_| own) {
            config.bindings.insert(spec.clone(), binding);
        }
    }
    // Our own sequences write these; a bound sequence may not write them too.
    if !config.baseline {
        let mut ours: Vec<String> = mm_runtime::preset::reset_outputs()
            .into_iter()
            .map(|(_, b)| b.point)
            .collect();
        if config.pressure {
            ours.extend(
                mm_runtime::preset::pressure_outputs()
                    .into_iter()
                    .map(|(_, b)| b.point),
            );
        }
        for (spec, binding) in &config.bindings {
            for f in &binding.floors {
                for o in binding.outputs.iter().filter(|o| !o.point.is_empty()) {
                    let point = o.point.replace("{floor}", &f.to_string());
                    if ours.contains(&point) {
                        return Err(Failure::new(
                            StatusCode::BAD_REQUEST,
                            "bad-request",
                            format!(
                                "{spec} would write {point}, which this run's own sequences write"
                            ),
                        ));
                    }
                }
            }
        }
    }
    let handle = state.runs().register(config).map_err(|e| match e {
        StartError::Active(_) => Failure::new(StatusCode::CONFLICT, "run-active", e.to_string()),
        StartError::Invalid(_) | StartError::NoRecording(_) => {
            Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.to_string())
        }
    })?;
    let info = handle.info();
    tokio::spawn(crate::session::run(handle, state.clone()));
    Ok((StatusCode::CREATED, Json(info)))
}

/// Each requested draft must be a stored, verified draft of a spec this run uses. Its vectors
/// are replayed again when the run loads it.
fn check_drafts(state: &AppState, request: &RunRequest) -> Result<(), Failure> {
    let bad = |m: String| Failure::new(StatusCode::BAD_REQUEST, "bad-request", m);
    let home = request.building == Some(crate::run_dto::Building::Home);
    let requests = request.requests.unwrap_or(true);
    let pressure = request.pressure.unwrap_or(false);
    for (spec, id) in request.drafts.iter().flatten() {
        let own = state.spec(spec).is_some_and(|s| !s.detail.reference);
        if own && state.binding(spec).is_none() {
            return Err(bad(format!(
                "{spec} is not bound to BOPTEST yet; confirm a binding first"
            )));
        }
        // The home runs its one sequence; the office's specs are the office's.
        let used = if home {
            spec == mm_runtime::home::SPEC
        } else {
            own || spec == RESET_SPEC
                || (spec == REQUESTS_SPEC && requests)
                || ((spec == PRESSURE_SPEC || spec == ZONE_PRESSURE_SPEC) && pressure)
        };
        if !used {
            return Err(bad(format!("this run does not use {spec}")));
        }
        match state.authoring().drafts.get(id) {
            Some(d) if d.info.spec == *spec => {}
            Some(_) => return Err(bad(format!("draft {id} is not a draft of {spec}"))),
            None => return Err(bad(format!("no verified draft {id}"))),
        }
    }
    Ok(())
}

fn no_replays() -> Failure {
    Failure::new(
        StatusCode::NOT_FOUND,
        "no-replays",
        "this server has no recorded runs to replay",
    )
}

async fn list_replays(State(state): State<AppState>) -> Result<Json<Vec<ReplayInfo>>, Failure> {
    let dir = state.runs().config.replays.clone().ok_or_else(no_replays)?;
    let list = tokio::task::spawn_blocking(move || crate::replay::list(&dir))
        .await
        .map_err(|e| {
            Failure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay-crashed",
                e.to_string(),
            )
        })?;
    Ok(Json(list))
}

/// Plays a recorded run back through a normal run's event stream, with a config that names the
/// record. One replay at a time; it may run beside a live run, since it drives no plant.
async fn start_replay(
    State(state): State<AppState>,
    request: Result<Json<ReplayRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<RunInfo>), Failure> {
    let Json(request) =
        request.map_err(|e| Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.body_text()))?;
    let speed = request.speed.unwrap_or(1);
    if !(1..=MAX_REPLAY_SPEED).contains(&speed) {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "bad-request",
            format!("speed must be 1 to {MAX_REPLAY_SPEED}"),
        ));
    }
    let dir = state.runs().config.replays.clone().ok_or_else(no_replays)?;
    let name = request.record.clone();
    let events = tokio::task::spawn_blocking(move || crate::replay::read(&dir, &name))
        .await
        .map_err(|e| {
            Failure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay-crashed",
                e.to_string(),
            )
        })?
        .map_err(|e| Failure::new(StatusCode::NOT_FOUND, "unknown-record", e))?;
    let config = crate::replay::config(&request.record, &events)
        .map_err(|e| Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.to_string()))?;
    let handle = state.runs().register(config).map_err(|e| match e {
        StartError::Active(_) => Failure::new(StatusCode::CONFLICT, "run-active", e.to_string()),
        StartError::Invalid(_) | StartError::NoRecording(_) => {
            Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.to_string())
        }
    })?;
    let info = handle.info();
    // Only the wall time between ticks changes; the events sent are the same at any speed.
    let pace = state.runs().config.replay_pace / speed;
    tokio::spawn(crate::replay::run(handle, events, pace));
    Ok((StatusCode::CREATED, Json(info)))
}

async fn run_info(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunInfo>, Failure> {
    Ok(Json(find(&state, &id)?.info()))
}

async fn stop_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<RunInfo>), Failure> {
    let handle = find(&state, &id)?;
    handle.request_stop();
    Ok((StatusCode::ACCEPTED, Json(handle.info())))
}

async fn run_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    let handle = find(&state, &id)?;
    Ok(journal::stream(
        handle.journal(),
        journal::last_event_id(&headers),
        state.shutdown_signal(),
    ))
}
