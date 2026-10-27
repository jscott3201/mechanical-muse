//! Binding an engineer's sequence to BOPTEST: Muse Spark proposes, the checks in [`crate::bind`]
//! list what cannot run, the engineer confirms. A recorded proposal (`fixtures/bindings/`) is
//! served, labelled, when its task matches and there is no key.

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::author_dto::ModelSource;
use crate::bind::{Binding, check, listing, with_units};
use crate::routes::{AppState, Failure};

/// A binding and what the checks found; `source` says who wrote it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BindingView {
    pub spec: String,
    pub binding: Binding,
    pub problems: Vec<String>,
    /// Muse Spark's proposal (`live` or `recorded`); absent once the engineer confirmed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub proposed_by: Option<ModelSource>,
    pub confirmed: bool,
    /// Digest of the task the binding was made for.
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct ConfirmRequest {
    pub binding: Binding,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/workbench/{name}/binding",
            get(confirmed).post(confirm),
        )
        .route("/api/workbench/{name}/binding/propose", post(propose))
        .route("/api/workbench/bindings", get(all))
}

fn task_digest(task: &mm_agent::TaskSpec) -> String {
    let text = serde_json::to_string(task).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("task:{hash:016x}")
}

/// Only engineers' sequences are bound here; ours have hand-written wiring.
fn workbench_task(state: &AppState, name: &str) -> Result<mm_agent::TaskSpec, Failure> {
    let spec = state.spec(name).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "unknown-spec",
            "no spec with that name",
        )
    })?;
    if spec.detail.reference {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "built-in",
            "built-in sequences have their own wiring; only your own sequences are bound here",
        ));
    }
    Ok(spec.task)
}

fn saved(dir: &std::path::Path, name: &str) -> Option<BindingView> {
    let text = std::fs::read_to_string(dir.join(format!("{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

async fn propose(
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
) -> Result<Json<BindingView>, Failure> {
    let task = workbench_task(&state, &name)?;
    let digest = task_digest(&task);
    let config = &state.authoring().config;
    let client = match config.spark.clone() {
        Some(client) => client,
        None => {
            let recorded = config
                .bindings
                .as_deref()
                .and_then(|d| saved(d, &name))
                .filter(|v| v.task == digest)
                .ok_or_else(|| {
                    Failure::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "no-model",
                        "no model key on this server, and no recorded binding for this sequence",
                    )
                })?;
            let binding = with_units(recorded.binding);
            return Ok(Json(BindingView {
                spec: name,
                problems: check(&binding, &task),
                binding,
                proposed_by: Some(ModelSource::Recorded),
                confirmed: false,
                task: digest,
            }));
        }
    };
    let points = listing();
    let model_error = |e: mm_agent::ModelError| {
        Failure::new(StatusCode::BAD_GATEWAY, "model-error", e.to_string())
    };
    let shape = |value: serde_json::Value| {
        serde_json::from_value::<Binding>(value).map_err(|e| {
            Failure::new(
                StatusCode::BAD_GATEWAY,
                "model-error",
                format!("the binding has the wrong shape: {e}"),
            )
        })
    };
    let (value, _) = mm_agent::propose_binding(
        client.as_ref(),
        crate::authoring::MODEL,
        &task,
        &points,
        None,
    )
    .await
    .map_err(model_error)?;
    let mut binding = shape(value.clone())?;
    let mut problems = check(&binding, &task);
    // One retry with the checks' findings, as for a reading.
    if !problems.is_empty() {
        let retry = Some((&value, problems.as_slice()));
        if let Ok((again, _)) = mm_agent::propose_binding(
            client.as_ref(),
            crate::authoring::MODEL,
            &task,
            &points,
            retry,
        )
        .await
            && let Ok(fixed) = shape(again)
        {
            problems = check(&fixed, &task);
            binding = fixed;
        }
    }
    let view = BindingView {
        spec: name.clone(),
        problems,
        binding: with_units(binding),
        proposed_by: Some(ModelSource::Live),
        confirmed: false,
        task: digest,
    };
    if let Some(dir) = config.records.as_ref().map(|d| d.join("bindings")) {
        let written = std::fs::create_dir_all(&dir).and_then(|()| {
            let text = serde_json::to_string_pretty(&view).map_err(std::io::Error::other)?;
            std::fs::write(dir.join(format!("{name}.json")), text)
        });
        if let Err(e) = written {
            tracing::warn!(error = %e, "could not save the proposed binding");
        }
    }
    Ok(Json(view))
}

async fn confirm(
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
    request: Result<Json<ConfirmRequest>, JsonRejection>,
) -> Result<Json<BindingView>, Failure> {
    let Json(request) =
        request.map_err(|e| Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.body_text()))?;
    let task = workbench_task(&state, &name)?;
    let problems = check(&request.binding, &task);
    if !problems.is_empty() {
        return Err(Failure::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "not-confirmed",
            problems.join("\n"),
        ));
    }
    let view = BindingView {
        spec: name.clone(),
        binding: with_units(request.binding),
        problems: Vec::new(),
        proposed_by: None,
        confirmed: true,
        task: task_digest(&task),
    };
    // The name is an accepted spec's (an identifier), so it is a plain file name.
    if let Some(dir) = confirmed_dir(&state) {
        let written = std::fs::create_dir_all(&dir).and_then(|()| {
            let text = serde_json::to_string_pretty(&view).map_err(std::io::Error::other)?;
            std::fs::write(dir.join(format!("{name}.json")), text)
        });
        if let Err(e) = written {
            tracing::warn!(error = %e, "could not save the binding");
            return Err(Failure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "not-saved",
                "the binding could not be saved",
            ));
        }
    }
    state.set_binding(&name, view.binding.clone());
    Ok(Json(view))
}

async fn confirmed(
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
) -> Result<Json<BindingView>, Failure> {
    let task = workbench_task(&state, &name)?;
    let binding = state.binding(&name).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "no-binding",
            "this sequence is not bound to BOPTEST yet",
        )
    })?;
    Ok(Json(BindingView {
        spec: name,
        problems: check(&binding, &task),
        binding,
        proposed_by: None,
        confirmed: true,
        task: task_digest(&task),
    }))
}

/// Every engineer's sequence that is bound to BOPTEST, with its binding.
async fn all(State(state): State<AppState>) -> Json<std::collections::BTreeMap<String, Binding>> {
    Json(state.bindings())
}

fn confirmed_dir(state: &AppState) -> Option<std::path::PathBuf> {
    state
        .authoring()
        .config
        .records
        .as_ref()
        .map(|d| d.join("workbench").join("bindings"))
}

/// Restores confirmed bindings of restored sequences (after `workbench::restore`).
pub fn restore(state: &AppState) {
    let Some(dir) = confirmed_dir(state) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(view) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<BindingView>(&t).ok())
        else {
            continue;
        };
        match state.spec(&view.spec) {
            Some(spec) if !spec.detail.reference && check(&view.binding, &spec.task).is_empty() => {
                state.set_binding(&view.spec, view.binding);
            }
            _ => tracing::warn!(path = %path.display(), "skipping a saved binding"),
        }
    }
}
