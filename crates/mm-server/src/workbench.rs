//! Bring your own sequence: Muse Spark reads an engineer's sequence of operations (text, a PDF or
//! a photo) into a task and proposed scenarios; the engineer reviews, edits and accepts them; the
//! accepted scenarios become that sequence's verification gate, and authoring proceeds as for our
//! own specs. The proposal is model output and is labelled so; only the engineer's acceptance
//! makes it a gate.
//!
//! Readings are keyed by a digest of what was sent, so the sample document has a recorded reading
//! (`fixtures/readings/`) that serves without a key, labelled recorded. Accepted sequences are
//! saved under `<records>/workbench/` and restored at start-up.

use std::path::{Path, PathBuf};

use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use mm_agent::{Attachment, SequenceSource};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::author_dto::ModelSource;
use crate::dto::{SpecDetail, SpecSummary};
use crate::routes::{AppState, Failure};
use crate::specs::Spec;
pub use crate::workbench_draft::{
    ExpectDraft, InputDraft, ParamDraft, PointDraft, ScenarioDraft, SequenceDraft, StepDraft,
    from_draft, to_draft,
};

/// Largest request body (a PDF or photo, base64): 20 MB.
const MAX_BODY: usize = 20 * 1024 * 1024;
const MAX_ATTACHMENTS: usize = 4;
const MAX_TEXT: usize = 40_000;
const MAX_TITLE: usize = 120;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkbenchAttachment {
    pub name: String,
    /// `application/pdf`, `image/png`, `image/jpeg`, `image/gif` or `image/webp`.
    pub mime: String,
    /// The file, base64 (no data-URL prefix).
    pub data_base64: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct ReadRequest {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<WorkbenchAttachment>,
}

/// What a read returns: the draft, labelled, with the problems the checks found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReadView {
    /// Digest of what was sent (`read:<hex>`).
    pub key: String,
    pub model: String,
    pub source: ModelSource,
    pub draft: SequenceDraft,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct AcceptRequest {
    /// The edited draft; only the scenarios the engineer kept.
    pub draft: SequenceDraft,
    /// Digest of the reading it came from, kept with the sequence for provenance.
    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AcceptView {
    pub spec: SpecDetail,
}

/// What is saved for an accepted sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Saved {
    key: Option<String>,
    draft: SequenceDraft,
}

/// FNV-1a 64 over the text and each attachment: the key of a reading.
fn digest(request: &ReadRequest) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    };
    feed(request.text.trim().as_bytes());
    for a in &request.attachments {
        feed(a.mime.as_bytes());
        feed(a.data_base64.as_bytes());
    }
    format!("read:{hash:016x}")
}

fn file_name(key: &str) -> String {
    format!("{}.json", key.replace(':', "_"))
}

fn load_reading(dir: &Path, key: &str) -> Option<ReadView> {
    let text = std::fs::read_to_string(dir.join(file_name(key))).ok()?;
    serde_json::from_str::<ReadView>(&text)
        .ok()
        .filter(|r| r.key == key)
}

fn save_json<T: Serialize>(dir: &Path, name: &str, value: &T) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(name), text)
}

fn attachment(a: &WorkbenchAttachment) -> Result<Attachment, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(a.data_base64.as_bytes())
        .map_err(|_| format!("`{}` is not valid base64", a.name))?;
    if bytes.is_empty() {
        return Err(format!("`{}` is empty", a.name));
    }
    match a.mime.as_str() {
        "application/pdf" => Ok(Attachment::Pdf {
            filename: a.name.clone(),
            base64: a.data_base64.clone(),
        }),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" => Ok(Attachment::Image {
            mime: a.mime.clone(),
            base64: a.data_base64.clone(),
        }),
        other => Err(format!("`{}`: {other} is not a PDF or an image", a.name)),
    }
}

fn bad(message: impl Into<String>) -> Failure {
    Failure::new(StatusCode::BAD_REQUEST, "bad-request", message)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/workbench/read", post(read))
        .route("/api/workbench/accept", post(accept))
        .route("/api/workbench", get(list))
        .layer(DefaultBodyLimit::max(MAX_BODY))
}

