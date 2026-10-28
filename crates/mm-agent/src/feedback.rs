//! The repair turn's text: what the model hears after a failed attempt. Codes come first so the
//! model can act on them; engine messages are prose and only a hint.

use mm_bridge::{InputSpec, LoadFailure, Scalar, Scenario, ScenarioOutcome, VectorFile};

use crate::repair::{AttemptOutcome, Feedback};

/// The only catalog classes that break an algebraic loop in the pinned engine: they output the
/// previous step's value. Checked against the engine in `tests/feedback.rs`.
const LOOP_BREAKERS: &str = "Discrete.UnitDelay (Real) or Logical.Pre (Boolean)";

pub(crate) fn feedback_message(
    outcome: &AttemptOutcome,
    feedback: Feedback,
    vectors: &VectorFile,
) -> String {
    let mut out = String::from("Your block graph was not accepted.\n");
    match outcome {
        AttemptOutcome::Passed { .. } | AttemptOutcome::HarnessError { .. } => {
            return String::new();
        }
        AttemptOutcome::ModelError { error } => out.push_str(&format!("Model error: {error}\n")),
        AttemptOutcome::NotAGraph { detail } => {
            out.push_str(&format!("It is not a valid block graph: {detail}\n"));
        }
        AttemptOutcome::InterfaceMismatch { problems } => {
            out.push_str("Its interface differs from the task:\n");
            problems
                .iter()
                .for_each(|p| out.push_str(&format!("- {p}\n")));
        }
        AttemptOutcome::Invalid { findings } => {
            out.push_str("The validator found:\n");
            for f in findings {
                out.push_str(&format!(
                    "- {} at {}: {}\n",
                    f.code.as_str(),
                    f.subject,
                    f.detail
                ));
            }
        }
        AttemptOutcome::LoadFailed { failure } => load_failed(&mut out, failure),
        AttemptOutcome::ReplayFailed {
            passed,
            total,
            failures,
        } => match feedback {
            Feedback::ValidatorOnly => out.push_str(
                "It loads, but its behavior does not match the task. Re-read the task and check \
                 signs, thresholds, timing and port order.\n",
            ),
            Feedback::WithVectors => {
                out.push_str(&format!(
                    "It loads, but {} of {total} test scenarios failed:\n",
                    total - passed
                ));
                for s in failures {
                    let scenario = vectors.scenarios.iter().find(|v| v.name == s.name);
                    let about = scenario
                        .map(|v| v.description.as_str())
                        .filter(|d| !d.is_empty())
                        .map_or(String::new(), |d| format!(" ({d})"));
                    out.push_str(&format!("- {}{about}: {}\n", s.name, describe(&s.outcome)));
                    if let (ScenarioOutcome::Fail { t, .. }, Some(scenario)) =
                        (&s.outcome, scenario)
                    {
                        out.push_str(&format!(
                            "  Inputs at t = {t} s: {}\n",
                            inputs_at(scenario, *t)
                        ));
                    }
                }
            }
        },
    }
    out.push_str("Reply with a corrected block graph.");
    out
}

pub(crate) fn load_failed(out: &mut String, failure: &LoadFailure) {
    out.push_str(&format!(
        "The control engine refused it at stage {} ({}).\n",
        failure.stage, failure.code
    ));
    if failure.code == "algebraic-loop" && !failure.diagnostics.is_empty() {
        // One diagnostic per loop member, in loop order; a chain of block ids reads better
        // than a list of connector URNs.
        let chain = loop_chain(failure);
        out.push_str(&format!(
            "- algebraic-loop: {chain}. Each block in this cycle computes its output from its \
             input in the same step, so the cycle has no start. Every feedback path must pass \
             through {LOOP_BREAKERS}, which outputs the previous step's value. Sampler, \
             ZeroOrderHold, TriggeredSampler, Latch, TrueDelay and TrueFalseHold pass the \
             current value through and do not break a loop.\n"
        ));
        return;
    }
    for d in &failure.diagnostics {
        let subject = d.subject.as_deref().unwrap_or("-");
        out.push_str(&format!("- {} at {subject}: {}\n", d.code, d.message));
    }
    // Some refusals carry no diagnostic records; the engine's error text is then the only
    // explanation, so pass it on.
    if failure.diagnostics.is_empty()
        && let Some(error) = &failure.error
    {
        out.push_str(&format!("- {}\n", error.detail));
    }
}

