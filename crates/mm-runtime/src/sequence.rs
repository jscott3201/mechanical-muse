//! A verified sequence running on its own engine. It loads only CXF whose content id matches a
//! passing vector replay (AGENTS.md hard rule 5), and checks every input binding up front.

use std::collections::BTreeMap;

use mm_bridge::{FrameStepper, ReplayReport, WireValue, load_cxf, resolve_name};
use serde::Serialize;

use crate::plant::Measurements;
use crate::wiring::{InputBinding, Source, SourceError, TickOutputs};

#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LoadError {
    #[error("{sequence}: CXF did not load ({code})")]
    Load { sequence: String, code: String },
    #[error("{sequence}: the verification replay did not pass")]
    NotPassed { sequence: String },
    #[error("{sequence}: content id {actual} is not the verified {expected}")]
    NotVerified {
        sequence: String,
        expected: String,
        actual: String,
    },
    #[error("{sequence}: input {input} has no binding")]
    Unbound { sequence: String, input: String },
    #[error("{sequence}: binding names no input {input}")]
    UnknownInput { sequence: String, input: String },
    #[error("{sequence}: input {input} is bound more than once")]
    DuplicateBinding { sequence: String, input: String },
    #[error("{sequence}: input {input} expects {expected}, its source gives {given}")]
    BindingType {
        sequence: String,
        input: String,
        expected: &'static str,
        given: &'static str,
    },
}

/// Why a sequence produced nothing this tick.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StepFailure {
    #[error("input {input}: {error}")]
    Input { input: String, error: SourceError },
    #[error("engine refused the frame ({code}): {detail}")]
    Refused { code: String, detail: String },
    /// Two outputs share a short name, so a binding could pick up the wrong one. The engine
    /// API does not list outputs before the first frame, so this is caught here, every tick.
    #[error("outputs {first} and {second} share a short name")]
    AmbiguousOutput { first: String, second: String },
}

pub struct RunningSequence {
    name: String,
    content_id: String,
    stepper: FrameStepper,
    inputs: Vec<(String, String, Source)>,
}

/// The short name of a boundary point: what follows the last `.` or `#` of its path.
fn short(path: &str) -> &str {
    path.rsplit(['.', '#']).next().unwrap_or(path)
}

impl RunningSequence {
    /// Loads `cxf` only if `receipt` is a passing replay of exactly this model, and checks that
    /// every engine input is bound exactly once to a source of the right type.
    pub fn load(
        name: &str,
        cxf: &[u8],
        receipt: &ReplayReport,
        bindings: Vec<InputBinding>,
    ) -> Result<Self, LoadError> {
        let sequence = || name.to_owned();
        if !receipt.passed() {
            return Err(LoadError::NotPassed {
                sequence: sequence(),
            });
        }
        let loaded = load_cxf(cxf).map_err(|f| LoadError::Load {
            sequence: sequence(),
            code: f.code,
        })?;
        if loaded.content_id != receipt.content_id {
            return Err(LoadError::NotVerified {
                sequence: sequence(),
                expected: receipt.content_id.clone(),
                actual: loaded.content_id,
            });
        }
        let content_id = loaded.content_id.clone();
        let stepper = FrameStepper::new(loaded.engine).map_err(|e| LoadError::Load {
            sequence: sequence(),
            code: e.code().to_owned(),
        })?;
        let paths = stepper.input_paths();
        let mut inputs: Vec<(String, String, Source)> = Vec::new();
        for binding in bindings {
            let path =
                resolve_name(&binding.input, &paths).map_err(|_| LoadError::UnknownInput {
                    sequence: sequence(),
                    input: binding.input.clone(),
                })?;
            if inputs.iter().any(|(p, _, _)| p == path) {
                return Err(LoadError::DuplicateBinding {
                    sequence: sequence(),
                    input: binding.input,
                });
            }
            let expected = stepper.input_type_name(path).unwrap_or("a known type");
            if let Some(given) = binding.source.static_type()
                && given != expected
            {
                return Err(LoadError::BindingType {
                    sequence: sequence(),
                    input: binding.input,
                    expected,
                    given,
                });
            }
            inputs.push((path.to_owned(), binding.input, binding.source));
        }
        if let Some(missing) = paths
            .iter()
            .find(|p| !inputs.iter().any(|(q, _, _)| q == *p))
        {
            return Err(LoadError::Unbound {
                sequence: sequence(),
                input: short(missing).to_owned(),
            });
        }
        Ok(Self {
            name: name.to_owned(),
            content_id,
            stepper,
            inputs,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn content_id(&self) -> &str {
        &self.content_id
    }

    /// The sources this sequence reads, for checking the run order.
    pub fn sources(&self) -> impl Iterator<Item = &Source> {
        self.inputs.iter().map(|(_, _, s)| s)
    }

    /// Evaluates every input, then runs one frame at `time`. Nothing is submitted unless every
    /// input has a value of the right type, so a bad reading never reaches the engine.
    pub fn step(
        &mut self,
        time: f64,
        readings: &Measurements,
        earlier: &TickOutputs,
    ) -> Result<BTreeMap<String, WireValue>, StepFailure> {
        let mut values = Vec::with_capacity(self.inputs.len());
        for (path, input, source) in &self.inputs {
            let value = source
                .eval(readings, earlier)
                .map_err(|error| StepFailure::Input {
                    input: input.clone(),
                    error,
                })?;
            values.push((path.clone(), input, value));
        }
        for (path, input, value) in &values {
            // Types of constants and readings were checked at load; this catches an output of
            // another sequence with the wrong type, before the engine sees anything.
            self.stepper
                .set_wire(path, value)
                .map_err(|e| StepFailure::Input {
                    input: (*input).clone(),
                    error: SourceError::WrongType {
                        what: format!("{value:?}"),
                        expected: match e {
                            mm_bridge::StepError::WrongType { expected, .. } => expected,
                            _ => "a known input",
                        },
                    },
                })?;
        }
        let outputs = self
            .stepper
            .step_wire(time)
            .map_err(|e| StepFailure::Refused {
                code: e.code().to_owned(),
                detail: e.to_string(),
            })?;
        let mut by_short: BTreeMap<String, (String, WireValue)> = BTreeMap::new();
        for (path, value) in outputs {
            let key = short(&path).to_owned();
            if let Some((first, _)) = by_short.get(&key) {
                return Err(StepFailure::AmbiguousOutput {
                    first: first.clone(),
                    second: path,
                });
            }
            by_short.insert(key, (path, value));
        }
        Ok(by_short.into_iter().map(|(k, (_, v))| (k, v)).collect())
    }
}
