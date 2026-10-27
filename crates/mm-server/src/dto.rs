//! Types that cross the HTTP boundary. Each derives `TS`; `cargo test` writes the matching
//! TypeScript to `web/src/types/generated` (see `.cargo/config.toml`). Never hand-write a mirror.

use serde::Serialize;
use ts_rs::TS;

/// A signal type as the engine names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum Signal {
    Real,
    Integer,
    Boolean,
}

/// An error body. The UI branches on `code` and only displays `message`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Health {
    pub status: String,
    /// open-control-engine commit every graph is checked against.
    pub engine_rev: String,
    pub specs: u32,
    /// The model a live authoring session would call, when the server has a key for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub live_model: Option<String>,
    /// Recorded model sessions can be played back.
    pub recorded_model: bool,
    /// The BACnet mirror's device instance, when it is running.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bacnet_device: Option<u32>,
}

/// One input, output or parameter of a spec's task card.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TaskPoint {
    pub name: String,
    /// Signal type for points; absent for parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#type: Option<Signal>,
    pub unit: String,
    pub meaning: String,
    /// Parameter value; absent for points.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SpecSummary {
    /// Folder name under `specs/`; the id used in every URL.
    pub name: String,
    /// The spec's first heading.
    pub title: String,
    pub inputs: Vec<TaskPoint>,
    pub outputs: Vec<TaskPoint>,
    pub scenarios: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ScenarioInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SpecDetail {
    pub summary: SpecSummary,
    /// The spec's opening paragraph (`spec.md`, our own words), for people.
    pub about: String,
    /// The task text given to the model.
    pub description: String,
    pub params: Vec<TaskPoint>,
    pub scenarios: Vec<ScenarioInfo>,
    /// Engine clock of the vectors.
    pub step_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PortView {
    pub name: String,
    pub signal: Signal,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ParamView {
    pub name: String,
    #[ts(type = "number | boolean | string")]
    pub value: serde_json::Value,
}

/// A block with every port of its class, in the catalog's declaration order.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BlockView {
    pub id: String,
    pub class: String,
    pub inputs: Vec<PortView>,
    pub outputs: Vec<PortView>,
    pub params: Vec<ParamView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct BoundaryView {
    pub name: String,
    pub signal: Signal,
    pub description: String,
}

/// One end of a connection: a boundary point (`port` absent) or a block port.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct EndView {
    pub node: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub port: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ConnectionView {
    pub from: EndView,
    pub to: EndView,
    /// The type of the source port.
    pub signal: Signal,
}

/// A block graph ready to draw: typed ports come from the pinned engine catalog.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct GraphView {
    pub name: String,
    pub inputs: Vec<BoundaryView>,
    pub outputs: Vec<BoundaryView>,
    pub blocks: Vec<BlockView>,
    pub connections: Vec<ConnectionView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum ScenarioResult {
    Pass,
    /// The first tick where an expectation did not hold.
    Fail {
        t: f64,
        output: String,
        #[ts(type = "number | boolean | string | { class: number, ordinal: number }")]
        expected: serde_json::Value,
        #[ts(type = "number | boolean | string | { class: number, ordinal: number }")]
        actual: serde_json::Value,
        /// The expectation window that failed.
        from_s: f64,
        to_s: f64,
    },
    /// The scenario could not run (bad name, wrong type, refused frame).
    Error {
        code: String,
        detail: String,
        /// Simulation time of the failure, when it happened mid-scenario.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        t: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ScenarioReport {
    pub name: String,
    /// Frames executed.
    #[ts(type = "number")]
    pub ticks: u64,
    #[serde(flatten)]
    pub result: ScenarioResult,
}

/// A verification receipt: the CXF's content id and every scenario's result. Only a passing
/// receipt lets a sequence reach the plant (AGENTS.md hard rule 5).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Receipt {
    pub spec: String,
    pub content_id: String,
    pub passed: bool,
    pub scenarios: Vec<ScenarioReport>,
}

/// One replayed scenario, as the API shows it.
pub(crate) fn scenario_report(s: mm_bridge::ScenarioReport) -> ScenarioReport {
    use mm_bridge::ScenarioOutcome;
    ScenarioReport {
        name: s.name,
        ticks: s.ticks,
        result: match s.outcome {
            ScenarioOutcome::Pass => ScenarioResult::Pass,
            ScenarioOutcome::Fail {
                t,
                output,
                expected,
                actual,
                from_s,
                to_s,
            } => ScenarioResult::Fail {
                t,
                output,
                expected: serde_json::to_value(&expected).unwrap_or(serde_json::Value::Null),
                actual: serde_json::to_value(&actual).unwrap_or(serde_json::Value::Null),
                from_s,
                to_s,
            },
            ScenarioOutcome::Error { code, detail, t } => ScenarioResult::Error { code, detail, t },
        },
    }
}
