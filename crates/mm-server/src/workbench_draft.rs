//! The workbench's editable draft of a sequence: the DTOs the engineer reviews and edits, and
//! the conversions between them and our task and vector files.

use std::collections::BTreeMap;

use mm_agent::{ParamHint, PointSpec, TaskSpec};
use mm_bridge::{Clock, Expectation, InputSpec, Scalar, Scenario, Step, VectorFile};
use mm_graph::{ParamValue, SignalType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PointDraft {
    pub name: String,
    /// `bool`, `int` or `real`.
    #[serde(rename = "type")]
    pub r#type: String,
    pub unit: String,
    pub meaning: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ParamDraft {
    pub name: String,
    #[ts(type = "number | boolean | string | null")]
    pub value: Value,
    pub unit: String,
    pub meaning: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StepDraft {
    pub t: f64,
    #[ts(type = "number | boolean")]
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InputDraft {
    pub point: String,
    pub steps: Vec<StepDraft>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExpectDraft {
    pub output: String,
    pub from_s: f64,
    pub to_s: f64,
    #[ts(type = "number | boolean")]
    pub equals: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tolerance: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ScenarioDraft {
    pub name: String,
    pub description: String,
    /// Why the model proposed it.
    pub rationale: String,
    pub inputs: Vec<InputDraft>,
    pub expect: Vec<ExpectDraft>,
}

/// A sequence as read by the model, or as the engineer edited it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SequenceDraft {
    pub name: String,
    pub title: String,
    pub description: String,
    pub inputs: Vec<PointDraft>,
    pub outputs: Vec<PointDraft>,
    pub params: Vec<ParamDraft>,
    pub step_s: f64,
    pub horizon_s: f64,
    pub scenarios: Vec<ScenarioDraft>,
    pub open_questions: Vec<String>,
}

fn scalar(v: &Value) -> Option<Scalar> {
    match v {
        Value::Bool(b) => Some(Scalar::Bool(*b)),
        Value::Number(n) => n.as_f64().map(Scalar::Number),
        _ => None,
    }
}

fn value(s: Scalar) -> Value {
    match s {
        Scalar::Bool(b) => Value::Bool(b),
        Scalar::Number(n) => serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number),
    }
}

fn type_name(t: SignalType) -> String {
    match t {
        SignalType::Bool => "bool",
        SignalType::Int => "int",
        SignalType::Real => "real",
    }
    .into()
}

fn point_draft(p: &PointSpec) -> PointDraft {
    PointDraft {
        name: p.name.clone(),
        r#type: type_name(p.ty),
        unit: p.unit.clone(),
        meaning: p.meaning.clone(),
    }
}

/// The model's reading as a draft for the engineer.
pub fn to_draft(read: &mm_agent::ReadSequence) -> SequenceDraft {
    let scenarios = read
        .vectors
        .scenarios
        .iter()
        .enumerate()
        .map(|(i, s)| ScenarioDraft {
            name: s.name.clone(),
            description: s.description.clone(),
            rationale: read.rationale.get(i).cloned().unwrap_or_default(),
            inputs: s
                .inputs
                .iter()
                .map(|(point, spec)| InputDraft {
                    point: point.clone(),
                    steps: match spec {
                        InputSpec::Constant(v) => vec![StepDraft {
                            t: 0.0,
                            value: value(*v),
                        }],
                        InputSpec::Steps(steps) => steps
                            .iter()
                            .map(|st| StepDraft {
                                t: st.t,
                                value: value(st.value),
                            })
                            .collect(),
                    },
                })
                .collect(),
            expect: s
                .expect
                .iter()
                .map(|e| ExpectDraft {
                    output: e.output.clone(),
                    from_s: e.from_s,
                    to_s: e.to_s,
                    equals: value(e.equals),
                    tolerance: e.tolerance,
                })
                .collect(),
        })
        .collect();
    SequenceDraft {
        name: read.task.name.clone(),
        title: read.title.clone(),
        description: read.task.description.clone(),
        inputs: read.task.inputs.iter().map(point_draft).collect(),
        outputs: read.task.outputs.iter().map(point_draft).collect(),
        params: read
            .task
            .params
            .iter()
            .map(|p| ParamDraft {
                name: p.name.clone(),
                value: match &p.value {
                    ParamValue::Bool(b) => Value::Bool(*b),
                    ParamValue::Number(n) => {
                        serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number)
                    }
                    ParamValue::Text(t) => Value::String(t.clone()),
                },
                unit: p.unit.clone(),
                meaning: p.meaning.clone(),
            })
            .collect(),
        step_s: read.vectors.clock.step_s,
        horizon_s: read.vectors.clock.horizon_s,
        scenarios,
        open_questions: read.open_questions.clone(),
    }
}

/// The engineer's draft as our task and vector files; a value that is not a number or boolean
/// is a problem, reported with the rest by [`mm_agent::check_task`].
pub fn from_draft(draft: &SequenceDraft) -> Result<(TaskSpec, VectorFile), Vec<String>> {
    let mut problems = Vec::new();
    let mut point = |p: &PointDraft| {
        let ty = match p.r#type.as_str() {
            "bool" => SignalType::Bool,
            "int" => SignalType::Int,
            "real" => SignalType::Real,
            other => {
                problems.push(format!("point `{}` has unknown type `{other}`", p.name));
                SignalType::Real
            }
        };
        PointSpec {
            name: p.name.clone(),
            ty,
            unit: p.unit.clone(),
            meaning: p.meaning.clone(),
        }
    };
    let inputs: Vec<PointSpec> = draft.inputs.iter().map(&mut point).collect();
    let outputs: Vec<PointSpec> = draft.outputs.iter().map(&mut point).collect();
    let mut params = Vec::new();
    for p in &draft.params {
        let value = match &p.value {
            Value::Bool(b) => ParamValue::Bool(*b),
            Value::Number(n) => ParamValue::Number(n.as_f64().unwrap_or(f64::NAN)),
            _ => {
                problems.push(format!("parameter `{}` is not a number or boolean", p.name));
                continue;
            }
        };
        params.push(ParamHint {
            name: p.name.clone(),
            value,
            unit: p.unit.clone(),
            meaning: p.meaning.clone(),
        });
    }
    let real: BTreeMap<&str, bool> = outputs
        .iter()
        .map(|p| (p.name.as_str(), p.ty == SignalType::Real))
        .collect();
    let mut scenarios = Vec::new();
    for s in &draft.scenarios {
        let mut given = BTreeMap::new();
        for input in &s.inputs {
            if given.contains_key(&input.point) {
                problems.push(format!(
                    "scenario `{}` gives input `{}` twice",
                    s.name, input.point
                ));
            }
            let mut steps = Vec::new();
            for st in &input.steps {
                match scalar(&st.value) {
                    Some(v) => steps.push(Step { t: st.t, value: v }),
                    None => problems.push(format!(
                        "scenario `{}`: input `{}` has a value that is not a number or boolean",
                        s.name, input.point
                    )),
                }
            }
            let spec = match steps.as_slice() {
                [only] if only.t == 0.0 => InputSpec::Constant(only.value),
                _ => InputSpec::Steps(steps),
            };
            given.insert(input.point.clone(), spec);
        }
        let mut expect = Vec::new();
        for e in &s.expect {
            let Some(equals) = scalar(&e.equals) else {
                problems.push(format!(
                    "scenario `{}`: `{}` is expected with a value that is not a number or boolean",
                    s.name, e.output
                ));
                continue;
            };
            let tolerance = real
                .get(e.output.as_str())
                .copied()
                .unwrap_or(false)
                .then(|| {
                    e.tolerance
                        .filter(|t| t.is_finite() && *t > 0.0)
                        .unwrap_or(1e-6)
                });
            expect.push(Expectation {
                output: e.output.clone(),
                from_s: e.from_s,
                to_s: e.to_s,
                equals,
                tolerance,
            });
        }
        scenarios.push(Scenario {
            name: s.name.clone(),
            description: s.description.clone(),
            inputs: given,
            defaults: BTreeMap::new(),
            expect,
        });
    }
    let task = TaskSpec {
        name: draft.name.clone(),
        description: draft.description.clone(),
        inputs,
        outputs,
        params,
        logic: None,
    };
    let vectors = VectorFile {
        schema: "mechanical-muse/vectors/v1".into(),
        clock: Clock {
            step_s: draft.step_s,
            horizon_s: draft.horizon_s,
        },
        scenarios,
    };
    problems.extend(mm_agent::check_task(&task, &vectors));
    if problems.is_empty() {
        Ok((task, vectors))
    } else {
        Err(problems)
    }
}
