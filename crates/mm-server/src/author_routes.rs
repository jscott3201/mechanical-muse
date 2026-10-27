//! Authoring routes: start a session for a spec, follow it as server-sent events, and list the
//! verified drafts it leaves behind.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::author_dto::{AuthorInfo, AuthorRequest, DraftInfo};
use crate::authoring::AuthorSession;
use crate::dto::GraphView;
use crate::journal;
use crate::routes::{AppState, Failure};
use crate::runs::StartError;
use crate::specs::graph_view;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/specs/{name}/author", post(start))
        .route("/api/authoring", get(list))
        .route("/api/authoring/{id}", get(info))
        .route("/api/authoring/{id}/events", get(events))
        .route("/api/drafts", get(drafts))
        .route("/api/drafts/{content_id}/graph", get(draft_graph))
        .route("/api/drafts/{content_id}/cxf", get(draft_cxf))
}

fn find(state: &AppState, id: &str) -> Result<Arc<AuthorSession>, Failure> {
    state.authoring().get(id).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "unknown-session",
            "no authoring session with that id",
        )
    })
}

async fn start(
    State(state): State<AppState>,
    Path(name): Path<String>,
    request: Result<Json<AuthorRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AuthorInfo>), Failure> {
    let Json(request) =
        request.map_err(|e| Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.body_text()))?;
    let Some(spec) = state.spec(&name) else {
        return Err(Failure::new(
            StatusCode::NOT_FOUND,
            "unknown-spec",
            "no spec with that name",
        ));
    };
    let session = state
        .authoring()
        .create(&name, Some(&spec.task), &request)
        .map_err(|e| match e {
            StartError::Active(_) => {
                Failure::new(StatusCode::CONFLICT, "author-active", e.to_string())
            }
            StartError::Invalid(_) => {
                Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.to_string())
            }
        })?;
    let info = session.info();
    tokio::spawn(crate::authoring::run(session, state.clone()));
    Ok((StatusCode::CREATED, Json(info)))
}

async fn list(State(state): State<AppState>) -> Json<Vec<AuthorInfo>> {
    Json(state.authoring().list())
}

async fn info(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AuthorInfo>, Failure> {
    Ok(Json(find(&state, &id)?.info()))
}

async fn events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    let session = find(&state, &id)?;
    Ok(journal::stream(
        session.journal(),
        journal::last_event_id(&headers),
        state.shutdown_signal(),
    ))
}

async fn drafts(State(state): State<AppState>) -> Json<Vec<DraftInfo>> {
    Json(state.authoring().drafts.list())
}

async fn draft_graph(
    State(state): State<AppState>,
    Path(content_id): Path<String>,
) -> Result<Json<GraphView>, Failure> {
    let draft = state.authoring().drafts.get(&content_id).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "unknown-draft",
            "no verified draft with that content id",
        )
    })?;
    graph_view(&draft.graph, state.palette())
        .map(Json)
        .map_err(|e| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, "bad-draft", e))
}

/// The verified draft as CXF (JSON-LD), emitted again from its stored graph (the emitter is
/// deterministic), as a download for the engineer's own tools.
async fn draft_cxf(
    State(state): State<AppState>,
    Path(content_id): Path<String>,
) -> Result<axum::response::Response, Failure> {
    use axum::response::IntoResponse;
    let draft = state.authoring().drafts.get(&content_id).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "unknown-draft",
            "no verified draft with that content id",
        )
    })?;
    let cxf = mm_graph::emit_cxf(
        &draft.graph,
        state.palette(),
        &mm_graph::EmitOptions::default(),
    )
    .map_err(|e| {
        Failure::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "bad-draft",
            e.to_string(),
        )
    })?;
    let short: String = content_id
        .rsplit(':')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    let spec: String = draft
        .info
        .spec
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let disposition = format!("attachment; filename=\"{spec}-{short}.cxf.jsonld\"");
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/ld+json".to_owned(),
            ),
            (axum::http::header::CONTENT_DISPOSITION, disposition),
        ],
        cxf,
    )
        .into_response())
}
