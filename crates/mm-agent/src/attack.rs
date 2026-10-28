//! "Try to break it": Muse Spark proposes new test scenarios for an engineer's accepted
//! sequence, aimed at the edges its accepted scenarios miss.
//!
//! Spark sees only the written sequence (task, parameters, clock), the accepted scenarios and the
//! open questions, never a drafted graph, so its expectations come from the sequence and not
//! from what one draft happens to do. The proposals are model output: a failing one means the
//! draft or the proposal is wrong, and the engineer decides which. They join the verification
//! gate only when the engineer accepts them as a new revision of the sequence.

use std::collections::{BTreeMap, BTreeSet};

use mm_bridge::{InputSpec, Scenario, VectorFile};
use mm_graph::SignalType;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::client::{Effort, ModelClient, ModelError, ModelRequest, OutputSchema, Usage};
use crate::prompt::TaskSpec;
use crate::scenario_reply::{ReplyScenario, scenario_name, scenario_schema, to_scenario};
use crate::task_check::check_task;

/// Most proposals kept from one reply.
pub const MAX_PROPOSALS: usize = 5;

const INSTRUCTIONS: &str = "You review the test scenarios of a control sequence for building HVAC equipment, \
looking for what they miss. You get the sequence as the engineer accepted it (description, inputs, outputs, \
parameters, clock), the scenarios already accepted, and the questions the document left open. You do not see any \
implementation. Propose new scenarios that a wrong implementation could fail while still passing every accepted \
scenario. Reply with one JSON object that follows the attack_proposals schema.

Rules:
1. Propose three to five scenarios. Each one probes one edge the accepted scenarios do not: a value exactly at or \
one step either side of a threshold, the last tick before a delay or hold expires and the first after, an input at \
its limit or beyond its usual range, two inputs changing on the same tick, recovery after an interruption, or a \
point the open questions leave ambiguous.
2. Expectations come only from the written sequence. Check an output only where the sequence settles the answer. \
If an open question leaves an instant unsettled, check either side of it, not the instant itself.
3. Every scenario gives every input, either as one constant step at t = 0 or as steps in time order starting at \
t = 0. Times are multiples of clock.step_s and no later than clock.horizon_s. Use only the listed input and output \
names, with values of their types. Booleans and integers are exact; reals need a tolerance (use 1e-6 unless the \
value is approximate). Compute every expected real carefully from the parameters.
4. Names are new snake_case identifiers, different from the accepted scenarios' names.
5. rationale: one short line naming the edge the scenario probes and why the accepted scenarios miss it.
6. Plain text in every field: no markdown, no code.";

/// One proposed scenario that passed the checks, with Spark's reason for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub scenario: Scenario,
    pub rationale: String,
}

/// Spark's proposals after the checks: the ones that can run, and why others were dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct Attack {
    pub proposals: Vec<Proposal>,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttackOutcome {
    pub attack: Attack,
    pub usage: Usage,
}

fn schema() -> Value {
    let mut item = scenario_schema();
    if let Some(props) = item.get_mut("properties").and_then(Value::as_object_mut) {
        props.insert("rationale".into(), json!({"type": "string"}));
    }
    if let Some(required) = item.get_mut("required").and_then(Value::as_array_mut) {
        required.push(json!("rationale"));
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["proposals"],
        "properties": {"proposals": {"type": "array", "items": item}}
    })
}

#[derive(Debug, Clone, Deserialize)]
struct Reply {
    proposals: Vec<ReplyProposal>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReplyProposal {
    #[serde(flatten)]
    scenario: ReplyScenario,
    rationale: String,
}

/// A scenario in the shape the schema uses (steps from t = 0), for the prompt.
fn scenario_json(s: &Scenario) -> Value {
    let inputs: Vec<Value> = s
        .inputs
        .iter()
        .map(|(point, spec)| {
            let steps = match spec {
                InputSpec::Constant(v) => json!([{"t": 0.0, "value": v}]),
                InputSpec::Steps(steps) => json!(steps),
            };
            json!({"point": point, "steps": steps})
        })
        .collect();
    let expect: Vec<Value> = s
        .expect
        .iter()
        .map(|e| {
            json!({"output": e.output, "from_s": e.from_s, "to_s": e.to_s, "equals": e.equals,
                   "tolerance": e.tolerance.unwrap_or(0.0)})
        })
        .collect();
    json!({"name": s.name, "description": s.description, "inputs": inputs, "expect": expect})
}

/// The request for `task`. The sequence's name is left out, so the same sequence accepted under
/// another name makes the same request (and finds the same recorded reply).
pub fn attack_request(
    model: &str,
    title: &str,
    task: &TaskSpec,
    vectors: &VectorFile,
    open_questions: &[String],
) -> ModelRequest {
    let sequence = json!({
        "title": title,
        "description": task.description,
        "inputs": task.inputs,
        "outputs": task.outputs,
        "params": task.params,
        "clock": {"step_s": vectors.clock.step_s, "horizon_s": vectors.clock.horizon_s},
        "open_questions": open_questions,
        "accepted_scenarios": vectors.scenarios.iter().map(scenario_json).collect::<Vec<_>>(),
    });
    let text = format!(
        "The accepted sequence and its scenarios:\n\n{}",
        serde_json::to_string_pretty(&sequence).unwrap_or_default()
    );
    ModelRequest {
        tools: Vec::new(),
        model: model.to_owned(),
        instructions: INSTRUCTIONS.to_owned(),
        input: vec![json!({"role": "user", "content": [{"type": "input_text", "text": text}]})],
        schema: Some(OutputSchema {
            name: "attack_proposals".into(),
            schema: schema(),
        }),
        effort: Some(Effort::Medium),
        max_output_tokens: 16_000,
    }
}

/// FNV-1a 64 over everything `request` sends (`attack:<hex>`): the key of a recorded reply.
pub fn attack_key(request: &ModelRequest) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    };
    feed(request.model.as_bytes());
    feed(request.instructions.as_bytes());
    for item in &request.input {
        feed(item.to_string().as_bytes());
    }
    if let Some(schema) = &request.schema {
        feed(schema.name.as_bytes());
        feed(schema.schema.to_string().as_bytes());
    }
    format!("attack:{hash:016x}")
}

