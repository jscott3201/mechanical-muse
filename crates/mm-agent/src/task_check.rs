//! Checks that a task and its scenarios can be a fair verification gate (see [`check_task`]).
//! Shared by the model's reading and the engineer's edits.

use std::collections::{BTreeMap, BTreeSet};

use mm_bridge::{InputSpec, Scalar, Step, VectorFile};
use mm_graph::SignalType;

use crate::prompt::TaskSpec;

/// Longest scenario horizon accepted (one simulated week), and most scenarios kept.
pub(crate) const MAX_HORIZON_S: f64 = 7.0 * 86_400.0;
pub(crate) const MAX_SCENARIOS: usize = 12;

pub(crate) fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && s.len() <= 64
}

pub(crate) fn snake(s: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            // A capital after a lowercase letter or digit starts a word (camelCase).
            if c.is_ascii_uppercase() && prev_lower && !out.ends_with('_') {
                out.push('_');
            }
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
            out.push(c.to_ascii_lowercase());
        } else {
            prev_lower = false;
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
        }
    }
    let out = out
        .trim_end_matches('_')
        .chars()
        .take(48)
        .collect::<String>();
    if out.is_empty() {
        "my_sequence".into()
    } else {
        out
    }
}

pub(crate) fn on_grid(t: f64, step: f64) -> bool {
    let k = (t / step).round();
    (t - k * step).abs() <= 1e-6 * step.max(1.0)
}

pub(crate) fn fits(ty: SignalType, v: Scalar) -> bool {
    match (ty, v) {
        (SignalType::Bool, Scalar::Bool(_)) | (SignalType::Real, Scalar::Number(_)) => true,
        (SignalType::Int, Scalar::Number(n)) => n.fract() == 0.0,
        _ => false,
    }
}

/// Problems that would stop `task` and `vectors` from being a fair verification gate: names,
/// coverage of every input in every scenario, steps from t = 0 on the clock grid, value types,
/// windows inside the run. Used on the model's reading and again on the engineer's edits. It
/// cannot tell whether an expected value is right; the engineer decides that.
pub fn check_task(task: &TaskSpec, vectors: &VectorFile) -> Vec<String> {
    let mut problems = Vec::new();
    if !is_identifier(&task.name) {
        problems.push(format!(
            "the sequence name `{}` is not an identifier",
            task.name
        ));
    }
    let mut seen = BTreeSet::new();
    for p in task.inputs.iter().chain(&task.outputs) {
        if !is_identifier(&p.name) {
            problems.push(format!("point name `{}` is not an identifier", p.name));
        }
        if !seen.insert(p.name.clone()) {
            problems.push(format!("point `{}` is declared twice", p.name));
        }
    }
    if task.inputs.is_empty() || task.outputs.is_empty() {
        problems.push("the sequence needs at least one input and one output".into());
    }
    let inputs: BTreeMap<&str, SignalType> = task
        .inputs
        .iter()
        .map(|p| (p.name.as_str(), p.ty))
        .collect();
    let outputs: BTreeMap<&str, SignalType> = task
        .outputs
        .iter()
        .map(|p| (p.name.as_str(), p.ty))
        .collect();
    let clock = vectors.clock;
    let clock_ok = clock.step_s.is_finite()
        && clock.step_s > 0.0
        && clock.horizon_s.is_finite()
        && clock.horizon_s >= clock.step_s
        && clock.horizon_s <= MAX_HORIZON_S;
    if !clock_ok {
        problems.push(format!(
            "the clock (step {} s, horizon {} s) must have a positive step and a horizon of at most one week",
            clock.step_s, clock.horizon_s
        ));
    }
    if vectors.scenarios.is_empty() {
        problems.push("there are no scenarios".into());
    }
    if vectors.scenarios.len() > MAX_SCENARIOS {
        problems.push(format!("at most {MAX_SCENARIOS} scenarios"));
    }
    let mut names = BTreeSet::new();
    for s in &vectors.scenarios {
        let name = &s.name;
        if !is_identifier(name) {
            problems.push(format!("scenario name `{name}` is not an identifier"));
        }
        if !names.insert(name.clone()) {
            problems.push(format!("scenario `{name}` appears twice"));
        }
        for (point, spec) in &s.inputs {
            let Some(&ty) = inputs.get(point.as_str()) else {
                problems.push(format!(
                    "scenario `{name}` sets `{point}`, which is not an input"
                ));
                continue;
            };
            let steps: Vec<Step> = match spec {
                InputSpec::Constant(v) => vec![Step { t: 0.0, value: *v }],
                InputSpec::Steps(steps) => steps.clone(),
            };
            if steps.first().is_none_or(|first| first.t != 0.0) {
                problems.push(format!(
                    "scenario `{name}`: input `{point}` must start at t = 0"
                ));
            }
            if steps.windows(2).any(|w| matches!(w, [a, b] if b.t <= a.t)) {
                problems.push(format!(
                    "scenario `{name}`: input `{point}` steps are not in time order"
                ));
            }
            for step in &steps {
                if !fits(ty, step.value) {
                    problems.push(format!(
                        "scenario `{name}`: input `{point}` gets a value of the wrong type at {} s",
                        step.t
                    ));
                }
                if clock_ok && (!on_grid(step.t, clock.step_s) || step.t > clock.horizon_s) {
                    problems.push(format!(
                        "scenario `{name}`: input `{point}` changes at {} s, off the clock",
                        step.t
                    ));
                }
            }
        }
        for point in inputs.keys() {
            if !s.inputs.contains_key(*point) && !s.defaults.contains_key(*point) {
                problems.push(format!("scenario `{name}` does not give input `{point}`"));
            }
        }
        if !s
            .expect
            .iter()
            .any(|e| outputs.contains_key(e.output.as_str()))
        {
            problems.push(format!("scenario `{name}` checks nothing"));
        }
        for e in &s.expect {
            let Some(&ty) = outputs.get(e.output.as_str()) else {
                problems.push(format!(
                    "scenario `{name}` checks `{}`, which is not an output",
                    e.output
                ));
                continue;
            };
            if !fits(ty, e.equals) {
                problems.push(format!(
                    "scenario `{name}`: `{}` is expected with a value of the wrong type",
                    e.output
                ));
            }
            if !(e.from_s <= e.to_s && e.from_s >= 0.0 && (!clock_ok || e.to_s <= clock.horizon_s))
            {
                problems.push(format!(
                    "scenario `{name}`: the window {}–{} s for `{}` is not inside the run",
                    e.from_s, e.to_s, e.output
                ));
            }
        }
    }
    problems
}
