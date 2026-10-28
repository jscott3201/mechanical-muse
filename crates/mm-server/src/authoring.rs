//! Authoring sessions. One at a time (each live attempt is a paid model call). A session's task
//! drives `mm_agent::author_observed` and publishes every step to a journal; a verified draft
//! goes to the draft store. The model is Muse Spark through the Meta Model API, or a recorded
//! live run played back through the same checks.

use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::FutureExt;
use mm_agent::{
    AttemptOutcome, Effort, ModelClient, ModelError, ModelReply, ModelRequest, Progress,
    RecordedClient, Recording, SparkClient, ToolLimits,
};

use crate::author_dto::{
    AuthorConfig, AuthorEnvelope, AuthorEvent, AuthorInfo, AuthorRequest, AuthorState, DraftInfo,
    ModelSource, TokenUsage,
};
use crate::author_view::{attempt_view, tokens, tool_call_view};
use crate::drafts::{Draft, DraftStore};
use crate::journal::{Entry, Journal, lock};
use crate::routes::AppState;
use crate::runs::StartError;

/// The product's model (AGENTS.md: `muse-spark-1.3` for our unpublished specs).
pub const MODEL: &str = "muse-spark-1.3";
const MAX_ATTEMPTS: u32 = 5;
const MAX_OUTPUT_TOKENS: u32 = 32_000;
/// A session that has not finished by then ends as failed and frees the slot.
const DEADLINE: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
pub struct AuthoringConfig {
    /// The live client; `None` when no API key is configured.
    pub spark: Option<Arc<SparkClient>>,
    /// Folder of recorded runs, `<spec>.json` each (`mm_agent::Recording`).
    pub recordings: Option<PathBuf>,
    /// Wall time per recorded reply, so a viewer sees the drafting step.
    pub pace: Duration,
    /// Folder for JSON-lines session records.
    pub records: Option<PathBuf>,
    /// Folder for verified drafts.
    pub drafts: Option<PathBuf>,
    /// Folder of committed (recorded) graph explanations.
    pub explanations: Option<PathBuf>,
    /// Folder of committed (recorded) sequence readings for the workbench.
    pub readings: Option<PathBuf>,
    /// Folder of committed (recorded) BOPTEST binding proposals for the workbench.
    pub bindings: Option<PathBuf>,
}

impl Default for AuthoringConfig {
    fn default() -> Self {
        Self {
            spark: None,
            recordings: None,
            pace: Duration::from_secs(2),
            records: None,
            drafts: None,
            explanations: None,
            readings: None,
            bindings: None,
        }
    }
}

impl Entry for AuthorEnvelope {
    fn seq(&self) -> u64 {
        self.seq
    }

    fn kind(&self) -> &'static str {
        match &self.event {
            AuthorEvent::Started { .. } => "started",
            AuthorEvent::Drafting { .. } => "drafting",
            AuthorEvent::ToolCall { .. } => "tool-call",
            AuthorEvent::Checked { .. } => "checked",
            AuthorEvent::Ended { .. } => "ended",
        }
    }

    fn is_last(&self) -> bool {
        matches!(self.event, AuthorEvent::Ended { .. })
    }
}

pub struct AuthorSession {
    pub id: String,
    pub config: AuthorConfig,
    effort: Effort,
    recording: Option<Recording>,
    state: Mutex<AuthorState>,
    journal: Arc<Journal<AuthorEnvelope>>,
}

impl AuthorSession {
    pub fn info(&self) -> AuthorInfo {
        AuthorInfo {
            id: self.id.clone(),
            config: self.config.clone(),
            state: *lock(&self.state),
            events: u32::try_from(self.journal.len()).unwrap_or(u32::MAX),
        }
    }

    pub fn journal(&self) -> Arc<Journal<AuthorEnvelope>> {
        Arc::clone(&self.journal)
    }

    fn publish(&self, event: AuthorEvent) {
        self.journal.append(|seq| AuthorEnvelope {
            session_id: self.id.clone(),
            seq,
            event,
        });
    }
}

#[derive(Default)]
struct Registry {
    next: u64,
    sessions: BTreeMap<String, Arc<AuthorSession>>,
}

pub struct Authoring {
    pub config: AuthoringConfig,
    pub drafts: DraftStore,
    registry: Mutex<Registry>,
}