/// Parses a reply and checks each proposal on its own against `task` and the clock of
/// `vectors`. A proposal that cannot run fairly is dropped with its problems; names that
/// collide with accepted scenarios or each other get a suffix.
pub fn check_attack(
    text: &str,
    task: &TaskSpec,
    vectors: &VectorFile,
) -> Result<Attack, ModelError> {
    let reply: Reply = serde_json::from_str(text).map_err(|e| ModelError::Malformed {
        message: format!("the proposals are not the expected JSON: {e}"),
    })?;
    let types: BTreeMap<String, SignalType> = task
        .outputs
        .iter()
        .map(|p| (p.name.clone(), p.ty))
        .collect();
    let mut taken: BTreeSet<String> = vectors.scenarios.iter().map(|s| s.name.clone()).collect();
    let mut proposals = Vec::new();
    let mut problems = Vec::new();
    for p in reply.proposals {
        if proposals.len() >= MAX_PROPOSALS {
            problems.push(format!(
                "only {MAX_PROPOSALS} proposals are kept; `{}` was dropped",
                p.scenario.name
            ));
            continue;
        }
        let name = scenario_name(&p.scenario.name, &mut taken);
        let (scenario, mut found) = to_scenario(p.scenario, name.clone(), &types);
        let alone = VectorFile {
            schema: vectors.schema.clone(),
            clock: vectors.clock,
            scenarios: vec![scenario.clone()],
        };
        found.extend(check_task(task, &alone));
        if found.is_empty() {
            proposals.push(Proposal {
                scenario,
                rationale: p.rationale.trim().to_owned(),
            });
        } else {
            problems.push(format!("dropped `{name}`: {}", found.join("; ")));
        }
    }
    if proposals.is_empty() && problems.is_empty() {
        problems.push("Muse Spark proposed no scenarios".into());
    }
    Ok(Attack {
        proposals,
        problems,
    })
}

