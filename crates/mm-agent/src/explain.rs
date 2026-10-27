//! Plain-language explanations of a verified block graph, for people. Model-written text: it is
//! shown labelled as such, never used for control, and its references to blocks are checked
//! against the graph (unknown ids are dropped and counted).

use std::collections::BTreeSet;

use mm_graph::BlockGraph;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::client::{Effort, ModelClient, ModelError, ModelRequest, OutputSchema, Usage};
use crate::prompt::TaskSpec;

/// What the model returns, after the check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    pub summary: String,
    pub parts: Vec<ExplainedPart>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplainedPart {
    pub title: String,
    pub text: String,
    /// Block ids this part covers; every one exists in the graph.
    pub blocks: Vec<String>,
}

/// An explanation with what the check removed and what the call cost.
#[derive(Debug, Clone, PartialEq)]
pub struct Explained {
    pub explanation: Explanation,
    /// Block ids the model named that the graph does not have.
    pub unknown_blocks: Vec<String>,
    pub usage: Usage,
}

const MAX_PARTS: usize = 8;
const MAX_CHARS: usize = 600;

const INSTRUCTIONS: &str = "You explain a verified building control sequence to a building operator who knows HVAC but \
not control block diagrams. You are given the sequence's description and its block graph (blocks with their class and \
parameters, and the connections between them). Reply with one JSON object that follows the explanation schema.

Rules:
1. summary: two or three sentences on what the sequence does for the building, in plain words.
2. parts: three to seven parts in the order the signals flow, from inputs to outputs. Each part has a short title, \
a plain explanation of what that group of blocks does (with parameter values and units where they matter), and \
blocks: the exact ids of the blocks it covers, copied from the graph.
3. Explain only what the graph does. Do not invent behaviour, equipment or numbers that are not in the graph or \
the description. If the graph and the description seem to disagree, say so in the part concerned.
4. Plain sentences, no code, no markdown.";

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "parts"],
        "properties": {
            "summary": {"type": "string"},
            "parts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "text", "blocks"],
                    "properties": {
                        "title": {"type": "string"},
                        "text": {"type": "string"},
                        "blocks": {"type": "array", "items": {"type": "string"}}
                    }
                }
            }
        }
    })
}

/// The request for `graph`: stable instructions, then the description and the graph.
pub fn explain_request(
    model: &str,
    title: &str,
    about: &str,
    task: &TaskSpec,
    graph: &BlockGraph,
) -> ModelRequest {
    let blocks: Vec<Value> = graph
        .blocks
        .iter()
        .map(|b| {
            json!({
                "id": b.id,
                "class": b.class,
                "params": b.params.iter().map(|p| json!({"name": p.name, "value": p.value})).collect::<Vec<_>>(),
            })
        })
        .collect();
    let message = json!({
        "sequence": title,
        "about": about,
        "description": task.description,
        "inputs": task.inputs.iter().map(|p| json!({"name": p.name, "unit": p.unit, "meaning": p.meaning})).collect::<Vec<_>>(),
        "outputs": task.outputs.iter().map(|p| json!({"name": p.name, "unit": p.unit, "meaning": p.meaning})).collect::<Vec<_>>(),
        "graph": {"blocks": blocks, "connections": graph.connections},
    });
    ModelRequest {
        model: model.to_owned(),
        instructions: INSTRUCTIONS.to_owned(),
        input: vec![json!({
            "role": "user",
            "content": [{"type": "input_text", "text": message.to_string()}],
        })],
        schema: Some(OutputSchema {
            name: "explanation".into(),
            schema: schema(),
        }),
        effort: Some(Effort::Medium),
        max_output_tokens: 8_000,
    }
}

