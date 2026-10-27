//! Run routes and the server-sent event stream. A subscriber first gets every logged event
//! after its `Last-Event-ID` (or all of them), then live events; one that falls behind the
//! live channel re-reads the log, so no event is skipped or repeated. The stream ends after the
//! run's `ended` event.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::watch;

use crate::routes::{AppState, Failure};
use crate::run_dto::{Envelope, RunEvent, RunInfo, RunRequest, RunState};
use crate::runs::{RunHandle, StartError};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/runs", get(list_runs).post(start_run))
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
    let Json(request) =
        request.map_err(|e| Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.body_text()))?;
    let handle = state.runs().create(&request).map_err(|e| match e {
        StartError::Active(_) => Failure::new(StatusCode::CONFLICT, "run-active", e.to_string()),
        StartError::Invalid(_) => {
            Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.to_string())
        }
    })?;
    let info = handle.info();
    tokio::spawn(crate::session::run(handle, state.clone()));
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

struct Feed {
    handle: Arc<RunHandle>,
    pending: VecDeque<Envelope>,
    rx: broadcast::Receiver<Envelope>,
    shutdown: watch::Receiver<bool>,
    last: Option<u64>,
    done: bool,
}

impl Feed {
    async fn next(&mut self) -> Option<Envelope> {
        loop {
            if self.done {
                return None;
            }
            if let Some(env) = self.pending.pop_front() {
                if self.last.is_some_and(|l| env.seq <= l) {
                    continue;
                }
                self.last = Some(env.seq);
                self.done = matches!(env.event, RunEvent::Ended { .. });
                return Some(env);
            }
            let received = tokio::select! {
                r = self.rx.recv() => r,
                _ = self.shutdown.wait_for(|stop| *stop) => return None,
            };
            match received {
                Ok(env) => self.pending.push_back(env),
                // Fell behind the live channel: refill from the log, which has everything.
                Err(RecvError::Lagged(_)) => self.pending.extend(self.handle.since(self.last)),
                Err(RecvError::Closed) => return None,
            }
        }
    }
}

fn sse_event(env: &Envelope) -> Event {
    let kind = match &env.event {
        RunEvent::Started { .. } => "started",
        RunEvent::Phase { .. } => "phase",
        RunEvent::Tick { .. } => "tick",
        RunEvent::Kpi { .. } => "kpi",
        RunEvent::Ended { .. } => "ended",
    };
    let data = serde_json::to_string(env).unwrap_or_else(|_| "{}".into());
    Event::default()
        .id(env.seq.to_string())
        .event(kind)
        .data(data)
}

async fn run_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    let handle = find(&state, &id)?;
    let after = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    let (backlog, rx) = handle.subscribe(after);
    // A finished run with nothing left to send: 204 tells an EventSource not to reconnect
    // (it would otherwise reconnect with the last id after every close, forever).
    let finished = matches!(
        handle.state(),
        RunState::Completed | RunState::Stopped | RunState::Failed
    );
    if finished && backlog.is_empty() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    let feed = Feed {
        handle,
        pending: backlog.into(),
        rx,
        shutdown: state.shutdown_signal(),
        last: after,
        done: false,
    };
    let stream = futures_util::stream::unfold(feed, |mut feed| async move {
        let env = feed.next().await?;
        Some((Ok::<_, Infallible>(sse_event(&env)), feed))
    });
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}