async fn read(
    State(state): State<AppState>,
    request: Result<Json<ReadRequest>, JsonRejection>,
) -> Result<Json<ReadView>, Failure> {
    let Json(request) = request.map_err(|e| bad(e.body_text()))?;
    if request.text.trim().is_empty() && request.attachments.is_empty() {
        return Err(bad("send the sequence as text or attach a PDF or image"));
    }
    // Characters, as the message says and as the accepted task is counted (not UTF-8 bytes).
    if request.text.chars().count() > MAX_TEXT || request.attachments.len() > MAX_ATTACHMENTS {
        return Err(bad(format!(
            "at most {MAX_TEXT} characters of text and {MAX_ATTACHMENTS} attachments"
        )));
    }
    let attachments = request
        .attachments
        .iter()
        .map(attachment)
        .collect::<Result<Vec<_>, _>>()
        .map_err(bad)?;
    let key = digest(&request);
    let config = &state.authoring().config;
    let own = config.records.as_ref().map(|d| d.join("readings"));
    if let Some(found) = own.as_deref().and_then(|d| load_reading(d, &key)) {
        return Ok(Json(found));
    }
    if let Some(mut found) = config
        .readings
        .as_deref()
        .and_then(|d| load_reading(d, &key))
    {
        found.source = ModelSource::Recorded;
        return Ok(Json(found));
    }
    let client = config.spark.clone().ok_or_else(|| {
        Failure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "no-model",
            "no model key on this server, and no recorded reading of this document",
        )
    })?;
    let _one = state.workbench_slot().try_lock().map_err(|_| {
        Failure::new(
            StatusCode::CONFLICT,
            "read-active",
            "Muse Spark is already reading a sequence; try again when it finishes",
        )
    })?;
    let source = SequenceSource {
        text: request.text.clone(),
        attachments,
    };
    let out = mm_agent::read_sequence(client.as_ref(), crate::authoring::MODEL, &source)
        .await
        .map_err(|e| Failure::new(StatusCode::BAD_GATEWAY, "model-error", e.to_string()))?;
    tracing::info!(
        calls = out.calls,
        input = out.usage.input_tokens,
        output = out.usage.output_tokens,
        "read a sequence"
    );
    let view = ReadView {
        key: key.clone(),
        model: crate::authoring::MODEL.to_owned(),
        source: ModelSource::Live,
        draft: to_draft(&out.read),
        problems: out.read.problems,
    };
    if let Some(dir) = &own
        && let Err(e) = save_json(dir, &file_name(&key), &view)
    {
        tracing::warn!(error = %e, "could not save the reading");
    }
    Ok(Json(view))
}

async fn accept(
    State(state): State<AppState>,
    request: Result<Json<AcceptRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AcceptView>), Failure> {
    let Json(request) = request.map_err(|e| bad(e.body_text()))?;
    if request.draft.title.chars().count() > MAX_TITLE
        || request.draft.description.chars().count() > MAX_TEXT
    {
        return Err(bad(format!(
            "the title is limited to {MAX_TITLE} characters and the task to {MAX_TEXT}"
        )));
    }
    let spec = register(&state, &request.draft).map_err(|problems| {
        Failure::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "not-accepted",
            problems.join("\n"),
        )
    })?;
    // The name passed check_task (an identifier), so it is a plain file name.
    if let Some(dir) = workbench_dir(&state) {
        let saved = Saved {
            key: request.key,
            draft: request.draft,
        };
        if let Err(e) = save_json(&dir, &format!("{}.json", saved.draft.name), &saved) {
            tracing::warn!(error = %e, "could not save the accepted sequence");
            return Err(Failure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "not-saved",
                "accepted, but it could not be saved and will be gone after a restart",
            ));
        }
    }
    Ok((StatusCode::CREATED, Json(AcceptView { spec: spec.detail })))
}

async fn list(State(state): State<AppState>) -> Json<Vec<SpecSummary>> {
    Json(state.workbench_specs())
}

fn workbench_dir(state: &AppState) -> Option<PathBuf> {
    state
        .authoring()
        .config
        .records
        .as_ref()
        .map(|d| d.join("workbench"))
}

fn register(state: &AppState, draft: &SequenceDraft) -> Result<Spec, Vec<String>> {
    let (task, vectors) = from_draft(draft)?;
    let about = "Your own sequence: Muse Spark read it and proposed the scenarios; you reviewed and \
                 accepted them. There is no hand-built graph; only Muse Spark's verified drafts \
                 implement it.";
    let spec = Spec::from_workbench(&draft.name, &draft.title, about, task, vectors);
    state
        .add_workbench_spec(spec.clone())
        .map_err(|e| vec![e])?;
    Ok(spec)
}

/// Restores accepted sequences saved by earlier runs of this server.
pub fn restore(state: &AppState) {
    let Some(dir) = workbench_dir(state) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    for path in paths {
        let saved = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Saved>(&t).ok());
        match saved.map(|s| register(state, &s.draft)) {
            Some(Ok(_)) => {}
            Some(Err(problems)) => {
                tracing::warn!(path = %path.display(), ?problems, "skipping a saved sequence")
            }
            None => tracing::warn!(path = %path.display(), "skipping an unreadable saved sequence"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn digests_change_with_the_document() {
        let a = ReadRequest {
            text: "one".into(),
            attachments: vec![],
        };
        let b = ReadRequest {
            text: " one \n".into(),
            attachments: vec![],
        };
        let c = ReadRequest {
            text: "two".into(),
            attachments: vec![],
        };
        assert_eq!(digest(&a), digest(&b), "surrounding space does not matter");
        assert_ne!(digest(&a), digest(&c));
        assert!(digest(&a).starts_with("read:"));
    }

    #[test]
    fn only_pdfs_and_images_are_attached() {
        let ok = WorkbenchAttachment {
            name: "a.png".into(),
            mime: "image/png".into(),
            data_base64: "iVBORw0K".into(),
        };
        assert!(matches!(attachment(&ok), Ok(Attachment::Image { .. })));
        let doc = WorkbenchAttachment {
            mime: "application/msword".into(),
            ..ok.clone()
        };
        assert!(attachment(&doc).is_err());
        let junk = WorkbenchAttachment {
            data_base64: "%%%".into(),
            ..ok
        };
        assert!(attachment(&junk).is_err());
    }
}