/// Checks a reply against `graph`: unknown block ids are dropped (and returned), empty parts
/// removed, lengths capped.
pub fn check_explanation(
    text: &str,
    graph: &BlockGraph,
) -> Result<(Explanation, Vec<String>), ModelError> {
    let raw: Explanation = serde_json::from_str(text).map_err(|e| ModelError::Malformed {
        message: format!("the explanation is not the expected JSON: {e}"),
    })?;
    let known: BTreeSet<&str> = graph.blocks.iter().map(|b| b.id.as_str()).collect();
    let mut unknown = Vec::new();
    let cap = |s: &str| s.trim().chars().take(MAX_CHARS).collect::<String>();
    let parts = raw
        .parts
        .into_iter()
        .filter(|p| !p.text.trim().is_empty())
        .take(MAX_PARTS)
        .map(|p| {
            let (keep, drop): (Vec<String>, Vec<String>) = p
                .blocks
                .into_iter()
                .partition(|b| known.contains(b.as_str()));
            unknown.extend(drop);
            ExplainedPart {
                title: cap(&p.title),
                text: cap(&p.text),
                blocks: keep,
            }
        })
        .collect();
    unknown.sort();
    unknown.dedup();
    Ok((
        Explanation {
            summary: cap(&raw.summary),
            parts,
        },
        unknown,
    ))
}

/// Asks `client` to explain `graph`.
pub async fn explain<C: ModelClient>(
    client: &C,
    model: &str,
    title: &str,
    about: &str,
    task: &TaskSpec,
    graph: &BlockGraph,
) -> Result<Explained, ModelError> {
    let reply = client
        .complete(&explain_request(model, title, about, task, graph))
        .await?;
    let (explanation, unknown_blocks) = check_explanation(&reply.text, graph)?;
    Ok(Explained {
        explanation,
        unknown_blocks,
        usage: reply.usage,
    })
}

/// What happened in a run, for an operator: model-written, from the run's own events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunExplanation {
    pub summary: String,
    pub items: Vec<RunExplainedItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunExplainedItem {
    /// When it happened, as the events give it (e.g. "day 186 00:40 to 00:48").
    pub when: String,
    pub title: String,
    pub text: String,
}

const RUN_INSTRUCTIONS: &str = "You explain what happened during a supervisory control run on an emulated office \
building to a building operator. You are given the run's settings, the control sequences that ran, and the events \
the runtime logged: test faults injected, sequences that wrote no new setpoints and why, setpoints handed back to the \
building's own controllers, clamps, and the end. Reply with one JSON object that follows the run_explanation schema.

Rules:
1. summary: two or three sentences on what the run did and whether anything went wrong, in plain words.
2. items: one to six items in time order. Each has when (copied from the events), a short title, and a plain \
explanation of what happened, why, and what the safeguard did. Group repeated events into one item.
3. Explain only what the events show. Do not guess causes the events do not give, and do not claim energy or \
comfort results.
4. Plain sentences, no code, no markdown. Use the operator's words (supply air temperature, chilled water) rather than \
point identifiers, but you may name an identifier once in brackets.";

fn run_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "items"],
        "properties": {
            "summary": {"type": "string"},
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["when", "title", "text"],
                    "properties": {
                        "when": {"type": "string"},
                        "title": {"type": "string"},
                        "text": {"type": "string"}
                    }
                }
            }
        }
    })
}

/// The request for a run whose settings and events are `context` (built by the caller).
pub fn explain_run_request(model: &str, context: &Value) -> ModelRequest {
    ModelRequest {
        model: model.to_owned(),
        instructions: RUN_INSTRUCTIONS.to_owned(),
        input: vec![json!({
            "role": "user",
            "content": [{"type": "input_text", "text": context.to_string()}],
        })],
        schema: Some(OutputSchema {
            name: "run_explanation".into(),
            schema: run_schema(),
        }),
        effort: Some(Effort::Medium),
        max_output_tokens: 8_000,
    }
}

