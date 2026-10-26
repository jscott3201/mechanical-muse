//! Vector replay: run each scenario on a fresh engine and check every expectation window.
//!
//! Semantics (restated from the library's old verifier for the frame API, `docs/engine-api.md`
//! section 9): ticks at `k * step_s` for `k = 0..=floor(horizon_s / step_s)`; an input step
//! applies from the first tick with `t >= step.t` and holds until the next; every input is
//! submitted on every frame; Reals compare within a tolerance (default 1e-9), everything else
//! exactly; the first violation ends the scenario.
//!
//! Every verdict must rest on checks that ran: a file with no scenarios, a scenario with no
//! expectations, or a window that contains no tick is rejected before anything runs.
//! Tick times are `k as f64 * step_s`; with a fractional `step_s` the last tick can fall short of
//! `horizon_s` by rounding, so our own specs use whole-second steps.

use std::collections::BTreeMap;

use oce_api::{Value, ValueType};
use serde::Serialize;

use crate::dto::WireValue;
use crate::load::{LoadFailure, load_cxf};
use crate::names::{NameError, resolve_name};
use crate::stepper::{FrameStepper, StepError, type_name};
use crate::vectors::{Clock, Expectation, InputSpec, Scalar, Scenario, VectorFile};

const DEFAULT_TOLERANCE: f64 = 1e-9;

/// The library's vector format, and ours (the same shape plus optional `defaults`).
pub const KNOWN_SCHEMAS: &[&str] = &["cxf-library/vectors/v1", "mechanical-muse/vectors/v1"];

/// Safety limits, the same as the library's old verifier.
#[derive(Debug, Clone, Copy)]
pub struct ReplayLimits {
    pub max_scenarios: usize,
    pub max_samples_per_scenario: u64,
    pub max_samples_total: u64,
}

