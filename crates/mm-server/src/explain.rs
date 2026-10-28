//! Explanations of a spec's graph (our reference, or a verified draft) by the live model, for
//! people. Cached by the graph's content id: in memory, under `<records>/explanations`, and
//! read-only from a recorded folder (committed fixtures), which serves them labelled "recorded"
//! when the server has no model key. Never used for control.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use mm_graph::{BlockGraph, EmitOptions, emit_cxf};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::routes::{AppState, Failure};

/// Which graph to explain: our reference (`graph` absent) or a stored draft's content id.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct ExplainRequest {
    #[serde(default)]
    #[ts(optional)]
    pub graph: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExplainedPartView {
    pub title: String,
    pub text: String,
    /// Block ids of the graph this part covers.
    pub blocks: Vec<String>,
}

/// A model-written explanation of one graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExplanationView {
    pub spec: String,
    /// Content id of the graph explained.
    pub content_id: String,
    pub model: String,
    /// `live` (written now or earlier on this server) or `recorded` (a committed fixture).
    pub source: String,
    pub summary: String,
    pub parts: Vec<ExplainedPartView>,
    /// Block ids the model named that the graph does not have (dropped from the parts).
    pub unknown_blocks: Vec<String>,
}

/// A stored explanation: found by its key, and labelled when it comes from the recorded folder.
pub(crate) trait Stored: Serialize + serde::de::DeserializeOwned + Clone {
    fn key(&self) -> &str;
    fn mark_recorded(&mut self);
}

impl Stored for ExplanationView {
    fn key(&self) -> &str {
        &self.content_id
    }
    fn mark_recorded(&mut self) {
        self.source = "recorded".into();
    }
}

#[derive(Debug, Default)]
pub struct Explanations {
    pub(crate) graphs: Mutex<BTreeMap<String, ExplanationView>>,
    pub(crate) runs: Mutex<BTreeMap<String, crate::run_explain::RunExplanationView>>,
    pub(crate) fixes: Mutex<BTreeMap<String, crate::fix_explain::FixExplanationView>>,
    /// Where live explanations are written (`<records>/explanations`).
    pub store: Option<PathBuf>,
    /// Committed explanations, served as recorded.
    pub recorded: Option<PathBuf>,
}

/// A content id as a file stem (`cxf:fnv1a128:abc` -> `cxf_fnv1a128_abc`).
fn stem(content_id: &str) -> Option<String> {
    let s: String = content_id
        .chars()
        .map(|c| if c == ':' { '_' } else { c })
        .collect();
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
        .then_some(s)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read<T: Stored>(dir: &Path, key: &str) -> Option<T> {
    let path = dir.join(format!("{}.json", stem(key)?));
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<T>(&text)
        .ok()
        .filter(|e| e.key() == key)
}

impl Explanations {
    pub fn new(store: Option<PathBuf>, recorded: Option<PathBuf>) -> Self {
        Self {
            store,
            recorded,
            ..Self::default()
        }
    }

    /// A known explanation under `key`: this server's own first, then a recorded one.
    pub(crate) fn known<T: Stored>(
        &self,
        cache: &Mutex<BTreeMap<String, T>>,
        key: &str,
    ) -> Option<T> {
        if let Some(e) = lock(cache).get(key) {
            return Some(e.clone());
        }
        let own = self.store.as_deref().and_then(|d| read::<T>(d, key));
        let found = own.or_else(|| {
            self.recorded
                .as_deref()
                .and_then(|d| read::<T>(d, key))
                .map(|mut e| {
                    e.mark_recorded();
                    e
                })
        })?;
        lock(cache).insert(key.to_owned(), found.clone());
        Some(found)
    }

    pub(crate) fn remember<T: Stored>(&self, cache: &Mutex<BTreeMap<String, T>>, view: &T) {
        lock(cache).insert(view.key().to_owned(), view.clone());
        let Some(dir) = &self.store else { return };
        let Some(name) = stem(view.key()) else {
            return;
        };
        let written = std::fs::create_dir_all(dir).and_then(|()| {
            let text = serde_json::to_string_pretty(view).map_err(std::io::Error::other)?;
            std::fs::write(dir.join(format!("{name}.json")), text)
        });
        if let Err(e) = written {
            tracing::warn!(error = %e, "could not save the explanation");
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/specs/{name}/explain", post(explain))
}

async fn explain(
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
    request: Result<Json<ExplainRequest>, JsonRejection>,
) -> Result<Json<ExplanationView>, Failure> {
    let Json(request) =
        request.map_err(|e| Failure::new(StatusCode::BAD_REQUEST, "bad-request", e.body_text()))?;
    let spec = state.spec(&name).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "unknown-spec",
            "no spec with that name",
        )
    })?;
    let graph: BlockGraph = match &request.graph {
        None => spec.graph.clone().ok_or_else(|| {
            Failure::new(
                StatusCode::NOT_FOUND,
                "no-reference-graph",
                "this sequence has no hand-built graph; explain one of its verified drafts",
            )
        })?,
        Some(id) => {
            let draft = state
                .authoring()
                .drafts
                .get(id)
                .filter(|d| d.info.spec == name)
                .ok_or_else(|| {
                    Failure::new(
                        StatusCode::NOT_FOUND,
                        "unknown-draft",
                        "no verified draft of this spec with that content id",
                    )
                })?;
            draft.graph
        }
    };
    let content_id = content_id(&state, &graph).await?;
    let ex = state.explanations();
    if let Some(found) = ex.known(&ex.graphs, &content_id) {
        return Ok(Json(found));
    }
    let client = state.authoring().config.spark.clone().ok_or_else(|| {
        Failure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "no-model",
            "no model key on this server and no recorded explanation of this graph",
        )
    })?;
    let explained = mm_agent::explain(
        client.as_ref(),
        crate::authoring::MODEL,
        &spec.detail.summary.title,
        &spec.detail.about,
        &spec.task,
        &graph,
    )
    .await
    .map_err(|e| Failure::new(StatusCode::BAD_GATEWAY, "model-error", e.to_string()))?;
    let view = ExplanationView {
        spec: name,
        content_id,
        model: crate::authoring::MODEL.to_owned(),
        source: "live".into(),
        summary: explained.explanation.summary,
        parts: explained
            .explanation
            .parts
            .into_iter()
            .map(|p| ExplainedPartView {
                title: p.title,
                text: p.text,
                blocks: p.blocks,
            })
            .collect(),
        unknown_blocks: explained.unknown_blocks,
    };
    state
        .explanations()
        .remember(&state.explanations().graphs, &view);
    Ok(Json(view))
}

/// The content id the engine gives the graph's CXF (the same id receipts carry).
async fn content_id(state: &AppState, graph: &BlockGraph) -> Result<String, Failure> {
    let graph = graph.clone();
    let worker = state.clone();
    tokio::task::spawn_blocking(move || {
        let cxf = emit_cxf(&graph, worker.palette(), &EmitOptions::default())
            .map_err(|e| format!("emit failed: {e}"))?;
        mm_bridge::load_cxf(&cxf)
            .map(|l| l.content_id)
            .map_err(|f| format!("the engine refused the graph: {}", f.code))
    })
    .await
    .map_err(|e| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()))?
    .map_err(|e| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, "bad-graph", e))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::stem;

    #[test]
    fn content_ids_become_safe_file_stems() {
        assert_eq!(
            stem("cxf:fnv1a128:61dc").as_deref(),
            Some("cxf_fnv1a128_61dc")
        );
        assert_eq!(stem("../x"), None);
    }
}
