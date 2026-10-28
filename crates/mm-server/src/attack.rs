//! "Try to break it": Muse Spark proposes new scenarios for an engineer's accepted sequence
//! (`mm_agent::attack`), and the server replays each one against a verified draft of it.
//!
//! Replaying is only a test: nothing runs on a building, and nothing here changes the sequence's
//! own scenarios or any receipt. A failing proposal means the draft or the proposal is wrong; the
//! engineer decides, and only proposals the engineer accepts as a new revision (through
//! `/api/workbench/accept`, with all its checks) ever gate a draft.
//!
//! Spark's proposals are keyed by a digest of what was sent (`attack:<hex>`) and kept in memory,
//! under `<records>/attacks`, and read-only from a recorded folder (committed fixtures), which
//! serves them labelled `recorded` when the server has no key. Replays are not cached: they are
//! cheap, deterministic, and depend on the draft named in the request.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use mm_bridge::{ReplayLimits, VectorFile};
use mm_graph::{EmitOptions, emit_cxf};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::author_dto::ModelSource;
use crate::dto::ScenarioReport;
use crate::routes::{AppState, Failure};
use crate::specs::Spec;
use crate::workbench::{ScenarioDraft, SequenceDraft, from_draft, saved_draft, to_draft};

/// Which verified draft to attack: its content id (a stored draft of this sequence).
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct AttackRequest {
    pub draft: String,
}

/// What replaying one proposal against the draft gave.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum AttackResult {
    Passed,
    /// The draft did not do what the proposal expects (or the proposal is wrong): the first
    /// failing check, as authoring reports it.
    Failed {
        failures: Vec<ScenarioReport>,
    },
}

/// One proposed scenario (`scenario.rationale` names the edge it probes) and its replay.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AttackProposal {
    pub scenario: ScenarioDraft,
    #[serde(flatten)]
    pub result: AttackResult,
}

/// Spark's proposals, model-written until the engineer accepts them, each replayed on the draft.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AttackView {
    /// Digest of what was sent to the model (`attack:<hex>`).
    pub key: String,
    pub model: String,
    pub source: ModelSource,
    /// Content id of the draft the proposals were replayed against.
    pub draft: String,
    /// The accepted sequence the proposals extend; a revision is this plus the chosen ones.
    pub sequence: SequenceDraft,
    pub proposals: Vec<AttackProposal>,
    /// Proposals the checks dropped, and why.
    pub problems: Vec<String>,
}

