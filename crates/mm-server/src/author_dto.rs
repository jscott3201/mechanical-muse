//! Types for authoring sessions: Muse Spark drafts a spec's graph, every draft is checked
//! (interface, validator, engine load, vector replay), and problems go back to the model until
//! a draft passes or the attempt budget runs out. One envelope shape for every event:
//! `{session_id, seq, kind, ...}`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dto::{GraphView, ScenarioReport};

/// Where the model's replies come from. `recorded` plays back a stored live run through the
/// same checks; the UI must say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum ModelSource {
    Live,
    Recorded,
}

/// Send `{}` for the defaults (the server's configured source, high effort, 5 attempts).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct AuthorRequest {
    #[serde(default)]
    #[ts(optional)]
    pub source: Option<ModelSource>,
    /// `low`, `medium` or `high`.
    #[serde(default)]
    #[ts(optional)]
    pub effort: Option<String>,
    /// Drafts allowed, including the first (1 to 5).
    #[serde(default)]
    #[ts(optional)]
    pub attempts: Option<u32>,
    /// Let Spark check its own drafts with function tools (`check_graph`, `replay_scenario`,
    /// `describe_block`) before it answers. Default false. A recorded session uses whatever its
    /// recording did.
    #[serde(default)]
    #[ts(optional)]
    pub tools: Option<bool>,
}

/// The settings a session actually uses.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AuthorConfig {
    pub spec: String,
    pub source: ModelSource,
    pub model: String,
    pub effort: String,
    pub max_attempts: u32,
    /// Always `with-vectors` here: repair messages name the failing scenarios.
    pub feedback: String,
    /// Spark may call its own checks while drafting. They never count as verification.
    pub tools: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum AuthorState {
    Running,
    /// A draft passed every scenario; its receipt is stored.
    Verified,
    /// The budget ran out (or the model stopped answering) without a passing draft.
    Unverified,
    /// Our side failed; nothing about the model can be concluded.
    Failed,
}

/// A problem found in a draft. `node` is the block or boundary point it concerns, when known,
/// so a graph view can highlight it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Problem {
    pub code: String,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub node: Option<String>,
    pub message: String,
}

/// What checking one draft found.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum AttemptResult {
    ModelError {
        message: String,
    },
    HarnessError {
        message: String,
    },
    NotAGraph {
        message: String,
    },
    InterfaceMismatch {
        problems: Vec<String>,
    },
    Invalid {
        problems: Vec<Problem>,
    },
    LoadFailed {
        stage: String,
        code: String,
        problems: Vec<Problem>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        detail: Option<String>,
    },
    ReplayFailed {
        passed: u32,
        total: u32,
        failures: Vec<ScenarioReport>,
    },
    Passed {
        content_id: String,
        scenarios: u32,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TokenUsage {
    #[ts(type = "number")]
    pub input: u64,
    #[ts(type = "number")]
    pub output: u64,
    #[ts(type = "number")]
    pub reasoning: u64,
}

/// One of Spark's own checks while drafting: a tool call and what our check said. Spark's
/// check, not the receipt; the receipt is the attempt's result.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ToolCallView {
    /// `check_graph`, `replay_scenario` or `describe_block`.
    pub name: String,
    /// A short summary of the arguments (scenario or class, and the draft's size).
    pub args: String,
    pub ok: bool,
    /// One line, e.g. "scenario x failed at 240 s: y expected 285.93, got 291.48".
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AttemptView {
    pub index: u32,
    #[serde(flatten)]
    pub result: AttemptResult,
    pub usage: TokenUsage,
    /// Model calls this attempt made (1 without tools).
    pub calls: u32,
    /// Spark's own checks before it answered, in order (tools sessions only).
    pub tool_calls: Vec<ToolCallView>,
    /// The drafted graph, when the reply parsed and its blocks are in the catalog.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub graph: Option<GraphView>,
}

/// Which recorded sessions a spec can play back without a model key: a plain one, and one where
/// Spark checked its own drafts with tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RecordedSessions {
    pub plain: bool,
    pub tools: bool,
}

/// A stored draft that passed every scenario. Runs may load it only after its vectors replay
/// green again (AGENTS.md hard rule 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftInfo {
    pub content_id: String,
    pub spec: String,
    pub scenarios: u32,
    pub model: String,
    pub source: ModelSource,
    pub session: String,
    pub attempts_used: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AuthorEvent {
    Started {
        config: AuthorConfig,
    },
    /// A model call is under way.
    Drafting {
        attempt: u32,
        of: u32,
    },
    /// Spark called one of its own checks while drafting `attempt` (tools sessions only).
    ToolCall {
        attempt: u32,
        call: ToolCallView,
    },
    Checked {
        attempt: AttemptView,
    },
    /// The last event of every session.
    Ended {
        state: AuthorState,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        draft: Option<DraftInfo>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        message: Option<String>,
        usage: TokenUsage,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AuthorEnvelope {
    pub session_id: String,
    /// Position in the session's event log, from 0; also the SSE event id.
    #[ts(type = "number")]
    pub seq: u64,
    #[serde(flatten)]
    pub event: AuthorEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AuthorInfo {
    pub id: String,
    pub config: AuthorConfig,
    pub state: AuthorState,
    pub events: u32,
}