impl Default for ReplayLimits {
    fn default() -> Self {
        Self {
            max_scenarios: 512,
            max_samples_per_scenario: 1_000_000,
            max_samples_total: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    /// The model did not load; no scenario ran.
    #[error(transparent)]
    Load(#[from] LoadFailure),
    /// The vector file breaks a rule or a limit; no scenario ran.
    #[error("vector file rejected ({code}): {detail}")]
    Invalid { code: &'static str, detail: String },
}

impl ReplayError {
    pub fn code(&self) -> &str {
        match self {
            ReplayError::Load(failure) => &failure.code,
            ReplayError::Invalid { code, .. } => code,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReplayReport {
    /// Engine content id of the model every scenario ran against.
    pub content_id: String,
    pub scenarios: Vec<ScenarioReport>,
}

impl ReplayReport {
    /// True only when there is at least one scenario and every one passed.
    pub fn passed(&self) -> bool {
        !self.scenarios.is_empty()
            && self
                .scenarios
                .iter()
                .all(|s| s.outcome == ScenarioOutcome::Pass)
    }

    pub fn pass_count(&self) -> usize {
        self.scenarios
            .iter()
            .filter(|s| s.outcome == ScenarioOutcome::Pass)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScenarioReport {
    pub name: String,
    /// Frames executed (all of them on a pass).
    pub ticks: u64,
    #[serde(flatten)]
    pub outcome: ScenarioOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum ScenarioOutcome {
    Pass,
    /// The first tick where an expectation did not hold.
    Fail {
        t: f64,
        output: String,
        expected: WireValue,
        actual: WireValue,
        from_s: f64,
        to_s: f64,
    },
    /// The scenario could not run to completion (bad name, wrong type, refused frame).
    Error {
        code: String,
        detail: String,
        t: Option<f64>,
    },
}

/// Replays every scenario against `cxf`. Each scenario gets its own freshly loaded engine.
pub fn replay(
    cxf: &[u8],
    vectors: &VectorFile,
    limits: &ReplayLimits,
) -> Result<ReplayReport, ReplayError> {
    let ticks = check_file(vectors, limits)?;
    let content_id = load_cxf(cxf)?.content_id;
    let mut scenarios = Vec::with_capacity(vectors.scenarios.len());
    for scenario in &vectors.scenarios {
        let loaded = load_cxf(cxf)?;
        let (ran, outcome) = match FrameStepper::new(loaded.engine) {
            Ok(stepper) => run_scenario(stepper, scenario, vectors.clock, ticks),
            Err(e) => (0, error(e.code(), e.to_string(), None)),
        };
        scenarios.push(ScenarioReport {
            name: scenario.name.clone(),
            ticks: ran,
            outcome,
        });
    }
    Ok(ReplayReport {
        content_id,
        scenarios,
    })
}

/// Validates the clock, windows and limits; returns the last tick index.
fn check_file(vectors: &VectorFile, limits: &ReplayLimits) -> Result<u64, ReplayError> {
    let invalid = |code, detail: String| Err(ReplayError::Invalid { code, detail });
    if !KNOWN_SCHEMAS.contains(&vectors.schema.as_str()) {
        return invalid("unknown-schema", format!("schema {}", vectors.schema));
    }
    let Clock { step_s, horizon_s } = vectors.clock;
    if !(step_s.is_finite() && step_s > 0.0) {
        return invalid(
            "bad-clock",
            format!("step_s must be positive, got {step_s}"),
        );
    }
    if !(horizon_s.is_finite() && horizon_s >= 0.0) {
        return invalid(
            "bad-clock",
            format!("horizon_s must be >= 0, got {horizon_s}"),
        );
    }
    if vectors.scenarios.len() > limits.max_scenarios {
        return invalid(
            "too-many-scenarios",
            format!("{} > {}", vectors.scenarios.len(), limits.max_scenarios),
        );
    }
    let last = (horizon_s / step_s).floor();
    // Compared as f64 first so a huge horizon cannot overflow the cast.
    if last + 1.0 > limits.max_samples_per_scenario as f64 {
        return invalid(
            "too-many-samples",
            format!("{} ticks per scenario", last + 1.0),
        );
    }
    let last = last as u64;
    let total = (last + 1).saturating_mul(vectors.scenarios.len() as u64);
    if total > limits.max_samples_total {
        return invalid("too-many-samples", format!("{total} ticks in total"));
    }
    // A verdict must rest on checks that actually ran: no empty files, no empty expectation
    // lists, and every window must contain at least one tick.
    if vectors.scenarios.is_empty() {
        return invalid("no-scenarios", "the file has no scenarios".into());
    }
    for scenario in &vectors.scenarios {
        if scenario.expect.is_empty() {
            return invalid(
                "no-expectations",
                format!("{} checks nothing", scenario.name),
            );
        }
        for e in &scenario.expect {
            let what = format!(
                "{}: {} window {}..{}",
                scenario.name, e.output, e.from_s, e.to_s
            );
            if !(e.from_s.is_finite() && e.to_s.is_finite() && e.from_s <= e.to_s) {
                return invalid("bad-window", what);
            }
            if first_tick_in(e.from_s, e.to_s, step_s, last).is_none() {
                return invalid("window-misses-ticks", format!("{what} contains no tick"));
            }
            if let Some(tol) = e.tolerance {
                if !(tol.is_finite() && tol >= 0.0) {
                    return invalid("bad-tolerance", format!("{what}: tolerance {tol}"));
                }
                if matches!(e.equals, Scalar::Bool(_)) {
                    return invalid(
                        "bad-tolerance",
                        format!("{what}: tolerance on a Boolean expectation"),
                    );
                }
            }
        }
    }
    Ok(last)
}

/// The first tick index `k <= last` whose time `k * step_s` lies in `[from_s, to_s]`, using the
/// same tick-time arithmetic as the replay loop (the candidates around the estimate absorb
/// rounding in the division).
fn first_tick_in(from_s: f64, to_s: f64, step_s: f64, last: u64) -> Option<u64> {
    let estimate = (from_s / step_s).ceil().max(0.0);
    if estimate > last as f64 + 1.0 {
        return None;
    }
    let start = (estimate as u64).saturating_sub(1);
    (start..=last.min(start + 2)).find(|&k| {
        let t = k as f64 * step_s;
        t >= from_s && t <= to_s
    })
}

fn error(code: &str, detail: String, t: Option<f64>) -> ScenarioOutcome {
    ScenarioOutcome::Error {
        code: code.to_owned(),
        detail,
        t,
    }
}

fn name_error(e: &NameError) -> &'static str {
    match e {
        NameError::Unknown(_) => "unknown-name",
        NameError::Ambiguous { .. } => "ambiguous-name",
    }
}

/// Per-input event lists (time, value), sorted by time, keyed by canonical path.
type Events = BTreeMap<String, Vec<(f64, Value)>>;

fn build_events(stepper: &FrameStepper, scenario: &Scenario) -> Result<Events, ScenarioOutcome> {
    let paths = stepper.input_paths();
    let mut events: Events = BTreeMap::new();
    let resolve = |name: &str| {
        resolve_name(name, &paths)
            .map(str::to_owned)
            .map_err(|e| error(name_error(&e), format!("input {e}"), None))
    };
    let convert = |path: &str, scalar: Scalar| {
        let ty = stepper.input_type(path).unwrap_or(ValueType::String);
        to_value(scalar, ty).ok_or_else(|| {
            error(
                "input-type-mismatch",
                format!("{path} expects {}, got {scalar:?}", type_name(ty)),
                None,
            )
        })
    };
    for (name, spec) in &scenario.inputs {
        let path = resolve(name)?;
        let mut list = match spec {
            InputSpec::Constant(v) => vec![(0.0, convert(&path, *v)?)],
            InputSpec::Steps(steps) => steps
                .iter()
                .map(|s| Ok((s.t, convert(&path, s.value)?)))
                .collect::<Result<Vec<_>, _>>()?,
        };
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        if events.insert(path.clone(), list).is_some() {
            return Err(error(
                "duplicate-input",
                format!("{path} is set twice"),
                None,
            ));
        }
    }
    // A default only fills an input the scenario leaves unset; two defaults for one input are
    // a mistake.
    let mut defaulted = std::collections::BTreeSet::new();
    for (name, scalar) in &scenario.defaults {
        let path = resolve(name)?;
        if !defaulted.insert(path.clone()) {
            return Err(error(
                "duplicate-input",
                format!("{path} has two defaults"),
                None,
            ));
        }
        let value = convert(&path, *scalar)?;
        events
            .entry(path)
            .or_insert_with(|| vec![(f64::NEG_INFINITY, value)]);
    }
    if let Some(missing) = paths.iter().find(|p| !events.contains_key(*p)) {
        return Err(error(
            "unset-input",
            format!("scenario never sets {missing}; add it to inputs or defaults"),
            None,
        ));
    }
    Ok(events)
}

fn run_scenario(
    mut stepper: FrameStepper,
    scenario: &Scenario,
    clock: Clock,
    last_tick: u64,
) -> (u64, ScenarioOutcome) {
    let events = match build_events(&stepper, scenario) {
        Ok(events) => events,
        Err(outcome) => return (0, outcome),
    };
    let mut cursor: BTreeMap<&str, usize> = BTreeMap::new();
    // Output paths for each expectation, resolved against the first frame's outputs.
    let mut targets: Option<Vec<String>> = None;

    for k in 0..=last_tick {
        let t = k as f64 * clock.step_s;
        for (path, list) in &events {
            let next = cursor.entry(path.as_str()).or_insert(0);
            while let Some((at, value)) = list.get(*next) {
                if *at > t {
                    break;
                }
                if let Err(e) = stepper.set(path, value.clone()) {
                    return (k, step_error(&e, t));
                }
                *next += 1;
            }
        }
        let frame = match stepper.step(t) {
            Ok(frame) => frame,
            Err(e) => return (k, step_error(&e, t)),
        };
        let outputs = frame.outputs();
        let targets = match &targets {
            Some(found) => found,
            None => {
                let paths: Vec<String> = outputs.iter().map(|(p, _)| p.clone()).collect();
                let mut found = Vec::with_capacity(scenario.expect.len());
                for e in &scenario.expect {
                    match resolve_name(&e.output, &paths) {
                        Ok(path) => found.push(path.to_owned()),
                        Err(err) => {
                            return (
                                k + 1,
                                error(name_error(&err), format!("output {err}"), Some(t)),
                            );
                        }
                    }
                }
                targets.insert(found)
            }
        };
        for (expect, path) in scenario.expect.iter().zip(targets) {
            if t < expect.from_s || t > expect.to_s {
                continue;
            }
            let Some(actual) = outputs.iter().find(|(p, _)| p == path).map(|(_, v)| v) else {
                return (
                    k + 1,
                    error("missing-output", format!("{path} not in frame"), Some(t)),
                );
            };
            match matches(expect, actual) {
                Ok(true) => {}
                Ok(false) => {
                    return (
                        k + 1,
                        ScenarioOutcome::Fail {
                            t,
                            output: expect.output.clone(),
                            expected: expected_wire(expect.equals, actual),
                            actual: WireValue::from_value(actual),
                            from_s: expect.from_s,
                            to_s: expect.to_s,
                        },
                    );
                }
                Err(detail) => return (k + 1, error("expect-type-mismatch", detail, Some(t))),
            }
        }
    }
    (last_tick + 1, ScenarioOutcome::Pass)
}

fn step_error(e: &StepError, t: f64) -> ScenarioOutcome {
    error(e.code(), e.to_string(), Some(t))
}

fn to_value(scalar: Scalar, ty: ValueType) -> Option<Value> {
    let whole = |x: f64| x.is_finite() && x.fract() == 0.0;
    match (ty, scalar) {
        (ValueType::Real, Scalar::Number(x)) => Some(Value::Real(x)),
        (ValueType::Integer, Scalar::Number(x)) if whole(x) && x.abs() <= f64::from(i32::MAX) => {
            Some(Value::Integer(x as i64))
        }
        (ValueType::Boolean, Scalar::Bool(b)) => Some(Value::Boolean(b)),
        (ValueType::Enum(class), Scalar::Number(x))
            if whole(x) && x >= 1.0 && x <= f64::from(u32::MAX) =>
        {
            Some(Value::Enum {
                class,
                ordinal: x as u32,
            })
        }
        _ => None,
    }
}

fn matches(expect: &Expectation, actual: &Value) -> Result<bool, String> {
    let tolerance = expect.tolerance.unwrap_or(DEFAULT_TOLERANCE);
    if expect.tolerance.is_some() && !matches!(actual, Value::Real(_)) {
        return Err(format!(
            "{} has a tolerance but the output is not a Real",
            expect.output
        ));
    }
    match (expect.equals, actual) {
        (Scalar::Bool(want), Value::Boolean(got)) => Ok(want == *got),
        (Scalar::Number(want), Value::Real(got)) => Ok((got - want).abs() <= tolerance),
        (Scalar::Number(want), Value::Integer(got)) => Ok(*got as f64 == want),
        (Scalar::Number(want), Value::Enum { ordinal, .. }) => Ok(f64::from(*ordinal) == want),
        (want, got) => Err(format!(
            "{} expects {want:?} but the output is {got:?}",
            expect.output
        )),
    }
}

/// The expected value, typed like the output it was compared with.
fn expected_wire(scalar: Scalar, actual: &Value) -> WireValue {
    match (scalar, actual) {
        (Scalar::Bool(b), _) => WireValue::Boolean(b),
        (Scalar::Number(x), Value::Integer(_)) => WireValue::Integer(x as i64),
        (Scalar::Number(x), Value::Enum { class, .. }) => WireValue::Enum {
            class: class.0,
            ordinal: x as u32,
        },
        (Scalar::Number(x), _) => WireValue::Real(x),
    }
}
