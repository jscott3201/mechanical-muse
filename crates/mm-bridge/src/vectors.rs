//! Test-vector files: the library's `cxf-library/vectors/v1` shape, plus an optional per-scenario
//! `defaults` map of our own for inputs a scenario does not care about.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorFile {
    pub schema: String,
    pub clock: Clock,
    pub scenarios: Vec<Scenario>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
    pub step_s: f64,
    pub horizon_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Point name to a constant or a step list. Sorted map, so iteration is deterministic.
    pub inputs: BTreeMap<String, InputSpec>,
    /// Values for inputs the scenario leaves unset. Without one, an unset input is an error.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub defaults: BTreeMap<String, Scalar>,
    pub expect: Vec<Expectation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputSpec {
    Constant(Scalar),
    Steps(Vec<Step>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub t: f64,
    pub value: Scalar,
}

/// A JSON scalar. Bool is tried first so `true` never reads as a number.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    Bool(bool),
    Number(f64),
}

/// `output` must equal `equals` at every tick with `from_s <= t <= to_s`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    pub output: String,
    pub from_s: f64,
    pub to_s: f64,
    pub equals: Scalar,
    /// Absolute tolerance for Real outputs; default 1e-9.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
}
