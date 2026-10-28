//! Live spike: does the Responses endpoint take function tools beside a strict `text.format`,
//! does the stateless function_call / function_call_output round trip work with our client,
//! and does Muse Spark call our checks when offered? Run with `make live-tools`
//! (`MM_TOOLS_SPEC` picks a spec folder, default chw_plant_reset; `MM_TOOLS_MODE=split` sends
//! tool turns without `text.format` and the schema only on a final call without tools).
//! Prints statuses, item types and usage only; never the key, ids or encrypted content.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::time::Instant;

use mm_agent::{
    AttemptOutcome, DraftTools, Effort, ModelClient, ModelRequest, OutputSchema, SparkClient,
    TaskSpec, check_reply, function_calls, system_instructions, task_message,
};
use mm_bridge::VectorFile;
use mm_graph::{BLOCK_GRAPH_SCHEMA, Palette};
use serde_json::{Value, json};

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn item_types(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .map(|i| i["type"].as_str().unwrap_or("?").to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "calls the Meta Model API; needs MODEL_API_KEY"]
async fn live_spark_drafts_with_tools() {
    let key = std::env::var("MODEL_API_KEY").expect("set MODEL_API_KEY");
    let client = SparkClient::new(key).unwrap();
    let spec = std::env::var("MM_TOOLS_SPEC").unwrap_or_else(|_| "chw_plant_reset".into());
    let split = std::env::var("MM_TOOLS_MODE").is_ok_and(|m| m == "split");
    let task: TaskSpec =
        serde_json::from_slice(&std::fs::read(repo(&format!("specs/{spec}/task.json"))).unwrap())
            .unwrap();
    let vectors: VectorFile = serde_json::from_slice(
        &std::fs::read(repo(&format!("specs/{spec}/vectors.json"))).unwrap(),
    )
    .unwrap();
    let palette = Palette::pinned().unwrap();
    let tools = DraftTools::new(&palette, &task, &vectors);
    let schema = OutputSchema {
        name: "block_graph".into(),
        schema: serde_json::from_str(BLOCK_GRAPH_SCHEMA).unwrap(),
    };
    let mut request = ModelRequest {
        model: "muse-spark-1.3".into(),
        instructions: format!(
            "{}\n\nThe block classes above are complete; call describe_block only for a \
             class you are unsure of. Before you answer, check your draft with the tools: \
             check_graph for structure, replay_scenario for behavior. Fix what they report, \
             then reply with the final block graph.",
            system_instructions(&palette, &[])
        ),
        input: vec![json!({"role": "user", "content": [{"type": "input_text",
            "text": task_message(&task)}]})],
        schema: (!split).then(|| schema.clone()),
        tools: tools.definitions(),
        effort: Some(Effort::High),
        max_output_tokens: 32_000,
    };
    eprintln!(
        "spec {spec}, mode {}",
        if split { "split" } else { "combined" }
    );
    let started = Instant::now();
    let mut final_text = None;
    for turn in 1..=8 {
        let t0 = Instant::now();
        let reply = match client.complete(&request).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("turn {turn}: ERROR {e}");
                return;
            }
        };
        let calls = function_calls(&reply.output_items);
        for item in &reply.output_items {
            if item["type"] == "message" {
                eprintln!(
                    "  message phase {:?}, {} chars",
                    item["phase"].as_str(),
                    item["content"][0]["text"].as_str().map_or(0, str::len)
                );
            }
        }
        eprintln!(
            "turn {turn}: {:.1}s items {:?} usage {:?}",
            t0.elapsed().as_secs_f64(),
            item_types(&reply.output_items),
            reply.usage
        );
        request.input.extend(reply.output_items.clone());
        if calls.is_empty() {
            final_text = Some(reply.text);
            break;
        }
        for call in calls {
            let result = tools.call(&call.name, &call.arguments);
            eprintln!(
                "  call {} ({} bytes of args): {}",
                call.name,
                call.arguments.len(),
                result.summary
            );
            request.input.push(json!({"type": "function_call_output",
                "call_id": call.call_id, "output": result.output}));
        }
    }
    let text = match (final_text, split) {
        (Some(text), false) => text,
        (text, true) => {
            if let Some(t) = &text {
                eprintln!("split: tool phase ended with {} chars of text", t.len());
                request
                    .input
                    .push(json!({"role": "user", "content": [{"type": "input_text",
                    "text": "Now reply with the final block graph only."}]}));
            }
            request.tools.clear();
            request.schema = Some(schema);
            match client.complete(&request).await {
                Ok(r) => {
                    eprintln!(
                        "final (no tools, schema): items {:?} usage {:?}",
                        item_types(&r.output_items),
                        r.usage
                    );
                    r.text
                }
                Err(e) => {
                    eprintln!("final: ERROR {e}");
                    return;
                }
            }
        }
        (None, false) => {
            // Out of turns: the same history, no tools, the schema kept.
            eprintln!("no final answer within 8 turns; final call without tools");
            request.tools.clear();
            request
                .input
                .push(json!({"role": "user", "content": [{"type": "input_text",
                "text": "No more tool calls. Reply with the final block graph."}]}));
            match client.complete(&request).await {
                Ok(r) => {
                    eprintln!(
                        "final: items {:?} usage {:?}",
                        item_types(&r.output_items),
                        r.usage
                    );
                    r.text
                }
                Err(e) => {
                    eprintln!("final: ERROR {e}");
                    return;
                }
            }
        }
    };
    let outcome = check_reply(&text, &task, &palette, &vectors);
    let verdict = match &outcome {
        AttemptOutcome::Passed { scenarios, .. } => format!("PASSED all {scenarios}"),
        AttemptOutcome::ReplayFailed { passed, total, .. } => format!("replay {passed}/{total}"),
        other => format!("{other:?}").chars().take(300).collect(),
    };
    eprintln!(
        "final answer: {} chars, verdict {verdict}, wall {:.1}s",
        text.len(),
        started.elapsed().as_secs_f64()
    );
}
