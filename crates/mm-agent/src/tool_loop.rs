//! One draft with Spark's own checks: the model may call `check_graph`, `replay_scenario` and
//! `describe_block` (see `tools.rs`) before it answers. Bounded by turns per attempt and calls
//! per session; when a budget runs out, the pending calls are answered with "no more tool
//! calls" and one last call without tools asks for the graph. Nothing here verifies a draft:
//! the loop in `repair.rs` checks the final answer exactly as it checks a draft made without
//! tools.

use serde::Serialize;
use serde_json::{Value, json};

use crate::client::{ModelClient, ModelError, ModelReply, ModelRequest, Usage, function_calls};
use crate::tools::{DESCRIBE_BLOCK, DraftTools, ToolLimits, ToolResult, summarize_arguments};

/// Appended to the system instructions when the tools are offered.
pub(crate) const TOOL_RULES: &str = "\n\nTools: the block classes above are complete; call \
describe_block only for a class you are unsure of. Before you answer, check your draft: \
check_graph for structure, replay_scenario for behavior on the task's scenarios. Fix what they \
report, then reply with the final block graph. Your answer is verified again on every scenario \
after you reply; the tools are for your own checking.";

/// One tool call: what the model asked for and what our check said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolCallRecord {
    pub name: String,
    /// The raw arguments, untrusted; kept so a recording can replay the call.
    #[serde(skip)]
    pub arguments: String,
    /// A short description of the arguments (scenario or class, and the draft's size).
    pub args_summary: String,
    pub ok: bool,
    pub summary: String,
}

/// One model call that asked for tools, and the calls it made.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolTurn {
    pub calls: Vec<ToolCallRecord>,
    pub usage: Usage,
}

/// The tools and what is left of the session's call budget.
pub(crate) struct ToolBox<'a> {
    pub tools: DraftTools<'a>,
    pub limits: ToolLimits,
    pub calls_used: u32,
    pub lookups_used: u32,
}

/// What one attempt's model work produced.
pub(crate) struct Drafted {
    /// The final reply. On success its `output_items` hold every item of the attempt (tool
    /// turns, our call outputs, the final answer) in order, for replay in the next attempt.
    pub reply: Result<ModelReply, ModelError>,
    pub turns: Vec<ToolTurn>,
    /// Model calls made, including the final one.
    pub calls: u32,
    /// Usage of every call, the failed one excluded.
    pub usage: Usage,
}

/// Calls the model until it answers without asking for tools (or a budget runs out).
/// `on_call` sees each tool call after it ran.
pub(crate) async fn draft_with_tools<C, F>(
    client: &C,
    request: &ModelRequest,
    toolbox: &mut ToolBox<'_>,
    mut on_call: F,
) -> Drafted
where
    C: ModelClient,
    F: FnMut(&ToolCallRecord),
{
    let mut request = request.clone();
    // A spent session budget means this attempt drafts without tools.
    if toolbox.calls_used < toolbox.limits.max_calls {
        request.tools = toolbox.tools.definitions();
    }
    let mut items: Vec<Value> = Vec::new();
    let mut turns = Vec::new();
    let mut usage = Usage::default();
    let mut calls = 0;
    let fail = |error, turns, calls, usage| Drafted {
        reply: Err(error),
        turns,
        calls,
        usage,
    };
    loop {
        calls += 1;
        let reply = match client.complete(&request).await {
            Ok(reply) => reply,
            Err(error) => return fail(error, turns, calls, usage),
        };
        usage.add(reply.usage);
        let asked = function_calls(&reply.output_items);
        let answering = !asked.is_empty() && !request.tools.is_empty();
        // Replay only the function calls that get an output below: an unanswered or
        // malformed call left in the history makes the next request fail.
        let kept: Vec<Value> = reply
            .output_items
            .iter()
            .filter(|item| answering_keeps(item, answering))
            .cloned()
            .collect();
        request.input.extend(kept.iter().cloned());
        items.extend(kept);
        if !answering {
            return Drafted {
                reply: Ok(ModelReply {
                    output_items: items,
                    text: reply.text,
                    usage,
                }),
                turns,
                calls,
                usage,
            };
        }
        let turn_budget_left =
            u32::try_from(turns.len()).unwrap_or(u32::MAX) + 1 < toolbox.limits.max_turns;
        let mut turn = ToolTurn {
            calls: Vec::new(),
            usage: reply.usage,
        };
        for call in asked {
            let within = toolbox.calls_used < toolbox.limits.max_calls;
            let lookup = call.name == DESCRIBE_BLOCK;
            let output = if within {
                // A lookup past its budget is refused without spending a call.
                let result = if lookup && toolbox.lookups_used >= toolbox.limits.max_lookups {
                    ToolResult {
                        ok: false,
                        summary: "lookup refused: the block list in the instructions is complete"
                            .into(),
                        output: json!({"ok": false, "error": "budget",
                            "detail": "No more describe_block calls. The block classes in the \
                                instructions list every port and parameter."})
                        .to_string(),
                    }
                } else {
                    toolbox.calls_used += 1;
                    toolbox.lookups_used += u32::from(lookup);
                    toolbox.tools.call(&call.name, &call.arguments)
                };
                let record = ToolCallRecord {
                    args_summary: summarize_arguments(&call.name, &call.arguments),
                    name: call.name,
                    arguments: call.arguments,
                    ok: result.ok,
                    summary: result.summary,
                };
                on_call(&record);
                turn.calls.push(record);
                result.output
            } else {
                json!({"ok": false, "error": "budget",
                       "detail": "No more tool calls in this session."})
                .to_string()
            };
            // Every function_call needs its output (same call_id, in call order), or the
            // next request is refused.
            let item = json!({"type": "function_call_output", "call_id": call.call_id,
                              "output": output});
            request.input.push(item.clone());
            items.push(item);
        }
        turns.push(turn);
        if !turn_budget_left || toolbox.calls_used >= toolbox.limits.max_calls {
            // Last call: no tools, the same schema, and a plain instruction.
            request.tools.clear();
            let note = json!({"role": "user", "content": [{"type": "input_text",
                "text": "No more tool calls. Reply with the final block graph."}]});
            request.input.push(note.clone());
            items.push(note);
        }
    }
}

/// Whether an output item goes back into the history: everything but function calls that
/// will not be answered (no tools on this call, or no `call_id`/`name` to answer).
fn answering_keeps(item: &Value, answering: bool) -> bool {
    if item.get("type").and_then(Value::as_str) != Some("function_call") {
        return true;
    }
    answering && !function_calls(std::slice::from_ref(item)).is_empty()
}