/// One call: Spark proposes, the checks keep what can run.
pub async fn attack<C: ModelClient>(
    client: &C,
    request: &ModelRequest,
    task: &TaskSpec,
    vectors: &VectorFile,
) -> Result<AttackOutcome, ModelError> {
    let reply = client.complete(request).await?;
    let attack = check_attack(&reply.text, task, vectors)?;
    Ok(AttackOutcome {
        attack,
        usage: reply.usage,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::time::Duration;

    use mm_bridge::{Clock, Expectation, Scalar};

    use super::*;
    use crate::prompt::PointSpec;
    use crate::recorded::{RecordedClient, RecordedReply, Recording};

    fn point(name: &str, ty: SignalType) -> PointSpec {
        PointSpec {
            name: name.into(),
            ty,
            unit: String::new(),
            meaning: String::new(),
        }
    }

    fn task() -> TaskSpec {
        TaskSpec {
            name: "fan".into(),
            description: "Run the fan above 300 K.".into(),
            inputs: vec![point("TRoo", SignalType::Real)],
            outputs: vec![point("y1Fan", SignalType::Bool)],
            params: vec![],
            logic: None,
        }
    }

    fn vectors() -> VectorFile {
        VectorFile {
            schema: "mechanical-muse/vectors/v1".into(),
            clock: Clock {
                step_s: 60.0,
                horizon_s: 600.0,
            },
            scenarios: vec![Scenario {
                name: "hot".into(),
                description: String::new(),
                inputs: [(
                    "TRoo".to_owned(),
                    InputSpec::Constant(Scalar::Number(305.0)),
                )]
                .into(),
                defaults: BTreeMap::new(),
                expect: vec![Expectation {
                    output: "y1Fan".into(),
                    from_s: 0.0,
                    to_s: 600.0,
                    equals: Scalar::Bool(true),
                    tolerance: None,
                }],
            }],
        }
    }

    fn proposal(name: &str, point: &str, t: f64, equals: Value) -> Value {
        json!({"name": name, "description": "", "rationale": format!("edge {name}"),
               "inputs": [{"point": point, "steps": [{"t": 0, "value": 300.0}, {"t": t, "value": 301.0}]}],
               "expect": [{"output": "y1Fan", "from_s": 0, "to_s": 60, "equals": equals, "tolerance": 0}]})
    }

    #[test]
    fn good_proposals_are_kept_and_bad_ones_dropped_with_a_reason() {
        let reply = json!({"proposals": [
            proposal("at threshold", "TRoo", 120.0, json!(false)),
            proposal("hot", "TRoo", 120.0, json!(false)),
            proposal("off_grid", "TRoo", 90.0, json!(false)),
            proposal("unknown_point", "TOut", 120.0, json!(false)),
            proposal("wrong_type", "TRoo", 120.0, json!(1)),
        ]});
        let a = check_attack(&reply.to_string(), &task(), &vectors()).unwrap();
        let names: Vec<&str> = a
            .proposals
            .iter()
            .map(|p| p.scenario.name.as_str())
            .collect();
        assert_eq!(names, ["at_threshold", "hot_2"], "renamed, never clashing");
        assert_eq!(a.proposals[0].rationale, "edge at threshold");
        let all = a.problems.join("\n");
        for needle in [
            "dropped `off_grid`",
            "off the clock",
            "dropped `unknown_point`",
            "not an input",
            "dropped `wrong_type`",
            "wrong type",
        ] {
            assert!(all.contains(needle), "missing `{needle}` in:\n{all}");
        }
    }

    #[test]
    fn at_most_five_are_kept() {
        let many: Vec<Value> = (0..7)
            .map(|i| proposal(&format!("p{i}"), "TRoo", 120.0, json!(false)))
            .collect();
        let a = check_attack(&json!({"proposals": many}).to_string(), &task(), &vectors()).unwrap();
        assert_eq!(a.proposals.len(), MAX_PROPOSALS);
        assert_eq!(a.problems.len(), 2);
    }

    #[test]
    fn a_reply_that_is_not_the_schema_is_malformed() {
        assert!(matches!(
            check_attack("{\"nope\": 1}", &task(), &vectors()),
            Err(ModelError::Malformed { .. })
        ));
    }

    #[test]
    fn the_request_shows_the_sequence_but_not_its_name_and_keys_on_its_content() {
        let r = attack_request(
            "muse-spark-1.3",
            "Fan",
            &task(),
            &vectors(),
            &["Strict?".into()],
        );
        let text = r.input[0]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("accepted_scenarios") && text.contains("Strict?"));
        assert!(text.contains("\"hot\""));
        assert!(!text.contains("\"fan\""), "the name is not sent");
        assert_eq!(r.effort, Some(Effort::Medium));
        let schema = &r.schema.as_ref().unwrap().schema;
        let item = &schema["properties"]["proposals"]["items"];
        assert_eq!(item["additionalProperties"], false);
        assert!(
            item["required"]
                .as_array()
                .unwrap()
                .contains(&json!("rationale"))
        );
        let mut renamed = task();
        renamed.name = "fan_r2".into();
        let again = attack_request(
            "muse-spark-1.3",
            "Fan",
            &renamed,
            &vectors(),
            &["Strict?".into()],
        );
        assert_eq!(attack_key(&r), attack_key(&again));
        let other = attack_request("muse-spark-1.3", "Fan", &task(), &vectors(), &[]);
        assert_ne!(attack_key(&r), attack_key(&other));
        assert!(attack_key(&r).starts_with("attack:"));
    }

    #[tokio::test]
    async fn one_call_through_the_model_client() {
        let text =
            json!({"proposals": [proposal("edge", "TRoo", 120.0, json!(false))]}).to_string();
        let client = RecordedClient::new(
            Recording {
                model: "muse-spark-1.3".into(),
                task: "fan".into(),
                note: String::new(),
                task_digest: None,
                replies: vec![RecordedReply::Reply {
                    text,
                    usage: Usage::default(),
                }],
            },
            Duration::ZERO,
        );
        let request = attack_request("muse-spark-1.3", "Fan", &task(), &vectors(), &[]);
        let out = attack(&client, &request, &task(), &vectors())
            .await
            .unwrap();
        assert_eq!(out.attack.proposals.len(), 1);
        assert!(out.attack.problems.is_empty());
    }
}
