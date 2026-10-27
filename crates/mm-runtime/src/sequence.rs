//! A verified sequence running on its own engine. It refuses to load CXF whose content id does
//! not match the verification receipt (AGENTS.md hard rule 5).

use std::collections::BTreeMap;

use mm_bridge::{FrameStepper, WireValue, load_cxf, resolve_name};
use serde::Serialize;

use crate::plant::Measurements;
use crate::wiring::{InputBinding, Source, SourceError, TickOutputs};

#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LoadError {
    #[error("{sequence}: CXF did not load ({code})")]
    Load { sequence: String, code: String },
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
}

/// Why a sequence produced nothing this tick.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StepFailure {
    #[error("input {input}: {error}")]
    Input { input: String, error: SourceError },
    #[error("engine refused the frame ({code}): {detail}")]
    Refused { code: String, detail: String },
}

pub struct RunningSequence {
    pub name: String,
    pub content_id: String,
    stepper: FrameStepper,
    inputs: Vec<(String, String, Source)>,
}

/// The short name of a boundary point: what follows the last `.` or `#` of its path.
fn short(path: &str) -> &str {
    path.rsplit(['.', '#']).next().unwrap_or(path)
}

impl RunningSequence {
    /// Loads `cxf` only if it is the verified model (`verified_content_id`), and checks that
    /// every engine input is bound exactly once.
    pub fn load(
        name: &str,
        cxf: &[u8],
        verified_content_id: &str,
        bindings: Vec<InputBinding>,
    ) -> Result<Self, LoadError> {
        let loaded = load_cxf(cxf).map_err(|f| LoadError::Load {
            sequence: name.to_owned(),
            code: f.code,
        })?;
        if loaded.content_id != verified_content_id {
            return Err(LoadError::NotVerified {
                sequence: name.to_owned(),
                expected: verified_content_id.to_owned(),
                actual: loaded.content_id,
            });
        }
        let content_id = loaded.content_id.clone();
        let stepper = FrameStepper::new(loaded.engine).map_err(|e| LoadError::Load {
            sequence: name.to_owned(),
            code: e.code().to_owned(),
        })?;
        let paths = stepper.input_paths();
        let mut inputs = Vec::new();
        for binding in bindings {
            let path =
                resolve_name(&binding.input, &paths).map_err(|_| LoadError::UnknownInput {
                    sequence: name.to_owned(),
                    input: binding.input.clone(),
                })?;
            inputs.push((path.to_owned(), binding.input, binding.source));
        }
        if let Some(missing) = paths
            .iter()
            .find(|p| !inputs.iter().any(|(q, _, _)| q == *p))
        {
            return Err(LoadError::Unbound {
                sequence: name.to_owned(),
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

    /// Evaluates every input, then runs one frame at `time`. Nothing is submitted unless every
    /// input has a value, so a bad reading never reaches the engine.
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
            values.push((path.clone(), value));
        }
        let refused = |e: mm_bridge::StepError| StepFailure::Refused {
            code: e.code().to_owned(),
            detail: e.to_string(),
        };
        for (path, value) in &values {
            self.stepper.set_wire(path, value).map_err(refused)?;
        }
        let outputs = self.stepper.step_wire(time).map_err(refused)?;
        Ok(outputs
            .into_iter()
            .map(|(path, value)| (short(&path).to_owned(), value))
            .collect())
    }
}
