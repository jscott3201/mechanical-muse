//! Re-scores the drafts stored in a run record on the engine this build pins, with no model
//! call: every recorded reply is checked again exactly as the loop checked it, and the verdict
//! is compared with the one recorded. Later attempts still answer the feedback the model saw at
//! the time, so this asks "would the same drafts get the same verdicts", not "would the model".

use std::collections::BTreeMap;

use mm_agent::{AttemptOutcome, TaskSpec, check_reply};
use mm_bridge::VectorFile;
use mm_graph::Palette;
use serde::Serialize;
use serde_json::Value;

use crate::BenchError;

/// One attempt whose verdict differs from the recorded one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Changed {
    pub rule: String,
    pub run: u64,
    pub attempt: u64,
    pub recorded: String,
    pub now: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RescoreReport {
    /// Rule-runs re-checked.
    pub rule_runs: usize,
    /// Replies re-checked (attempts with no reply, such as model errors, are skipped).
    pub replies: usize,
    pub changed: Vec<Changed>,
    /// Rule-runs whose first attempt passes now / passed then.
    pub first_try_now: usize,
    pub first_try_then: usize,
    /// Rule-runs with any passing attempt now / then.
    pub verified_now: usize,
    pub verified_then: usize,
}

/// A short verdict: the outcome kind, plus the pass count for replay failures.
pub fn verdict(outcome: &Value) -> String {
    let kind = outcome["result"].as_str().unwrap_or("?");
    match kind {
        "replay-failed" => format!(
            "replay-failed {}/{}",
            outcome["passed"].as_u64().unwrap_or(0),
            outcome["total"].as_u64().unwrap_or(0)
        ),
        "load-failed" => format!(
            "load-failed {}",
            outcome
                .pointer("/failure/code")
                .and_then(Value::as_str)
                .unwrap_or("?")
        ),
        other => other.to_owned(),
    }
}

/// Re-checks every reply in `record` (a run record as written by `mm-bench --out`). `inputs`
/// maps each rule id to the task the model was given and the vectors it was scored on.
pub fn rescore(
    record: &Value,
    inputs: &BTreeMap<String, (TaskSpec, VectorFile)>,
    palette: &Palette,
) -> Result<RescoreReport, BenchError> {
    let results = record["results"]
        .as_array()
        .ok_or_else(|| BenchError::Other("the record has no results".into()))?;
    let mut report = RescoreReport::default();
    for result in results {
        let rule = result["rule"].as_str().unwrap_or_default().to_owned();
        let (task, vectors) = inputs
            .get(&rule)
            .ok_or_else(|| BenchError::Other(format!("no task or vectors for {rule}")))?;
        let run = result["run"].as_u64().unwrap_or(0);
        let attempts = result
            .pointer("/trace/attempts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        report.rule_runs += 1;
        let (mut passed_now, mut passed_then) = (false, false);
        for (i, attempt) in attempts.iter().enumerate() {
            let then = verdict(&attempt["outcome"]);
            let Some(text) = attempt["reply"].as_str() else {
                continue;
            };
            report.replies += 1;
            let outcome = check_reply(text, task, palette, vectors);
            let now = verdict(&serde_json::to_value(&outcome)?);
            let passes = matches!(outcome, AttemptOutcome::Passed { .. });
            if i == 0 {
                report.first_try_now += usize::from(passes);
                report.first_try_then += usize::from(then == "passed");
            }
            passed_now |= passes;
            passed_then |= then == "passed";
            if now != then {
                report.changed.push(Changed {
                    rule: rule.clone(),
                    run,
                    attempt: attempt["index"].as_u64().unwrap_or(0),
                    recorded: then,
                    now,
                });
            }
        }
        report.verified_now += usize::from(passed_now);
        report.verified_then += usize::from(passed_then);
    }
    Ok(report)
}