impl Authoring {
    pub fn new(config: AuthoringConfig) -> Self {
        let drafts = DraftStore::open(config.drafts.clone());
        Self {
            config,
            drafts,
            registry: Mutex::new(Registry::default()),
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<AuthorSession>> {
        lock(&self.registry).sessions.get(id).cloned()
    }

    pub fn list(&self) -> Vec<AuthorInfo> {
        lock(&self.registry)
            .sessions
            .values()
            .map(|s| s.info())
            .collect()
    }

    /// Registers a session for `spec` if none is running. The caller spawns [`run`].
    pub fn create(
        &self,
        spec: &str,
        task: Option<&mm_agent::TaskSpec>,
        request: &AuthorRequest,
    ) -> Result<Arc<AuthorSession>, StartError> {
        let source = request.source.unwrap_or(if self.config.spark.is_some() {
            ModelSource::Live
        } else {
            ModelSource::Recorded
        });
        let effort_name = request.effort.clone().unwrap_or_else(|| "high".into());
        let effort = match effort_name.as_str() {
            "low" => Effort::Low,
            "medium" => Effort::Medium,
            "high" => Effort::High,
            _ => {
                return Err(StartError::Invalid(
                    "effort must be low, medium or high".into(),
                ));
            }
        };
        let max_attempts = request.attempts.unwrap_or(MAX_ATTEMPTS);
        if !(1..=MAX_ATTEMPTS).contains(&max_attempts) {
            return Err(StartError::Invalid(format!(
                "attempts must be 1 to {MAX_ATTEMPTS}"
            )));
        }
        let (model, recording, effort_name, max_attempts, tools) = match source {
            ModelSource::Live => {
                if self.config.spark.is_none() {
                    return Err(StartError::Invalid(
                        "no model API key is configured on the server; use source \"recorded\""
                            .into(),
                    ));
                }
                let tools = request.tools.unwrap_or(false);
                (MODEL.to_owned(), None, effort_name, max_attempts, tools)
            }
            ModelSource::Recorded => {
                // A playback replays what the model said then: the effort it ran at is part of
                // the recording, and there are only as many attempts as recorded replies.
                // Tool turns are replayed as recorded, so the tools follow the recording.
                let recording = self.recording(spec, task, request.tools.unwrap_or(false))?;
                let replies = u32::try_from(recording.attempts()).unwrap_or(u32::MAX);
                let tools = recording.uses_tools();
                (
                    recording.model.clone(),
                    Some(recording),
                    "as recorded".to_owned(),
                    max_attempts.min(replies.max(1)),
                    tools,
                )
            }
        };
        let mut reg = lock(&self.registry);
        if let Some(active) = reg
            .sessions
            .values()
            .find(|s| *lock(&s.state) == AuthorState::Running)
        {
            return Err(StartError::Active(active.id.clone()));
        }
        reg.next += 1;
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let id = format!("author-{started}-{}", reg.next);
        let session = Arc::new(AuthorSession {
            id: id.clone(),
            config: AuthorConfig {
                spec: spec.to_owned(),
                source,
                model,
                effort: effort_name,
                max_attempts,
                feedback: "with-vectors".into(),
                tools,
            },
            effort,
            recording,
            state: Mutex::new(AuthorState::Running),
            journal: Arc::new(Journal::new(&id, self.config.records.as_deref())),
        });
        reg.sessions.insert(id, Arc::clone(&session));
        Ok(session)
    }

    /// The recorded session for `spec`: `<spec>.json`, or `<spec>.tools.json` when a session
    /// with Spark's own checks is asked for. Failing that, a recording of the same kind whose
    /// task matches.
    fn recording(
        &self,
        spec: &str,
        task: Option<&mm_agent::TaskSpec>,
        tools: bool,
    ) -> Result<Recording, StartError> {
        let kind = if tools { " with tools" } else { "" };
        let missing = || StartError::Invalid(format!("no recorded run{kind} for {spec}"));
        let dir = self.config.recordings.as_ref().ok_or_else(missing)?;
        let file = if tools {
            format!("{spec}.tools.json")
        } else {
            format!("{spec}.json")
        };
        let Ok(bytes) = std::fs::read(dir.join(file)) else {
            // The same task accepted under another name: a recording whose task matches.
            let key = task.map(crate::specs::task_key).ok_or_else(missing)?;
            let mut paths: Vec<_> = std::fs::read_dir(dir)
                .map_err(|_| missing())?
                .flatten()
                .map(|e| e.path())
                .collect();
            paths.sort();
            return paths
                .into_iter()
                .find_map(|p| {
                    let r: Recording = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
                    let same = r.task_digest.as_deref() == Some(key.as_str());
                    (same && r.uses_tools() == tools).then_some(r)
                })
                .ok_or_else(missing);
        };
        let recording: Recording = serde_json::from_slice(&bytes)
            .map_err(|e| StartError::Invalid(format!("the recorded run for {spec} is bad: {e}")))?;
        if recording.task != spec {
            return Err(StartError::Invalid(format!(
                "the recorded run for {spec} is for {}",
                recording.task
            )));
        }
        Ok(recording)
    }
}

/// The live client or a playback, behind one `ModelClient`.
enum AnyModel {
    Spark(Arc<SparkClient>),
    Recorded(RecordedClient),
}

impl ModelClient for AnyModel {
    async fn complete(&self, request: &ModelRequest) -> Result<ModelReply, ModelError> {
        match self {
            AnyModel::Spark(c) => c.complete(request).await,
            AnyModel::Recorded(c) => c.complete(request).await,
        }
    }
}

/// Runs a session to the end and publishes its `ended` event, whatever happens.
pub async fn run(session: Arc<AuthorSession>, state: AppState) {
    session.publish(AuthorEvent::Started {
        config: session.config.clone(),
    });
    let outcome = tokio::time::timeout(
        DEADLINE,
        AssertUnwindSafe(author(&session, &state)).catch_unwind(),
    )
    .await
    .map_err(|_| "the session ran past its 30-minute deadline".to_owned())
    .and_then(|r| r.unwrap_or_else(|_| Err("the authoring task panicked".into())));
    let (final_state, draft, message, usage) = match outcome {
        Ok(done) => done,
        Err(message) => {
            tracing::warn!(session = %session.id, %message, "authoring failed");
            (
                AuthorState::Failed,
                None,
                Some(message),
                TokenUsage::default(),
            )
        }
    };
    *lock(&session.state) = final_state;
    session.publish(AuthorEvent::Ended {
        state: final_state,
        draft,
        message,
        usage,
    });
}

type Done = (AuthorState, Option<DraftInfo>, Option<String>, TokenUsage);

async fn author(session: &AuthorSession, state: &AppState) -> Result<Done, String> {
    let spec = state
        .spec(&session.config.spec)
        .ok_or_else(|| format!("spec {} is not loaded", session.config.spec))?;
    let authoring = state.authoring();
    let client = match &session.recording {
        Some(recording) => AnyModel::Recorded(RecordedClient::new(
            recording.clone(),
            authoring.config.pace,
        )),
        None => AnyModel::Spark(
            authoring
                .config
                .spark
                .clone()
                .ok_or("no model API key is configured")?,
        ),
    };
    let config = mm_agent::AuthorConfig {
        model: session.config.model.clone(),
        effort: Some(session.effort),
        max_output_tokens: MAX_OUTPUT_TOKENS,
        max_attempts: session.config.max_attempts,
        feedback: mm_agent::Feedback::WithVectors,
    };
    let palette = state.palette();
    // Replay runs inline on this worker thread; one session at a time and well under a second
    // per draft, so it does not starve the runtime.
    let observe = |progress: Progress<'_>| match progress {
        Progress::Drafting { attempt, of } => {
            session.publish(AuthorEvent::Drafting { attempt, of });
        }
        Progress::ToolCall { attempt, call } => session.publish(AuthorEvent::ToolCall {
            attempt,
            call: tool_call_view(call),
        }),
        Progress::Checked(attempt) => session.publish(AuthorEvent::Checked {
            attempt: attempt_view(attempt, &spec.task.name, palette),
        }),
    };
    let (task, vectors) = (&spec.task, &spec.vectors);
    let trace = if session.config.tools {
        let limits = ToolLimits::default();
        mm_agent::author_with_tools(
            &client,
            palette,
            task,
            vectors,
            &[],
            &config,
            limits,
            observe,
        )
        .await
    } else {
        mm_agent::author_observed(&client, palette, task, vectors, &[], &config, observe).await
    };
    if session.recording.is_none() {
        crate::recording::save(
            authoring.config.records.as_deref(),
            &session.id,
            &trace,
            &spec.task,
        );
    }
    let usage = tokens(trace.usage);
    if let Some(error) = trace.setup_error {
        return Ok((AuthorState::Failed, None, Some(error), usage));
    }
    let Some(verified) = trace.verified else {
        let harness = trace.attempts.iter().find_map(|a| match &a.outcome {
            AttemptOutcome::HarnessError { detail } => Some(detail.clone()),
            _ => None,
        });
        return Ok(match harness {
            Some(detail) => (AuthorState::Failed, None, Some(detail), usage),
            None => (AuthorState::Unverified, None, None, usage),
        });
    };
    let info = DraftInfo {
        content_id: verified.content_id.clone(),
        spec: session.config.spec.clone(),
        scenarios: u32::try_from(verified.scenarios).unwrap_or(u32::MAX),
        model: session.config.model.clone(),
        source: session.config.source,
        session: session.id.clone(),
        attempts_used: u32::try_from(trace.attempts.len()).unwrap_or(u32::MAX),
    };
    authoring.drafts.insert(Draft {
        info: info.clone(),
        graph: verified.graph,
    });
    Ok((AuthorState::Verified, Some(info), None, usage))
}