/// What is kept of one model reply: the checked proposals, not their replays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttackRecord {
    pub key: String,
    pub model: String,
    pub source: ModelSource,
    pub proposals: Vec<ScenarioDraft>,
    pub problems: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Attacks {
    cache: Mutex<BTreeMap<String, AttackRecord>>,
    /// Where live replies are written (`<records>/attacks`).
    store: Option<PathBuf>,
    /// Committed replies, served as recorded.
    recorded: Option<PathBuf>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `attack:abc` as a file name; keys are made here, so only hex follows the prefix.
fn file_name(key: &str) -> Option<String> {
    let stem = key.replace(':', "_");
    stem.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
        .then(|| format!("{stem}.json"))
}

fn read(dir: &Path, key: &str) -> Option<AttackRecord> {
    let text = std::fs::read_to_string(dir.join(file_name(key)?)).ok()?;
    serde_json::from_str::<AttackRecord>(&text)
        .ok()
        .filter(|r| r.key == key)
}

impl Attacks {
    pub fn new(store: Option<PathBuf>, recorded: Option<PathBuf>) -> Self {
        Self {
            store,
            recorded,
            ..Self::default()
        }
    }

    /// A known reply under `key`: this server's own first, then a recorded one.
    fn known(&self, key: &str) -> Option<AttackRecord> {
        if let Some(r) = lock(&self.cache).get(key) {
            return Some(r.clone());
        }
        let own = self.store.as_deref().and_then(|d| read(d, key));
        let found = own.or_else(|| {
            self.recorded
                .as_deref()
                .and_then(|d| read(d, key))
                .map(|mut r| {
                    r.source = ModelSource::Recorded;
                    r
                })
        })?;
        lock(&self.cache).insert(key.to_owned(), found.clone());
        Some(found)
    }

    fn remember(&self, record: &AttackRecord) {
        lock(&self.cache).insert(record.key.clone(), record.clone());
        let (Some(dir), Some(name)) = (&self.store, file_name(&record.key)) else {
            return;
        };
        let written = std::fs::create_dir_all(dir).and_then(|()| {
            let text = serde_json::to_string_pretty(record).map_err(std::io::Error::other)?;
            std::fs::write(dir.join(name), text)
        });
        if let Err(e) = written {
            tracing::warn!(error = %e, "could not save the proposals");
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/workbench/{name}/attack", post(attack))
}

fn bad(code: &str, message: impl Into<String>) -> Failure {
    Failure::new(StatusCode::BAD_REQUEST, code, message)
}

/// The accepted sequence as the engineer saved it, or rebuilt from the registered spec (then
/// without open questions or rationale) when this server did not save it.
fn sequence_of(state: &AppState, spec: &Spec) -> SequenceDraft {
    let name = &spec.detail.summary.name;
    saved_draft(state, name).unwrap_or_else(|| {
        to_draft(&mm_agent::ReadSequence {
            title: spec.detail.summary.title.clone(),
            task: spec.task.clone(),
            vectors: spec.vectors.clone(),
            rationale: Vec::new(),
            open_questions: Vec::new(),
            problems: Vec::new(),
        })
    })
}

/// The request Spark gets for sequence `name` and its key, or `None` for an unknown name or
/// one of our own specs. Public so tests (and recordings) can find the key a reply is filed
/// under.
pub fn attack_key(state: &AppState, name: &str) -> Option<String> {
    let spec = state.spec(name).filter(|s| !s.detail.reference)?;
    let sequence = sequence_of(state, &spec);
    Some(mm_agent::attack_key(&request_for(&spec, &sequence)))
}

fn request_for(spec: &Spec, sequence: &SequenceDraft) -> mm_agent::ModelRequest {
    mm_agent::attack_request(
        crate::authoring::MODEL,
        &spec.detail.summary.title,
        &spec.task,
        &spec.vectors,
        &sequence.open_questions,
    )
}

async fn attack(
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
    request: Result<Json<AttackRequest>, JsonRejection>,
) -> Result<Json<AttackView>, Failure> {
    let Json(request) = request.map_err(|e| bad("bad-request", e.body_text()))?;
    let spec = state
        .spec(&name)
        .ok_or_else(|| Failure::unknown_spec(&name))?;
    if spec.detail.reference {
        return Err(bad(
            "built-in",
            "only your own sequences can be attacked here; ours have hand-built references",
        ));
    }
    let draft = state
        .authoring()
        .drafts
        .get(&request.draft)
        .filter(|d| d.info.spec == name)
        .ok_or_else(|| {
            bad(
                "unknown-draft",
                "no verified draft of this sequence with that content id",
            )
        })?;
    let sequence = sequence_of(&state, &spec);
    let model_request = request_for(&spec, &sequence);
    let key = mm_agent::attack_key(&model_request);
    let record = match state.attacks().known(&key) {
        Some(found) => found,
        None => propose(&state, &spec, &sequence, &model_request, key).await?,
    };
    let AttackRecord {
        key,
        model,
        source,
        proposals,
        mut problems,
    } = record;
    let worker = state.clone();
    let base = sequence.clone();
    let vectors = spec.vectors.clone();
    let (proposals, mut unplayed) = tokio::task::spawn_blocking(move || {
        replay_all(&worker, &draft.graph, &base, &vectors, &proposals)
    })
    .await
    .map_err(|e| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()))?
    .map_err(|e| Failure::new(StatusCode::INTERNAL_SERVER_ERROR, "replay-failed", e))?;
    problems.append(&mut unplayed);
    Ok(Json(AttackView {
        key,
        model,
        source,
        draft: request.draft,
        sequence,
        proposals,
        problems,
    }))
}

/// One live call (one at a time on this server), checked and remembered.
async fn propose(
    state: &AppState,
    spec: &Spec,
    sequence: &SequenceDraft,
    request: &mm_agent::ModelRequest,
    key: String,
) -> Result<AttackRecord, Failure> {
    let client = state.authoring().config.spark.clone().ok_or_else(|| {
        Failure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "no-model",
            "no model key on this server, and no recorded proposals for this sequence",
        )
    })?;
    let _one = state.workbench_slot().try_lock().map_err(|_| {
        Failure::new(
            StatusCode::CONFLICT,
            "model-busy",
            "Muse Spark is already working on a sequence; try again when it finishes",
        )
    })?;
    let out = mm_agent::attack(client.as_ref(), request, &spec.task, &spec.vectors)
        .await
        .map_err(|e| Failure::new(StatusCode::BAD_GATEWAY, "model-error", e.to_string()))?;
    tracing::info!(
        proposals = out.attack.proposals.len(),
        dropped = out.attack.problems.len(),
        input = out.usage.input_tokens,
        output = out.usage.output_tokens,
        "Spark proposed scenarios to break a sequence"
    );
    let (rationale, scenarios): (Vec<String>, Vec<mm_bridge::Scenario>) = out
        .attack
        .proposals
        .into_iter()
        .map(|p| (p.rationale, p.scenario))
        .unzip();
    let as_drafts = to_draft(&mm_agent::ReadSequence {
        title: sequence.title.clone(),
        task: spec.task.clone(),
        vectors: VectorFile {
            scenarios,
            ..spec.vectors.clone()
        },
        rationale,
        open_questions: Vec::new(),
        problems: Vec::new(),
    });
    let record = AttackRecord {
        key,
        model: crate::authoring::MODEL.to_owned(),
        source: ModelSource::Live,
        proposals: as_drafts.scenarios,
        problems: out.attack.problems,
    };
    state.attacks().remember(&record);
    Ok(record)
}

/// Replays every proposal that converts against `graph` (one fresh engine per scenario). A
/// proposal that no longer converts (a recording made for an older form of the sequence) is
/// left out with its problems.
fn replay_all(
    state: &AppState,
    graph: &mm_graph::BlockGraph,
    base: &SequenceDraft,
    vectors: &VectorFile,
    proposals: &[ScenarioDraft],
) -> Result<(Vec<AttackProposal>, Vec<String>), String> {
    let mut kept = Vec::new();
    let mut scenarios = Vec::new();
    let mut problems = Vec::new();
    for p in proposals {
        let alone = SequenceDraft {
            scenarios: vec![p.clone()],
            ..base.clone()
        };
        match from_draft(&alone) {
            Ok((_, mut v)) if v.scenarios.len() == 1 => {
                scenarios.append(&mut v.scenarios);
                kept.push(p.clone());
            }
            Ok(_) => problems.push(format!("`{}` could not be replayed", p.name)),
            Err(found) => problems.push(format!(
                "`{}` could not be replayed: {}",
                p.name,
                found.join("; ")
            )),
        }
    }
    if scenarios.is_empty() {
        return Ok((Vec::new(), problems));
    }
    let cxf = emit_cxf(graph, state.palette(), &EmitOptions::default())
        .map_err(|e| format!("emit failed: {e}"))?;
    let file = VectorFile {
        scenarios,
        ..vectors.clone()
    };
    let report = mm_bridge::replay(&cxf, &file, &ReplayLimits::default())
        .map_err(|e| format!("replay failed: {e}"))?;
    let results = kept
        .into_iter()
        .zip(report.scenarios)
        .map(|(scenario, r)| {
            let result = if r.outcome == mm_bridge::ScenarioOutcome::Pass {
                AttackResult::Passed
            } else {
                AttackResult::Failed {
                    failures: vec![crate::dto::scenario_report(r)],
                }
            };
            AttackProposal { scenario, result }
        })
        .collect();
    Ok((results, problems))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::file_name;

    #[test]
    fn keys_become_safe_file_names() {
        assert_eq!(
            file_name("attack:00ff").as_deref(),
            Some("attack_00ff.json")
        );
        assert_eq!(file_name("../x"), None);
    }
}
