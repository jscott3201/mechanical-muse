//! Types for run sessions and their event stream. One envelope shape for every event:
//! `{run_id, seq, sim_time_s, kind, ...}`, where `kind` tags the payload and `sim_time_s` is
//! BOPTEST simulation time (absent before the plant is running).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a client may ask for. Everything is optional; the server fills in the demo defaults
/// and refuses values outside its limits.
/// Send `{}` for all defaults; an empty body is refused.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    /// Run BOPTEST's own controllers only, writing nothing (for comparison).
    #[serde(default)]
    #[ts(optional)]
    pub baseline: Option<bool>,
    /// Feed the plant reset the air handlers' requests (default true).
    #[serde(default)]
    #[ts(optional)]
    pub requests: Option<bool>,
    /// Simulation start, seconds from January 1.
    #[serde(default)]
    #[ts(optional)]
    pub start_s: Option<f64>,
    #[serde(default)]
    #[ts(optional)]
    pub warmup_s: Option<f64>,
    #[serde(default)]
    #[ts(optional)]
    pub steps: Option<u32>,
    /// Use a verified draft instead of a spec's reference graph: spec name to the draft's
    /// content id (see `/api/drafts`). Its vectors are replayed again before it loads.
    #[serde(default)]
    #[ts(optional)]
    pub drafts: Option<BTreeMap<String, String>>,
    /// A test fault: replace one sensor reading with NaN for a few steps, to show the
    /// fail-safe. Labelled as injected in every tick it touches.
    #[serde(default)]
    #[ts(optional)]
    pub fault: Option<Fault>,
}

/// A sensor reading replaced with NaN before the sequences see it, for `steps` steps from
/// step `from_step` (0-based). Only the air-handler readings the sequences use can be faulted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    pub point: String,
    pub from_step: u32,
    pub steps: u32,
}

/// The settings a run actually uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunConfig {
    pub baseline: bool,
    pub requests: bool,
    pub start_s: f64,
    pub warmup_s: f64,
    pub steps: u32,
    pub step_s: f64,
    pub case: String,
    /// Spec name to the content id of the drafted graph used for it.
    #[serde(default)]
    pub drafts: BTreeMap<String, String>,
    /// The injected test fault, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fault: Option<Fault>,
    /// Set when this run plays back a recorded run instead of driving a plant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub replay: Option<ReplaySource>,
}

/// The record a replayed run plays back. Its events are the recorded ones, re-sent in order
/// at a fixed pace; nothing runs against a plant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReplaySource {
    /// The record's file name, without `.jsonl`.
    pub record: String,
    /// The id the run had when it was recorded.
    pub recorded_run: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Starting,
    Running,
    Completed,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Verifying,
    Selecting,
    Initializing,
    Running,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunInfo {
    pub id: String,
    pub config: RunConfig,
    pub state: RunState,
    pub ticks: u32,
    pub events: u32,
    /// BOPTEST's KPIs for the run, once it has reported them (at the end).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kpis: Option<BTreeMap<String, f64>>,
}

/// Where a running sequence's graph came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Our hand-built reference graph (also what records from before drafts existed mean).
    #[default]
    Reference,
    /// Written by the model and verified against the spec's vectors.
    Drafted,
}

/// A sequence in the run and the receipt it was loaded with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SequenceInfo {
    pub name: String,
    pub spec: String,
    pub content_id: String,
    #[serde(default)]
    pub origin: Origin,
    /// The model that drafted it, for drafted graphs.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    /// Whether that draft came from a live model call or a recorded run, for drafted graphs.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<crate::author_dto::ModelSource>,
}

/// A fail-safe or clamp event from the supervisor, flattened for the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SupervisorEvent {
    /// `stale`, `degraded`, `clamped`, `rejected`, `held`, `released`, `no-such-output`, or
    /// `injected` (a test fault replaced `point`'s reading this step).
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub point: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub value: Option<f64>,
    /// The clamped value actually sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sent: Option<f64>,
    /// Why a sequence produced nothing (degraded only).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunEvent {
    Started {
        config: RunConfig,
        sequences: Vec<SequenceInfo>,
    },
    Phase {
        phase: Phase,
    },
    /// One control step: what each sequence computed, what BOPTEST was sent, and the readings
    /// worth showing. Non-finite readings never reach this point (the host drops them).
    Tick {
        #[ts(
            type = "Record<string, Record<string, number | boolean | string | { class: number, ordinal: number }>>"
        )]
        outputs: serde_json::Value,
        writes: BTreeMap<String, f64>,
        readings: BTreeMap<String, f64>,
        events: Vec<SupervisorEvent>,
    },
    Kpi {
        values: BTreeMap<String, f64>,
    },
    /// The last event of every run.
    Ended {
        state: RunState,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        message: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Envelope {
    pub run_id: String,
    /// Position in the run's event log, from 0; also the SSE event id.
    #[ts(type = "number")]
    pub seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sim_time_s: Option<f64>,
    #[serde(flatten)]
    pub event: RunEvent,
}

/// A recorded run that can be replayed.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ReplayInfo {
    pub name: String,
    pub recorded_run: String,
    pub ticks: u32,
    pub baseline: bool,
    /// Simulation start, seconds from January 1.
    pub start_s: f64,
    /// True when the recorded run used a model-drafted graph.
    pub drafted: bool,
    /// How the recorded run ended, if the record has its end.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<RunState>,
    /// BOPTEST's KPIs as recorded, if the record has them.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kpis: Option<BTreeMap<String, f64>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
#[serde(deny_unknown_fields)]
pub struct ReplayRequest {
    /// A name from `GET /api/replays`.
    pub record: String,
}