/// `a -> b -> c -> a` from the loop members' subjects (`urn:...#<sequence>.<block>.<port>`).
fn loop_chain(failure: &LoadFailure) -> String {
    let mut blocks: Vec<&str> = Vec::new();
    for d in &failure.diagnostics {
        let Some(subject) = d.subject.as_deref() else {
            continue;
        };
        let local = subject.rsplit_once('#').map_or(subject, |(_, l)| l);
        let mut parts = local.split('.');
        // `<sequence>.<block>.<port>`: the block is the middle part when there are three.
        let block = match (parts.next(), parts.next(), parts.next()) {
            (Some(_), Some(block), Some(_)) | (Some(block), Some(_), None) => block,
            (Some(block), None, _) => block,
            _ => continue,
        };
        if blocks.last() != Some(&block) {
            blocks.push(block);
        }
    }
    // Close the cycle (a block feeding itself reads `a -> a`).
    if let Some(first) = blocks.first().copied() {
        blocks.push(first);
    }
    blocks.join(" -> ")
}

fn describe(outcome: &ScenarioOutcome) -> String {
    match outcome {
        ScenarioOutcome::Pass => "passed".into(),
        ScenarioOutcome::Fail {
            t,
            output,
            expected,
            actual,
            from_s,
            to_s,
        } => format!(
            "at t = {t} s, {output} should be {} (checked from {from_s} to {to_s} s) but was {}",
            wire(expected),
            wire(actual)
        ),
        ScenarioOutcome::Error { code, detail, .. } => format!("error {code}: {detail}"),
    }
}

/// Each input's value at `t` and when it last changed, so a timing miss can be traced to the
/// edge that caused it. Inputs are in name order (the scenario map is sorted).
pub(crate) fn inputs_at(scenario: &Scenario, t: f64) -> String {
    let mut parts: Vec<String> = scenario
        .inputs
        .iter()
        .map(|(name, spec)| match spec {
            InputSpec::Constant(v) => format!("{name} = {} (constant)", scalar(*v)),
            InputSpec::Steps(steps) => {
                let mut sorted: Vec<_> = steps.iter().filter(|s| s.t <= t).collect();
                sorted.sort_by(|a, b| a.t.total_cmp(&b.t));
                match sorted.last() {
                    Some(step) => format!("{name} = {} (since {} s)", scalar(step.value), step.t),
                    None => format!("{name} = not yet set"),
                }
            }
        })
        .collect();
    parts.extend(
        scenario
            .defaults
            .iter()
            .filter(|(name, _)| !scenario.inputs.contains_key(*name))
            .map(|(name, v)| format!("{name} = {} (constant)", scalar(*v))),
    );
    parts.join(", ")
}

fn scalar(value: Scalar) -> String {
    match value {
        Scalar::Bool(b) => b.to_string(),
        Scalar::Number(n) => n.to_string(),
    }
}

fn wire(value: &mm_bridge::WireValue) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use mm_bridge::DiagnosticDto;

    fn member(port: &str) -> DiagnosticDto {
        DiagnosticDto {
            stage: "schedule",
            code: "algebraic-loop".into(),
            severity: "error",
            subject: Some(format!("urn:mechanical-muse:s#s.{port}")),
            message: String::new(),
        }
    }

    #[test]
    fn a_loop_reads_as_a_chain_of_blocks() {
        let failure = LoadFailure {
            stage: "schedule",
            code: "algebraic-loop".into(),
            diagnostics: ["add.u1", "add.y", "hold.u", "hold.y", "lim.u", "lim.y"]
                .into_iter()
                .map(member)
                .collect(),
            error: None,
        };
        assert_eq!(loop_chain(&failure), "add -> hold -> lim -> add");
        let mut out = String::new();
        load_failed(&mut out, &failure);
        assert!(out.contains("add -> hold -> lim -> add"), "{out}");
        assert!(out.contains("Discrete.UnitDelay"), "{out}");
        assert!(!out.contains("urn:"), "{out}");
    }

    #[test]
    fn inputs_show_their_last_edge() {
        let scenario: Scenario = serde_json::from_value(serde_json::json!({
            "name": "x",
            "inputs": {
                "n": 0,
                "sta": [{"t": 0.0, "value": false}, {"t": 4200.0, "value": true},
                        {"t": 4500.0, "value": false}]
            },
            "expect": []
        }))
        .unwrap();
        assert_eq!(
            inputs_at(&scenario, 4200.0),
            "n = 0 (constant), sta = true (since 4200 s)"
        );
        assert_eq!(
            inputs_at(&scenario, 4140.0),
            "n = 0 (constant), sta = false (since 0 s)"
        );
    }
}
