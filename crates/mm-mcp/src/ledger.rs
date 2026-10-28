//! Ledger tools over recorded runs: one run's Scope 1 and 2 ledger, and two runs of the same
//! window compared. The same code as the HTTP API's ledger routes; no model calls.

use mm_server::ledger::{self, RecordLedgerError, RunLedger};
use serde_json::{Value, json};

use crate::tools::{ToolError, ToolResult, Tools, string_arg};

/// The tool definitions, appended to the list in `tools.rs`.
pub fn definitions() -> Vec<Value> {
    let read_only = json!({"readOnlyHint": true, "idempotentHint": true, "openWorldHint": false});
    let record = |what: &str| {
        json!({"type": "string", "description": format!(
            "{what}: a recorded run's name, e.g. day143-morning-baseline (a `replay:` prefix is accepted)"
        )})
    };
    vec![
        json!({
            "name": "run_ledger",
            "title": "Scope 1 and 2 ledger of a recorded run",
            "description": "Energy (kWh), emissions (kgCO2e) and cost (in `factors.currency`: \
                USD for the office, EUR for the home) of one recorded BOPTEST run: Scope 1 (gas, \
                the office's boiler; none for the all-electric home) and Scope 2 (electricity), \
                from BOPTEST's own KPIs with its factors for the test case and checked against \
                them (`reconciles`). End uses only when the record has every power reading \
                (`measured`). Simulation results; heating and cooling loads only, no Scope 3.",
            "inputSchema": {
                "type": "object",
                "properties": {"record": record("The run")},
                "required": ["record"],
                "additionalProperties": false,
            },
            "annotations": read_only,
        }),
        json!({
            "name": "compare_runs",
            "title": "Compare two recorded runs",
            "description": "Both runs' ledgers and `delta` = b minus a (negative is a saving), \
                per scope, per end use when both are measured, and in thermal and CO2 \
                discomfort. Refused with window-mismatch unless both cover the same test case, \
                start, step length, number of steps and ticks recorded (a run stopped early differs).",
            "inputSchema": {
                "type": "object",
                "properties": {"a": record("The reference run"), "b": record("The run compared with it")},
                "required": ["a", "b"],
                "additionalProperties": false,
            },
            "annotations": read_only,
        }),
    ]
}

fn ledger_of(tools: &Tools, args: &Value, key: &str) -> Result<RunLedger, ToolError> {
    let name = string_arg(args, key)?
        .ok_or_else(|| ToolError::new("invalid-arguments", format!("missing argument: {key}")))?;
    let name = name.strip_prefix("replay:").unwrap_or(name);
    let dir = tools
        .replays
        .as_deref()
        .ok_or_else(|| ToolError::new("no-replays", "this server has no recorded runs"))?;
    ledger::of_record(dir, name).map_err(|e| match e {
        RecordLedgerError::Record(m) => ToolError::new("unknown-record", m),
        RecordLedgerError::Ledger(l) => ToolError::new(l.code(), l.to_string()),
    })
}

fn to_json<T: serde::Serialize>(value: &T) -> ToolResult {
    serde_json::to_value(value).map_err(|e| ToolError::new("internal", e.to_string()))
}

pub fn run_ledger(tools: &Tools, args: &Value) -> ToolResult {
    to_json(&ledger_of(tools, args, "record")?)
}

pub fn compare_runs(tools: &Tools, args: &Value) -> ToolResult {
    let a = ledger_of(tools, args, "a")?;
    let b = ledger_of(tools, args, "b")?;
    let c = ledger::compare(a, b).map_err(|e| ToolError::new(e.code(), e.to_string()))?;
    to_json(&c)
}
