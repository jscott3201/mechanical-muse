//! Where each sequence input comes from and where each output goes. Inputs come from BOPTEST
//! readings, constants, or other sequences' outputs; outputs go to BOPTEST `_u` points through
//! a clamp and an optional gate.

use std::collections::BTreeMap;

use mm_bridge::WireValue;
use serde::{Deserialize, Serialize};

use crate::plant::Measurements;

/// A typed constant (the engine needs every input in its declared type).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum Scalar {
    Real(f64),
    Int(i64),
    Bool(bool),
}

impl Scalar {
    fn wire(self) -> WireValue {
        match self {
            Scalar::Real(x) => WireValue::Real(x),
            Scalar::Int(n) => WireValue::Integer(n),
            Scalar::Bool(b) => WireValue::Boolean(b),
        }
    }
}

/// A value source for a sequence input or an output gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Source {
    /// A BOPTEST reading, as a Real.
    Measurement {
        point: String,
    },
    /// True while a BOPTEST reading is above `threshold` (e.g. occupancy flags sent as 0/1).
    MeasurementAbove {
        point: String,
        threshold: f64,
    },
    Constant {
        value: Scalar,
    },
    /// An output of a sequence that runs earlier in the same tick.
    Output {
        sequence: String,
        output: String,
    },
    /// True if any source is true.
    AnyTrue {
        of: Vec<Source>,
    },
    /// The sum of integer sources (e.g. requests from several air handlers).
    SumInt {
        of: Vec<Source>,
    },
}

/// Why a source could not give a value this tick. Any of these degrades the sequence.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SourceError {
    #[error("reading {point} is missing")]
    Missing { point: String },
    #[error("reading {point} is not finite")]
    NotFinite { point: String },
    #[error("output {sequence}.{output} is not available")]
    NoOutput { sequence: String, output: String },
    #[error("{what}: expected {expected}")]
    WrongType {
        what: String,
        expected: &'static str,
    },
}

/// Outputs produced so far this tick, by sequence then output name.
pub type TickOutputs = BTreeMap<String, BTreeMap<String, WireValue>>;

impl Source {
    /// The type this source always gives, in the engine's words, or `None` when it depends on
    /// another sequence's output. Used to reject bad bindings before the run starts.
    pub fn static_type(&self) -> Option<&'static str> {
        match self {
            Source::Measurement { .. } => Some("a Real"),
            Source::MeasurementAbove { .. } | Source::AnyTrue { .. } => Some("a Boolean"),
            Source::SumInt { .. } => Some("an Integer"),
            Source::Constant { value } => Some(match value {
                Scalar::Real(_) => "a Real",
                Scalar::Int(_) => "an Integer",
                Scalar::Bool(_) => "a Boolean",
            }),
            Source::Output { .. } => None,
        }
    }

    /// Every `(sequence, output)` this source reads, including nested ones.
    pub fn outputs_read(&self) -> Vec<(&str, &str)> {
        match self {
            Source::Output { sequence, output } => vec![(sequence.as_str(), output.as_str())],
            Source::AnyTrue { of } | Source::SumInt { of } => {
                of.iter().flat_map(Source::outputs_read).collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn eval(
        &self,
        readings: &Measurements,
        outputs: &TickOutputs,
    ) -> Result<WireValue, SourceError> {
        let reading = |point: &str| -> Result<f64, SourceError> {
            let x = *readings.get(point).ok_or_else(|| SourceError::Missing {
                point: point.to_owned(),
            })?;
            if x.is_finite() {
                Ok(x)
            } else {
                Err(SourceError::NotFinite {
                    point: point.to_owned(),
                })
            }
        };
        Ok(match self {
            Source::Measurement { point } => WireValue::Real(reading(point)?),
            Source::MeasurementAbove { point, threshold } => {
                WireValue::Boolean(reading(point)? > *threshold)
            }
            Source::Constant { value } => value.wire(),
            Source::Output { sequence, output } => outputs
                .get(sequence)
                .and_then(|o| o.get(output))
                .cloned()
                .ok_or_else(|| SourceError::NoOutput {
                    sequence: sequence.clone(),
                    output: output.clone(),
                })?,
            Source::AnyTrue { of } => {
                let mut any = false;
                for s in of {
                    match s.eval(readings, outputs)? {
                        WireValue::Boolean(b) => any |= b,
                        _ => return Err(wrong(s, "a Boolean")),
                    }
                }
                WireValue::Boolean(any)
            }
            Source::SumInt { of } => {
                let mut sum: i64 = 0;
                for s in of {
                    match s.eval(readings, outputs)? {
                        WireValue::Integer(n) => sum = sum.saturating_add(n),
                        _ => return Err(wrong(s, "an Integer")),
                    }
                }
                WireValue::Integer(sum)
            }
        })
    }
}

fn wrong(source: &Source, expected: &'static str) -> SourceError {
    SourceError::WrongType {
        what: format!("{source:?}"),
        expected,
    }
}

/// One sequence input (short name, as in the spec) and its source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputBinding {
    pub input: String,
    pub source: Source,
}

/// One sequence output sent to a BOPTEST point (base name, without `_u`), clamped to
/// `[min, max]`, written only while `gate` (if any) is true.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputBinding {
    pub output: String,
    pub point: String,
    pub min: f64,
    pub max: f64,
    #[serde(default)]
    pub gate: Option<Source>,
}
