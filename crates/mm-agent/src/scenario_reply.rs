//! The test scenario as the model writes it, shared by the reader (`reader.rs`) and the attack
//! proposals (`attack.rs`): its schema, its reply shape, and the conversion to our vectors.

use std::collections::{BTreeMap, BTreeSet};

use mm_bridge::{Expectation, InputSpec, Scalar, Scenario, Step};
use mm_graph::SignalType;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::task_check::{is_identifier, snake};

pub(crate) fn scalar_schema() -> Value {
    json!({"anyOf": [{"type": "number"}, {"type": "boolean"}]})
}

/// One test scenario as the model writes it (shared with the attack proposals).
pub(crate) fn scenario_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "description", "inputs", "expect"],
        "properties": {
            "name": {"type": "string"},
            "description": {"type": "string"},
            "inputs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["point", "steps"],
                    "properties": {
                        "point": {"type": "string"},
                        "steps": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["t", "value"],
                                "properties": {"t": {"type": "number"}, "value": scalar_schema()}
                            }
                        }
                    }
                }
            },
            "expect": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["output", "from_s", "to_s", "equals", "tolerance"],
                    "properties": {
                        "output": {"type": "string"},
                        "from_s": {"type": "number"},
                        "to_s": {"type": "number"},
                        "equals": scalar_schema(),
                        "tolerance": {"type": "number"}
                    }
                }
            }
        }
    })
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ReplyScenario {
    pub(crate) name: String,
    description: String,
    inputs: Vec<ReplyInput>,
    expect: Vec<ReplyExpect>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ReplyInput {
    point: String,
    steps: Vec<Step>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ReplyExpect {
    output: String,
    from_s: f64,
    to_s: f64,
    equals: Scalar,
    tolerance: f64,
}

/// A scenario name as an identifier, made unique among `taken` (which it joins) with a suffix.
pub(crate) fn scenario_name(raw: &str, taken: &mut BTreeSet<String>) -> String {
    let base = if is_identifier(raw) {
        raw.to_owned()
    } else {
        snake(raw)
    };
    // Names must be unique for the engineer to keep or drop each one.
    let mut name = base.clone();
    let mut n = 2;
    while !taken.insert(name.clone()) {
        name = format!("{base}_{n}");
        n += 1;
    }
    name
}

/// One scenario of a reply as our vector format, named `name`; reals get a tolerance (`types`
/// are the outputs' types). Also returns a problem for each input given twice.
pub(crate) fn to_scenario(
    s: ReplyScenario,
    name: String,
    types: &BTreeMap<String, SignalType>,
) -> (Scenario, Vec<String>) {
    let mut seen = BTreeSet::new();
    let mut doubled = Vec::new();
    for input in &s.inputs {
        if !seen.insert(input.point.clone()) {
            doubled.push(format!(
                "scenario `{name}` gives input `{}` twice",
                input.point
            ));
        }
    }
    let inputs = s
        .inputs
        .into_iter()
        .map(|input| {
            let mut steps = input.steps;
            steps.sort_by(|a, b| a.t.total_cmp(&b.t));
            let spec = match steps.as_slice() {
                [only] if only.t == 0.0 => InputSpec::Constant(only.value),
                _ => InputSpec::Steps(steps),
            };
            (input.point, spec)
        })
        .collect();
    let expect = s
        .expect
        .into_iter()
        .map(|e| {
            let real = types.get(&e.output) == Some(&SignalType::Real);
            let tolerance = real.then(|| {
                if e.tolerance.is_finite() && e.tolerance > 0.0 {
                    e.tolerance
                } else {
                    1e-6
                }
            });
            Expectation {
                output: e.output,
                from_s: e.from_s,
                to_s: e.to_s,
                equals: e.equals,
                tolerance,
            }
        })
        .collect();
    let scenario = Scenario {
        name,
        description: s.description,
        inputs,
        defaults: BTreeMap::new(),
        expect,
    };
    (scenario, doubled)
}
