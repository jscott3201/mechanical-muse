//! What a repair changed, in plain words: given two consecutive drafts' results and the exact
//! difference between their graphs (computed by the caller, not the model), Spark explains what
//! failed and how the later draft fixed it. Advisory and labelled: the verification receipt is
//! the only verdict. Numbers are quoted from the input, never computed: a reply with a number
//! the input does not give is sent back once, then refused.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::client::{Effort, ModelClient, ModelError, ModelRequest, OutputSchema, Usage};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixChange {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixExplanation {
    pub summary: String,
    pub changes: Vec<FixChange>,
}

const MAX_CHANGES: usize = 6;
const MAX_CHARS: usize = 600;

const INSTRUCTIONS: &str = "You explain to a controls engineer how one drafted control sequence (a block graph of \
elementary CDL blocks) was repaired. You are given the earlier draft's result (why the engine refused it, or which \
test scenarios it failed: the output, the instant, expected and actual values), the later draft's result, and the \
exact difference between the two graphs (blocks added, removed or changed with their parameters, and wires added or \
removed), computed by a program. Reply with one JSON object that follows the fix_explanation schema.

Rules:
1. summary: two or three sentences: what was wrong with the earlier draft and what the later one changed to fix it.
2. changes: one to six items, each a short title and a plain explanation tying a change in the difference to the \
failure it addresses. Group related block and wire changes into one item. Name blocks by their id in backticks.
3. Explain only what the difference and the results show. If a change does not clearly relate to a failure, say it \
was also changed, without inventing a reason. Never claim the later draft is correct beyond what its result says.
4. Quote numbers exactly as the input gives them (times are given in seconds and minutes; use either), with units; \
never compute new numbers. A reply with a number that is not in the input is rejected.
5. Plain sentences, no markdown beyond backticks around block ids.";

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "changes"],
        "properties": {
            "summary": {"type": "string"},
            "changes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "text"],
                    "properties": {"title": {"type": "string"}, "text": {"type": "string"}}
                }
            }
        }
    })
}

/// The request for a repair whose results and graph difference are `context`.
pub fn explain_fix_request(model: &str, context: &Value) -> ModelRequest {
    ModelRequest {
        tools: Vec::new(),
        model: model.to_owned(),
        instructions: INSTRUCTIONS.to_owned(),
        input: vec![json!({
            "role": "user",
            "content": [{"type": "input_text", "text": context.to_string()}],
        })],
        schema: Some(OutputSchema {
            name: "fix_explanation".into(),
            schema: schema(),
        }),
        effort: Some(Effort::Medium),
        max_output_tokens: 8_000,
    }
}

/// Parses and caps a reply: empty changes dropped, counts and lengths limited.
pub fn check_fix_explanation(text: &str) -> Result<FixExplanation, ModelError> {
    let raw: FixExplanation = serde_json::from_str(text).map_err(|e| ModelError::Malformed {
        message: format!("the explanation is not the expected JSON: {e}"),
    })?;
    let cap = |s: &str, n: usize| s.trim().chars().take(n).collect::<String>();
    Ok(FixExplanation {
        summary: cap(&raw.summary, MAX_CHARS),
        changes: raw
            .changes
            .into_iter()
            .filter(|c| !c.text.trim().is_empty())
            .take(MAX_CHANGES)
            .map(|c| FixChange {
                title: cap(&c.title, 120),
                text: cap(&c.text, MAX_CHARS),
            })
            .collect(),
    })
}

fn invented(e: &FixExplanation, context: &Value) -> Vec<String> {
    let mut texts = vec![e.summary.as_str()];
    for c in &e.changes {
        texts.extend([c.title.as_str(), c.text.as_str()]);
    }
    crate::numbers::unsupported_numbers(&texts, context)
}

/// Asks `client` to explain a repair from its `context`, with one retry naming any number the
/// input does not give.
pub async fn explain_fix<C: ModelClient>(
    client: &C,
    model: &str,
    context: &Value,
) -> Result<(FixExplanation, Usage), ModelError> {
    let mut request = explain_fix_request(model, context);
    let reply = client.complete(&request).await?;
    let mut usage = reply.usage;
    let explanation = check_fix_explanation(&reply.text)?;
    let bad = invented(&explanation, context);
    if bad.is_empty() {
        return Ok((explanation, usage));
    }
    request.input.extend(reply.output_items);
    request.input.push(json!({"role": "user", "content": [{"type": "input_text", "text": format!(
        "These numbers in your reply are not in the input: {}. Rewrite the explanation using only \
         numbers the input gives; do not compute sums, differences or conversions of your own.",
        bad.join(", ")
    )}]}));
    let reply = client.complete(&request).await?;
    usage.add(reply.usage);
    let explanation = check_fix_explanation(&reply.text)?;
    let bad = invented(&explanation, context);
    if !bad.is_empty() {
        return Err(ModelError::Malformed {
            message: format!(
                "the explanation states numbers that are not in the input: {}",
                bad.join(", ")
            ),
        });
    }
    Ok((explanation, usage))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn replies_are_capped_and_empty_changes_dropped() {
        let long = "x".repeat(2000);
        let text = json!({"summary": long, "changes": [
            {"title": "a", "text": ""}, {"title": "b", "text": "kept"}
        ]})
        .to_string();
        let e = check_fix_explanation(&text).unwrap();
        assert_eq!(e.summary.chars().count(), MAX_CHARS);
        assert_eq!(e.changes.len(), 1);
        assert!(check_fix_explanation("not json").is_err());
    }

    #[test]
    fn the_request_is_structured_and_carries_the_context() {
        let r = explain_fix_request("m", &json!({"diff": {"blocks_added": []}}));
        assert_eq!(r.schema.as_ref().unwrap().name, "fix_explanation");
        assert!(r.input[0].to_string().contains("blocks_added"));
        assert!(r.tools.is_empty());
    }

    #[test]
    fn an_invented_number_is_found() {
        let e = FixExplanation {
            summary: "The hold of 600 s was missing; now 10 minutes.".into(),
            changes: vec![],
        };
        assert_eq!(invented(&e, &json!({"t_s": 600})), vec!["10"]);
        assert!(invented(&e, &json!({"t_s": 600, "t_min": 10})).is_empty());
    }
}
