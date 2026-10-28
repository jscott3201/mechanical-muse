//! Spark's own checks (function tools) against a scripted model, the real validator and real
//! engine replay. The tools help the model draft; the receipt is still the full replay.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use mm_agent::{
    AttemptOutcome, AuthorConfig, DraftTools, Feedback, MAX_OUTPUT_BYTES, ModelClient, ModelError,
    ModelReply, ModelRequest, Progress, RecordedClient, RecordedReply, Recording, TaskSpec,
    ToolLimits, Usage, author_with_tools,
};
use mm_bridge::VectorFile;
use mm_graph::Palette;
use serde_json::{Value, json};

struct ScriptedModel {
    replies: Mutex<VecDeque<ModelReply>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl ScriptedModel {
    fn new(replies: Vec<ModelReply>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
    fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl ModelClient for ScriptedModel {
    async fn complete(&self, request: &ModelRequest) -> Result<ModelReply, ModelError> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("more calls than scripted"))
    }
}

fn usage() -> Usage {
    Usage {
        input_tokens: 100,
        cached_tokens: 0,
        output_tokens: 10,
        reasoning_tokens: 5,
    }
}

fn calls(list: &[(&str, &str, String)]) -> ModelReply {
    let mut items = vec![json!({"type": "reasoning", "summary": [], "encrypted_content": "x"})];
    items.extend(list.iter().map(|(id, name, args)| {
        json!({"type": "function_call", "call_id": id, "name": name, "arguments": args})
    }));
    ModelReply {
        output_items: items,
        text: String::new(),
        usage: usage(),
    }
}

fn answer(text: &str) -> ModelReply {
    ModelReply {
        output_items: vec![json!({"type": "message", "role": "assistant",
            "content": [{"type": "output_text", "text": text}]})],
        text: text.to_owned(),
        usage: usage(),
    }
}

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn spec() -> (TaskSpec, VectorFile, String) {
    let read = |f: &str| std::fs::read(repo(&format!("specs/chw_plant_reset/{f}"))).unwrap();
    (
        serde_json::from_slice(&read("task.json")).unwrap(),
        serde_json::from_slice(&read("vectors.json")).unwrap(),
        String::from_utf8(read("graph.json")).unwrap(),
    )
}

/// Loads and runs, but the respond cap is wrong (0.10 instead of 0.07).
fn wrong_cap(right: &str) -> String {
    let mut graph: Value = serde_json::from_str(right).unwrap();
    for block in graph["blocks"].as_array_mut().unwrap() {
        if block["id"] == "respCapValue" {
            block["params"][0]["value"] = json!(0.10);
        }
    }
    graph.to_string()
}

fn config() -> AuthorConfig {
    AuthorConfig {
        model: "muse-spark-1.3".into(),
        effort: None,
        max_output_tokens: 16_000,
        max_attempts: 2,
        feedback: Feedback::WithVectors,
    }
}

fn graph_args(graph: &str, scenario: Option<&str>) -> String {
    let graph: Value = serde_json::from_str(graph).unwrap();
    match scenario {
        Some(s) => json!({"graph": graph, "scenario": s}).to_string(),
        None => json!({"graph": graph}).to_string(),
    }
}

#[test]
fn tools_report_the_first_failing_instant() {
    let (task, vectors, right) = spec();
    let palette = Palette::pinned().unwrap();
    let tools = DraftTools::new(&palette, &task, &vectors);
    let pass = tools.call("check_graph", &graph_args(&right, None));
    assert!(pass.ok, "{}", pass.output);
    let failing = vectors
        .scenarios
        .iter()
        .map(|s| {
            (
                s.name.clone(),
                tools.call(
                    "replay_scenario",
                    &graph_args(&wrong_cap(&right), Some(&s.name)),
                ),
            )
        })
        .find(|(_, r)| !r.ok)
        .expect("the wrong cap fails some scenario");
    let output: Value = serde_json::from_str(&failing.1.output).unwrap();
    assert_eq!(output["result"], "fail", "{output}");
    assert!(output["t_s"].is_number() && output["expected"].is_number());
    assert!(output["inputs_at_t"].as_str().unwrap().contains("nReqRes"));
    assert!(
        failing.1.summary.contains(&failing.0),
        "{}",
        failing.1.summary
    );
    // Deterministic: the same call gives the same bytes.
    let again = tools.call(
        "replay_scenario",
        &graph_args(&wrong_cap(&right), Some(&failing.0)),
    );
    assert_eq!(again, failing.1);
}

#[test]
fn tool_arguments_are_never_trusted() {
    let (task, vectors, right) = spec();
    let palette = Palette::pinned().unwrap();
    let tools = DraftTools::new(&palette, &task, &vectors);
    let refused = |name: &str, args: &str| {
        let r = tools.call(name, args);
        assert!(!r.ok, "{name} {args}: {}", r.output);
        assert!(r.output.len() <= MAX_OUTPUT_BYTES);
        r
    };
    refused("check_graph", "not json");
    refused("check_graph", "[1, 2]");
    refused("check_graph", r#"{"graph": {"name": 3}}"#);
    refused(
        "replay_scenario",
        &graph_args(&right, Some("no_such_scenario")),
    );
    refused("replay_scenario", &graph_args(&right, None));
    refused("rm_rf", "{}");
    let huge = format!(r#"{{"graph": "{}"}}"#, "x".repeat(300_000));
    let r = refused("check_graph", &huge);
    assert!(r.output.contains("limit"), "{}", r.output);
    let r = refused("describe_block", r#"{"class": "Reals.Nope"}"#);
    assert!(r.output.contains("similar"), "{}", r.output);
    let ok = tools.call("describe_block", r#"{"class": "Reals.Line"}"#);
    assert!(ok.ok && ok.output.contains("limitAbove"), "{}", ok.output);
    // Every scenario name is offered as an enum; nothing else.
    let defs = tools.definitions();
    assert_eq!(defs.len(), 3);
    let names = &defs[1]["parameters"]["properties"]["scenario"]["enum"];
    assert_eq!(names.as_array().unwrap().len(), vectors.scenarios.len());
}

#[tokio::test]
async fn spark_checks_its_draft_then_the_receipt_decides() {
    let (task, vectors, right) = spec();
    let palette = Palette::pinned().unwrap();
    let model = ScriptedModel::new(vec![
        calls(&[
            ("call_a", "check_graph", "{oops".into()),
            (
                "call_b",
                "replay_scenario",
                graph_args(&wrong_cap(&right), Some("ignore_respond_cap")),
            ),
        ]),
        calls(&[(
            "call_c",
            "describe_block",
            r#"{"class":"Reals.Min"}"#.into(),
        )]),
        answer(&right),
    ]);
    let mut seen = Vec::new();
    let trace = author_with_tools(
        &model,
        &palette,
        &task,
        &vectors,
        &[],
        &config(),
        ToolLimits::default(),
        |p| {
            if let Progress::ToolCall { attempt, call } = p {
                seen.push(format!("{attempt} {} {}", call.name, call.ok));
            }
        },
    )
    .await;
    assert_eq!(
        seen,
        [
            "1 check_graph false",
            "1 replay_scenario false",
            "1 describe_block true"
        ]
    );
    assert_eq!(trace.attempts.len(), 1);
    let attempt = &trace.attempts[0];
    assert!(matches!(attempt.outcome, AttemptOutcome::Passed { .. }));
    assert_eq!(attempt.calls, 3);
    assert_eq!(attempt.tool_turns.len(), 2);
    assert_eq!(attempt.usage.input_tokens, 300, "every call's usage counts");
    assert!(
        trace.verified.is_some(),
        "the receipt replayed every scenario"
    );

    let requests = model.requests();
    assert!(requests.iter().all(|r| r.tools.len() == 3));
    assert!(requests[0].instructions.contains("replay_scenario"));
    // The second request replays the calls and answers each, in call order.
    let outputs: Vec<&Value> = requests[1]
        .input
        .iter()
        .filter(|i| i["type"] == "function_call_output")
        .collect();
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0]["call_id"], "call_a");
    assert_eq!(outputs[1]["call_id"], "call_b");
    assert!(outputs[1]["output"].as_str().unwrap().contains("\"fail\""));
}

#[tokio::test]
async fn a_spent_turn_budget_asks_for_the_answer_without_tools() {
    let (task, vectors, right) = spec();
    let palette = Palette::pinned().unwrap();
    let describe = || calls(&[("c", "describe_block", r#"{"class":"Reals.Add"}"#.into())]);
    let model = ScriptedModel::new(vec![describe(), describe(), answer(&right)]);
    let limits = ToolLimits {
        max_turns: 2,
        ..ToolLimits::default()
    };
    let trace = author_with_tools(
        &model,
        &palette,
        &task,
        &vectors,
        &[],
        &config(),
        limits,
        |_| {},
    )
    .await;
    assert!(trace.verified.is_some());
    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[1].tools.len(), 3);
    assert!(
        requests[2].tools.is_empty(),
        "the last call offers no tools"
    );
    assert!(requests[2].schema.is_some(), "and keeps the schema");
    let last = &requests[2].input.last().unwrap()["content"][0]["text"];
    assert!(last.as_str().unwrap().contains("No more tool calls"));
}

#[tokio::test]
async fn the_session_call_budget_holds_across_attempts() {
    let (task, vectors, right) = spec();
    let palette = Palette::pinned().unwrap();
    let three = || {
        calls(&[
            ("a", "describe_block", r#"{"class":"Reals.Add"}"#.into()),
            ("b", "describe_block", r#"{"class":"Reals.Min"}"#.into()),
            ("c", "describe_block", r#"{"class":"Reals.Max"}"#.into()),
        ])
    };
    let model = ScriptedModel::new(vec![three(), answer(&wrong_cap(&right)), answer(&right)]);
    let limits = ToolLimits {
        max_turns: 8,
        max_calls: 2,
        max_lookups: 10,
    };
    let trace = author_with_tools(
        &model,
        &palette,
        &task,
        &vectors,
        &[],
        &config(),
        limits,
        |_| {},
    )
    .await;
    assert_eq!(
        trace.attempts[0].tool_turns[0].calls.len(),
        2,
        "third refused"
    );
    let requests = model.requests();
    let budget = requests[1]
        .input
        .iter()
        .filter(|i| i["type"] == "function_call_output")
        .filter(|i| i["output"].as_str().unwrap().contains("budget"))
        .count();
    assert_eq!(budget, 1);
    assert!(requests[1].tools.is_empty());
    // Attempt 2 (after the red receipt) gets no tools: the session budget is spent.
    assert!(requests[2].tools.is_empty());
    assert!(matches!(
        trace.attempts[0].outcome,
        AttemptOutcome::ReplayFailed { .. }
    ));
    assert!(trace.verified.is_some());
}

#[tokio::test]
async fn lookups_past_their_budget_are_refused_without_spending_calls() {
    let (task, vectors, right) = spec();
    let palette = Palette::pinned().unwrap();
    let many = calls(&[
        ("a", "describe_block", r#"{"class":"Reals.Add"}"#.into()),
        ("b", "describe_block", r#"{"class":"Reals.Min"}"#.into()),
        ("c", "describe_block", r#"{"class":"Reals.Max"}"#.into()),
    ]);
    let model = ScriptedModel::new(vec![many, answer(&right)]);
    let limits = ToolLimits {
        max_lookups: 2,
        ..ToolLimits::default()
    };
    let mut seen = Vec::new();
    let trace = author_with_tools(
        &model,
        &palette,
        &task,
        &vectors,
        &[],
        &config(),
        limits,
        |p| {
            if let Progress::ToolCall { call, .. } = p {
                seen.push(call.ok);
            }
        },
    )
    .await;
    assert_eq!(seen, [true, true, false]);
    assert!(trace.verified.is_some());
    // The model was still offered tools on its next turn: no call was spent.
    assert_eq!(model.requests()[1].tools.len(), 3);
}

#[tokio::test]
async fn a_tools_session_records_and_plays_back_without_a_key() {
    let (task, vectors, right) = spec();
    let palette = Palette::pinned().unwrap();
    let model = ScriptedModel::new(vec![
        calls(&[(
            "call_x",
            "replay_scenario",
            graph_args(&wrong_cap(&right), Some("ignore_respond_cap")),
        )]),
        answer(&wrong_cap(&right)),
        calls(&[("call_y", "check_graph", graph_args(&right, None))]),
        answer(&right),
    ]);
    let (trace, live) = run(&model, &palette, &task, &vectors).await;
    let recording = Recording::from_trace(&trace);
    assert!(recording.uses_tools());
    assert_eq!(recording.attempts(), 2);
    assert!(matches!(
        recording.replies[0],
        RecordedReply::ToolCalls { .. }
    ));
    let text = serde_json::to_string(&recording).unwrap();
    assert!(!text.contains("call_x"), "no call ids in a recording");
    assert!(!text.contains("encrypted"), "no reasoning in a recording");

    let played = RecordedClient::new(recording, Duration::ZERO);
    let (again, summaries) = run(&played, &palette, &task, &vectors).await;
    assert_eq!(summaries, live, "the checks run again and say the same");
    assert_eq!(
        again.verified.map(|v| v.content_id),
        trace.verified.map(|v| v.content_id)
    );
}

async fn run<C: ModelClient>(
    client: &C,
    palette: &Palette,
    task: &TaskSpec,
    vectors: &VectorFile,
) -> (mm_agent::AuthoringTrace, Vec<String>) {
    let mut summaries = Vec::new();
    let trace = author_with_tools(
        client,
        palette,
        task,
        vectors,
        &[],
        &config(),
        ToolLimits::default(),
        |p| {
            if let Progress::ToolCall { call, .. } = p {
                summaries.push(call.summary.clone());
            }
        },
    )
    .await;
    (trace, summaries)
}

/// Every function_call in a request's history has its output, or the API refuses the request.
fn assert_calls_answered(request: &ModelRequest) {
    let of = |kind: &str| -> Vec<String> {
        request
            .input
            .iter()
            .filter(|i| i["type"] == kind)
            .map(|i| i["call_id"].as_str().unwrap_or("").to_owned())
            .collect()
    };
    assert_eq!(of("function_call"), of("function_call_output"));
}

#[tokio::test]
async fn unanswered_or_malformed_calls_never_reach_the_history() {
    let (task, vectors, right) = spec();
    let palette = Palette::pinned().unwrap();
    // A malformed call (no call_id) beside a good one, then a call on the tool-less last
    // call together with a wrong graph, then the right graph.
    let mut first = calls(&[("c1", "describe_block", r#"{"class":"Reals.Add"}"#.into())]);
    first
        .output_items
        .push(json!({"type": "function_call", "name": "check_graph", "arguments": "{}"}));
    let mut stray = answer(&wrong_cap(&right));
    stray
        .output_items
        .push(json!({"type": "function_call", "call_id": "c2",
        "name": "check_graph", "arguments": "{}"}));
    let model = ScriptedModel::new(vec![first, stray, answer(&right)]);
    let limits = ToolLimits {
        max_turns: 1,
        ..ToolLimits::default()
    };
    let trace = author_with_tools(
        &model,
        &palette,
        &task,
        &vectors,
        &[],
        &config(),
        limits,
        |_| {},
    )
    .await;
    assert!(trace.verified.is_some());
    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_calls_answered(request);
    }
}
