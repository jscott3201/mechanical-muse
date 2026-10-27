//! Types for run sessions and their event stream. One envelope shape for every event:
//! `{run_id, seq, sim_time_s, kind, ...}`, where `kind` tags the payload and `sim_time_s` is
//! BOPTEST simulation time (absent before the plant is running).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a client may ask for. Everything is optional; the server fills in the demo defaults
/// and refuses values outside its limits.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, TS)]
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
}

/// The settings a run actually uses.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RunConfig {
    pub baseline: bool,
    pub requests: bool,
    pub start_s: f64,
    pub warmup_s: f64,
    pub steps: u32,
    pub step_s: f64,
    pub case: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Starting,
    Running,
    Completed,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Verifying,
    Selecting,
    Initializing,
    Running,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RunInfo {
    pub id: String,
    pub config: RunConfig,
    pub state: RunState,
    pub ticks: u32,
    pub events: u32,
}

/// A sequence in the run and the receipt it was loaded with.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SequenceInfo {
    pub name: String,
    pub spec: String,
    pub content_id: String,
}

/// A fail-safe or clamp event from the supervisor, flattened for the UI.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SupervisorEvent {
    /// `stale`, `degraded`, `clamped`, `rejected`, `held`, `released` or `no-such-output`.
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

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
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

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
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