/// Checks a run explanation: empty items removed, counts and lengths capped.
pub fn check_run_explanation(text: &str) -> Result<RunExplanation, ModelError> {
    let raw: RunExplanation = serde_json::from_str(text).map_err(|e| ModelError::Malformed {
        message: format!("the explanation is not the expected JSON: {e}"),
    })?;
    let cap = |s: &str, n: usize| s.trim().chars().take(n).collect::<String>();
    Ok(RunExplanation {
        summary: cap(&raw.summary, MAX_CHARS),
        items: raw
            .items
            .into_iter()
            .filter(|i| !i.text.trim().is_empty())
            .take(MAX_PARTS)
            .map(|i| RunExplainedItem {
                when: cap(&i.when, 80),
                title: cap(&i.title, 120),
                text: cap(&i.text, MAX_CHARS),
            })
            .collect(),
    })
}

/// Asks `client` to explain a run from its `context`.
pub async fn explain_run<C: ModelClient>(
    client: &C,
    model: &str,
    context: &Value,
) -> Result<(RunExplanation, Usage), ModelError> {
    let reply = client
        .complete(&explain_run_request(model, context))
        .await?;
    Ok((check_run_explanation(&reply.text)?, reply.usage))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn graph() -> BlockGraph {
        serde_json::from_value(json!({
            "name": "g",
            "inputs": [{"name": "u", "type": "real", "unit": "1", "description": ""}],
            "outputs": [{"name": "y", "type": "real", "description": ""}],
            "blocks": [{"id": "gain", "class": "Reals.MultiplyByParameter", "params": [{"name": "k", "value": 2.0}]}],
            "connections": ["u -> gain.u", "gain.y -> y"]
        }))
        .unwrap()
    }

    #[test]
    fn unknown_blocks_are_dropped_and_reported() {
        let reply = json!({
            "summary": "Doubles the input.",
            "parts": [
                {"title": "Gain", "text": "Multiplies by 2.", "blocks": ["gain", "ghost"]},
                {"title": "Empty", "text": "  ", "blocks": ["gain"]}
            ]
        });
        let (e, unknown) = check_explanation(&reply.to_string(), &graph()).unwrap();
        assert_eq!(e.parts.len(), 1, "an empty part is removed");
        assert_eq!(e.parts[0].blocks, ["gain"]);
        assert_eq!(unknown, ["ghost"]);
    }

    #[test]
    fn a_reply_that_is_not_the_schema_is_an_error() {
        assert!(matches!(
            check_explanation("not json", &graph()),
            Err(ModelError::Malformed { .. })
        ));
    }

    #[test]
    fn the_request_carries_the_graph_and_a_strict_schema() {
        let task: TaskSpec = serde_json::from_value(json!({
            "name": "g", "description": "Doubles u.",
            "inputs": [{"name": "u", "type": "real", "unit": "1", "meaning": "in"}],
            "outputs": [{"name": "y", "type": "real", "unit": "1", "meaning": "out"}]
        }))
        .unwrap();
        let r = explain_request("muse-spark-1.3", "G", "About.", &task, &graph());
        let text = r.input[0]
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap();
        assert!(
            text.contains("\"gain\"") && text.contains("Reals.MultiplyByParameter"),
            "{text}"
        );
        assert_eq!(
            r.schema.as_ref().unwrap().schema["additionalProperties"],
            false
        );
    }

    #[test]
    fn a_run_explanation_is_capped_and_cleaned() {
        let long = "x".repeat(2000);
        let reply = json!({
            "summary": "A fault was injected and handled.",
            "items": [
                {"when": "day 186 00:40", "title": "Fault", "text": long},
                {"when": "day 186 00:50", "title": "Empty", "text": " "}
            ]
        });
        let e = check_run_explanation(&reply.to_string()).unwrap();
        assert_eq!(e.items.len(), 1);
        assert_eq!(e.items[0].text.chars().count(), MAX_CHARS);
        let r = explain_run_request("muse-spark-1.3", &json!({"events": []}));
        assert_eq!(r.schema.unwrap().name, "run_explanation");
    }
}
