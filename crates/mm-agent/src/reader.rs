//! Reading an engineer's own sequence of operations (text, a PDF or a photo) into our task format
//! and a set of proposed test scenarios.
//!
//! Everything here is model output and only a proposal: the scenarios become the verification
//! gate only after an engineer has reviewed and accepted them (in `mm-server` and the web app).
//! The checks below catch scenarios that cannot run (unknown points, missing inputs, times off
//! the clock, wrong value types); they cannot tell whether an expected value is right.

use std::collections::BTreeMap;

use mm_bridge::{Clock, Expectation, InputSpec, Scalar, Scenario, Step, VectorFile};
use mm_graph::{ParamValue, SignalType};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::client::{Effort, ModelClient, ModelError, ModelRequest, OutputSchema, Usage};
use crate::prompt::{ParamHint, PointSpec, TaskSpec};
use crate::task_check::{check_task, is_identifier, snake};

/// A document the engineer attached.
#[derive(Debug, Clone, PartialEq)]
pub enum Attachment {
    /// A PDF, base64 without the data-URL prefix.
    Pdf { filename: String, base64: String },
    /// A png, jpeg, gif or webp image, base64 without the data-URL prefix.
    Image { mime: String, base64: String },
}

/// What the engineer gave us.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SequenceSource {
    pub text: String,
    pub attachments: Vec<Attachment>,
}

/// The model's reading of the sequence, after the checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadSequence {
    pub title: String,
    pub task: TaskSpec,
    pub vectors: VectorFile,
    /// One line per proposed scenario: why it is there (same order as `vectors.scenarios`).
    pub rationale: Vec<String>,
    /// Points the document leaves unsettled, for the engineer to answer.
    pub open_questions: Vec<String>,
    /// Problems the checks found and the model could not fix; the engineer must fix or drop
    /// the scenarios concerned.
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadOutcome {
    pub read: ReadSequence,
    /// Model calls made (1, or 2 when the first reply had problems).
    pub calls: u32,
    pub usage: Usage,
}

const INSTRUCTIONS: &str = "You help a controls engineer turn a written sequence of operations for building HVAC \
equipment into a precise task and a set of test scenarios. A separate step will build the control logic as a block \
graph and check it against the scenarios you propose, after the engineer has reviewed them. Reply with one JSON \
object that follows the sequence_reading schema.

Rules:
1. Read only what the document says. Write the description in your own words, precisely enough that someone could \
build the logic from it alone: every threshold, delay, hysteresis band, limit, rate, and what happens on start-up \
and when an input is lost. Do not copy long passages.
2. Interface: inputs are what the logic reads (sensors, statuses, enables, setpoints from elsewhere); outputs are what \
it commands or reports. Names are short lowerCamel identifiers (letters and digits, starting with a letter). Types: \
bool, int or real. Use SI units for reals (K for temperatures, Pa for pressures, s for times, 1 for fractions) and \
convert any values the document gives in other units.
3. params: every number the logic needs (thresholds, delays, gains, limits), with the document's value converted to \
SI. If the document leaves a value out, choose a typical one and add an open question saying so.
4. open_questions: anything ambiguous or missing that changes behaviour (for example whether a comparison is strict, \
what happens at an exact threshold, or what the output is before the first update). Ask; do not silently decide.
5. Scenarios: four to eight, each testing one behaviour: steady normal operation, each transition, each delay \
(not yet true just before it expires, true after), each limit, behaviour at start-up, and recovery. Every scenario \
gives every input, either as one constant step at t = 0 or as steps in time order starting at t = 0. Times are \
multiples of clock.step_s and no later than clock.horizon_s.
6. Expectations: check outputs only where the document settles the answer. Prefer short windows that avoid the exact \
instant of a change unless the document says what happens at that instant. Booleans and integers are exact; reals \
need a tolerance (use 1e-6 unless the value is approximate). Compute every expected real carefully from the params. \
If you cannot settle an output from the document, leave it out and add an open question.
7. rationale: one short line per scenario, in the same order, saying what behaviour it pins down.
8. Plain text in every field: no markdown, no code.";

fn scalar_schema() -> Value {
    json!({"anyOf": [{"type": "number"}, {"type": "boolean"}]})
}

fn point_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "type", "unit", "meaning"],
        "properties": {
            "name": {"type": "string"},
            "type": {"type": "string", "enum": ["bool", "int", "real"]},
            "unit": {"type": "string"},
            "meaning": {"type": "string"}
        }
    })
}

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "title", "description", "inputs", "outputs", "params", "open_questions", "clock", "scenarios", "rationale"],
        "properties": {
            "name": {"type": "string"},
            "title": {"type": "string"},
            "description": {"type": "string"},
            "inputs": {"type": "array", "items": point_schema()},
            "outputs": {"type": "array", "items": point_schema()},
            "params": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["name", "value", "unit", "meaning"],
                    "properties": {
                        "name": {"type": "string"},
                        "value": scalar_schema(),
                        "unit": {"type": "string"},
                        "meaning": {"type": "string"}
                    }
                }
            },
            "open_questions": {"type": "array", "items": {"type": "string"}},
            "clock": {
                "type": "object",
                "additionalProperties": false,
                "required": ["step_s", "horizon_s"],
                "properties": {"step_s": {"type": "number"}, "horizon_s": {"type": "number"}}
            },
            "scenarios": {
                "type": "array",
                "items": {
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
                }
            },
            "rationale": {"type": "array", "items": {"type": "string"}}
        }
    })
}

/// The reply as the schema shapes it.
#[derive(Debug, Clone, Deserialize)]
struct Reply {
    name: String,
    title: String,
    description: String,
    inputs: Vec<PointSpec>,
    outputs: Vec<PointSpec>,
    params: Vec<ReplyParam>,
    open_questions: Vec<String>,
    clock: Clock,
    scenarios: Vec<ReplyScenario>,
    rationale: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReplyParam {
    name: String,
    value: Scalar,
    unit: String,
    meaning: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReplyScenario {
    name: String,
    description: String,
    inputs: Vec<ReplyInput>,
    expect: Vec<ReplyExpect>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReplyInput {
    point: String,
    steps: Vec<Step>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReplyExpect {
    output: String,
    from_s: f64,
    to_s: f64,
    equals: Scalar,
    tolerance: f64,
}

/// Turns the reply into our task and vector files, then checks them. Scenarios with problems are
/// kept (the engineer decides) but every problem is listed.
fn check(reply: Reply) -> ReadSequence {
    let types: BTreeMap<String, SignalType> = reply
        .outputs
        .iter()
        .map(|p| (p.name.clone(), p.ty))
        .collect();
    let mut taken = std::collections::BTreeSet::new();
    let mut doubled = Vec::new();
    let scenarios = reply
        .scenarios
        .into_iter()
        .map(|s| {
            let base = if is_identifier(&s.name) {
                s.name
            } else {
                snake(&s.name)
            };
            // Names must be unique for the engineer to keep or drop each one.
            let mut name = base.clone();
            let mut n = 2;
            while !taken.insert(name.clone()) {
                name = format!("{base}_{n}");
                n += 1;
            }
            let mut seen = std::collections::BTreeSet::new();
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
            Scenario {
                name,
                description: s.description,
                inputs,
                defaults: BTreeMap::new(),
                expect,
            }
        })
        .collect::<Vec<_>>();
    let mut rationale = reply.rationale;
    rationale.resize(scenarios.len(), String::new());
    let task = TaskSpec {
        name: snake(&reply.name),
        description: reply.description,
        inputs: reply.inputs,
        outputs: reply.outputs,
        params: reply
            .params
            .into_iter()
            .map(|p| ParamHint {
                name: p.name,
                value: match p.value {
                    Scalar::Bool(b) => ParamValue::Bool(b),
                    Scalar::Number(n) => ParamValue::Number(n),
                },
                unit: p.unit,
                meaning: p.meaning,
            })
            .collect(),
        logic: None,
    };
    let vectors = VectorFile {
        schema: "mechanical-muse/vectors/v1".into(),
        clock: reply.clock,
        scenarios,
    };
    let mut problems = doubled;
    problems.extend(check_task(&task, &vectors));
    ReadSequence {
        title: reply.title.trim().chars().take(120).collect(),
        task,
        vectors,
        rationale,
        open_questions: reply.open_questions,
        problems,
    }
}

/// Parses and checks a reply's text.
pub fn check_reading(text: &str) -> Result<ReadSequence, ModelError> {
    let reply: Reply = serde_json::from_str(text).map_err(|e| ModelError::Malformed {
        message: format!("the reading is not the expected JSON: {e}"),
    })?;
    Ok(check(reply))
}

fn user_content(source: &SequenceSource) -> Vec<Value> {
    let mut content = Vec::new();
    for a in &source.attachments {
        content.push(match a {
            Attachment::Pdf { filename, base64 } => json!({
                "type": "input_file",
                "filename": filename,
                "file_data": format!("data:application/pdf;base64,{base64}"),
            }),
            Attachment::Image { mime, base64 } => json!({
                "type": "input_image",
                "image_url": format!("data:{mime};base64,{base64}"),
            }),
        });
    }
    let text = if source.text.trim().is_empty() {
        "Read the attached sequence of operations.".to_owned()
    } else {
        format!(
            "Sequence of operations from the engineer:\n\n{}",
            source.text.trim()
        )
    };
    content.push(json!({"type": "input_text", "text": text}));
    content
}

/// The first request for `source`.
pub fn read_request(model: &str, source: &SequenceSource) -> ModelRequest {
    ModelRequest {
        model: model.to_owned(),
        instructions: INSTRUCTIONS.to_owned(),
        input: vec![json!({"role": "user", "content": user_content(source)})],
        schema: Some(OutputSchema {
            name: "sequence_reading".into(),
            schema: schema(),
        }),
        effort: Some(Effort::High),
        max_output_tokens: 32_000,
    }
}

fn add_usage(total: &mut Usage, more: &Usage) {
    total.input_tokens += more.input_tokens;
    total.output_tokens += more.output_tokens;
}

/// Reads `source`; when the checks find problems, asks once more with the problems listed.
pub async fn read_sequence<C: ModelClient>(
    client: &C,
    model: &str,
    source: &SequenceSource,
) -> Result<ReadOutcome, ModelError> {
    let mut request = read_request(model, source);
    let first = client.complete(&request).await?;
    let mut usage = first.usage;
    let read = check_reading(&first.text)?;
    if read.problems.is_empty() {
        return Ok(ReadOutcome {
            read,
            calls: 1,
            usage,
        });
    }
    request.input.push(
        json!({"role": "assistant", "content": [{"type": "output_text", "text": first.text}]}),
    );
    request.input.push(json!({
        "role": "user",
        "content": [{"type": "input_text", "text": format!(
            "These problems make some scenarios impossible to run. Reply with the whole reading again, fixed:\n- {}",
            read.problems.join("\n- ")
        )}],
    }));
    match client.complete(&request).await {
        Ok(second) => {
            add_usage(&mut usage, &second.usage);
            match check_reading(&second.text) {
                Ok(fixed) => Ok(ReadOutcome {
                    read: fixed,
                    calls: 2,
                    usage,
                }),
                Err(_) => Ok(ReadOutcome {
                    read,
                    calls: 2,
                    usage,
                }),
            }
        }
        // The first reading is still useful: the engineer sees its problems.
        Err(_) => Ok(ReadOutcome {
            read,
            calls: 2,
            usage,
        }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn reply() -> Value {
        json!({
            "name": "Exhaust fan control",
            "title": "Exhaust fan on high temperature",
            "description": "Run the fan when the room is hot.",
            "inputs": [{"name": "TRoo", "type": "real", "unit": "K", "meaning": "Room temperature"},
                       {"name": "u1Ena", "type": "bool", "unit": "bool", "meaning": "Enabled"}],
            "outputs": [{"name": "y1Fan", "type": "bool", "unit": "bool", "meaning": "Fan command"}],
            "params": [{"name": "TOn", "value": 300.15, "unit": "K", "meaning": "On above"}],
            "open_questions": ["Is the comparison strict?"],
            "clock": {"step_s": 60, "horizon_s": 1200},
            "scenarios": [
                {"name": "hot", "description": "Hot room", "inputs": [
                    {"point": "TRoo", "steps": [{"t": 0, "value": 295.15}, {"t": 600, "value": 303.15}]},
                    {"point": "u1Ena", "steps": [{"t": 0, "value": true}]}],
                 "expect": [{"output": "y1Fan", "from_s": 0, "to_s": 540, "equals": false, "tolerance": 0},
                            {"output": "y1Fan", "from_s": 660, "to_s": 1200, "equals": true, "tolerance": 0}]},
                {"name": "bad one", "description": "", "inputs": [
                    {"point": "TRoo", "steps": [{"t": 30, "value": true}]},
                    {"point": "nope", "steps": [{"t": 0, "value": 1}]}],
                 "expect": [{"output": "TRoo", "from_s": 0, "to_s": 60, "equals": 1, "tolerance": 0}]}
            ],
            "rationale": ["turns on when hot"]
        })
    }

    #[test]
    fn a_reading_becomes_a_task_and_vectors_with_its_problems_listed() {
        let r = check_reading(&reply().to_string()).unwrap();
        assert_eq!(r.task.name, "exhaust_fan_control");
        assert_eq!(
            snake("ahu1SupplyAirTempSetpoint"),
            "ahu1_supply_air_temp_setpoint"
        );
        assert_eq!(snake("AHU-1 SAT"), "ahu_1_sat");
        assert_eq!(r.vectors.scenarios.len(), 2);
        let hot = &r.vectors.scenarios[0];
        assert!(matches!(
            hot.inputs["u1Ena"],
            InputSpec::Constant(Scalar::Bool(true))
        ));
        assert!(matches!(hot.inputs["TRoo"], InputSpec::Steps(ref s) if s.len() == 2));
        assert_eq!(hot.expect[0].tolerance, None, "booleans are exact");
        assert_eq!(r.vectors.scenarios[1].name, "bad_one");
        let all = r.problems.join("\n");
        for needle in [
            "must start at t = 0",
            "wrong type",
            "off the clock",
            "not an input",
            "does not give input `u1Ena`",
            "not an output",
            "checks nothing",
        ] {
            assert!(all.contains(needle), "missing `{needle}` in:\n{all}");
        }
        assert!(
            !all.contains("`hot`"),
            "the good scenario has no problems:\n{all}"
        );
        assert_eq!(r.rationale.len(), 2);
    }

    #[test]
    fn attachments_go_before_the_text() {
        let source = SequenceSource {
            text: "see page 2".into(),
            attachments: vec![Attachment::Pdf {
                filename: "soo.pdf".into(),
                base64: "AAAA".into(),
            }],
        };
        let r = read_request("muse-spark-1.3", &source);
        let content = r.input[0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "input_file");
        assert!(
            content[0]["file_data"]
                .as_str()
                .unwrap()
                .starts_with("data:application/pdf;base64,")
        );
        assert_eq!(content[1]["type"], "input_text");
    }
}
