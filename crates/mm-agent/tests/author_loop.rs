//! The authoring loop against a scripted model, the real validator and real engine replay.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

use mm_agent::{
    AttemptOutcome, AuthorConfig, Feedback, ModelClient, ModelError, ModelReply, ModelRequest,
    TaskSpec, Usage, author,
};
use mm_bridge::{ReplayLimits, VectorFile, replay};
use mm_graph::Palette;
use serde_json::{Value, json};

/// Replays scripted replies in order and records every request it was sent.
struct ScriptedModel {
    replies: Mutex<VecDeque<Result<ModelReply, ModelError>>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl ScriptedModel {
    fn new(replies: Vec<Result<ModelReply, ModelError>>) -> Self {
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
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("the loop asked for more replies than scripted")
    }
}

/// A reply shaped like a Responses output: an encrypted reasoning item, then the message.
fn reply(text: &str) -> Result<ModelReply, ModelError> {
    Ok(ModelReply {
        output_items: vec![
            json!({"type": "reasoning", "id": "rs_1", "summary": [], "encrypted_content": "opaque"}),
            json!({"type": "message", "role": "assistant",
                   "content": [{"type": "output_text", "text": text}]}),
        ],
        text: text.to_owned(),
        usage: Usage {
            input_tokens: 1000,
            cached_tokens: 0,
            output_tokens: 200,
            reasoning_tokens: 150,
        },
    })
}

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn task() -> TaskSpec {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/ahu_0005.task.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn vectors() -> VectorFile {
    let path = repo("fixtures/open-control-library/AHU-0005/vectors.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// A correct answer: the mm-graph golden for this rule.
fn right() -> String {
    std::fs::read_to_string(repo("crates/mm-graph/tests/golden/ahu_0005.graph.json")).unwrap()
}

/// Valid and loadable, but computes sat - mat: the sign is wrong, so the alarm never fires.
fn swapped() -> String {
    right()
        .replace("\"mat -> gap.u1\"", "\"MAT_TMP\"")
        .replace("\"sat -> gap.u2\"", "\"sat -> gap.u1\"")
        .replace("\"MAT_TMP\"", "\"mat -> gap.u2\"")
}

fn config(attempts: u32, feedback: Feedback) -> AuthorConfig {
    AuthorConfig {
        model: "muse-spark-1.3".into(),
        effort: None,
        max_output_tokens: 16_000,
        max_attempts: attempts,
        feedback,
    }
}

fn last_input_text(request: &ModelRequest) -> String {
    request.input.last().unwrap()["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn converges_after_vector_feedback() {
    assert_ne!(swapped(), right(), "mutation did not apply");
    let model = ScriptedModel::new(vec![reply(&swapped()), reply(&right())]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(3, Feedback::WithVectors),
    )
    .await;

    assert_eq!(trace.attempts.len(), 2);
    assert!(matches!(
        trace.attempts[0].outcome,
        AttemptOutcome::ReplayFailed { .. }
    ));
    assert!(matches!(
        trace.attempts[1].outcome,
        AttemptOutcome::Passed { .. }
    ));
    assert!(trace.repair_saw_vectors);
    assert_eq!(trace.usage.output_tokens, 400);

    // The repair turn replays the model's own items (reasoning included), then the feedback.
    let second = &model.requests()[1];
    assert_eq!(second.input.len(), 4);
    assert_eq!(second.input[1]["type"], "reasoning");
    let feedback = last_input_text(second);
    assert!(
        feedback.contains("cooling_coil_leaking_through_heating"),
        "{feedback}"
    );
    assert!(feedback.contains("yFault should be true"), "{feedback}");
}

#[tokio::test]
async fn validator_only_feedback_hides_the_scenarios() {
    let model = ScriptedModel::new(vec![reply(&swapped()), reply(&right())]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(3, Feedback::ValidatorOnly),
    )
    .await;
    assert!(trace.verified.is_some());
    assert!(!trace.repair_saw_vectors);
    let feedback = last_input_text(&model.requests()[1]);
    // No scenario name, description, time, value or count reaches the model.
    for scenario in &vectors().scenarios {
        assert!(!feedback.contains(&scenario.name), "{feedback}");
        assert!(!feedback.contains(&scenario.description), "{feedback}");
    }
    for leak in ["t =", "should be", " of ", "yFault"] {
        assert!(!feedback.contains(leak), "{leak:?} in {feedback}");
    }
    assert!(feedback.contains("does not match the task"), "{feedback}");
}

#[tokio::test]
async fn the_verified_cxf_is_the_one_that_passed() {
    let model = ScriptedModel::new(vec![reply(&swapped()), reply(&right())]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(2, Feedback::ValidatorOnly),
    )
    .await;
    let verified = trace.verified.expect("second draft passes");
    let want: mm_graph::BlockGraph = serde_json::from_str(&right()).unwrap();
    assert_eq!(verified.graph, want);
    let report = replay(&verified.cxf, &vectors(), &ReplayLimits::default()).unwrap();
    assert!(report.passed());
    assert_eq!(report.content_id, verified.content_id);
}

#[tokio::test]
async fn static_problems_are_fed_back_with_codes() {
    let not_json = "Here is the graph you asked for.".to_owned();
    let renamed = right().replace("yFault", "fault");
    let no_delay = right().replace(r#"{"name": "delayTime", "value": 1800.0}, "#, "");
    assert_ne!(no_delay, right(), "mutation did not apply");
    let model = ScriptedModel::new(vec![
        reply(&not_json),
        reply(&renamed),
        reply(&no_delay),
        reply(&right()),
    ]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(4, Feedback::ValidatorOnly),
    )
    .await;
    let outcomes: Vec<_> = trace.attempts.iter().map(|a| &a.outcome).collect();
    assert!(matches!(outcomes[0], AttemptOutcome::NotAGraph { .. }));
    assert!(
        matches!(outcomes[1], AttemptOutcome::InterfaceMismatch { problems }
        if problems.iter().any(|p| p == "missing output yFault"))
    );
    assert!(matches!(outcomes[2], AttemptOutcome::Invalid { .. }));
    assert!(matches!(outcomes[3], AttemptOutcome::Passed { .. }));
    let requests = model.requests();
    assert!(last_input_text(&requests[3]).contains("missing-required-param at persist.delayTime"));
}

#[tokio::test]
async fn the_budget_caps_attempts() {
    let model = ScriptedModel::new(vec![
        reply(&swapped()),
        reply(&swapped()),
        reply(&swapped()),
    ]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(3, Feedback::WithVectors),
    )
    .await;
    assert_eq!(trace.attempts.len(), 3);
    assert!(trace.verified.is_none());
    assert_eq!(model.requests().len(), 3);
}

#[tokio::test]
async fn a_model_error_stops_the_loop() {
    let model = ScriptedModel::new(vec![Err(ModelError::Http {
        status: 401,
        message: "invalid key".into(),
    })]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(3, Feedback::WithVectors),
    )
    .await;
    assert_eq!(trace.attempts.len(), 1);
    assert!(matches!(
        trace.attempts[0].outcome,
        AttemptOutcome::ModelError { .. }
    ));
    assert!(trace.verified.is_none());
}

#[tokio::test]
async fn the_first_request_carries_schema_palette_and_task() {
    let model = ScriptedModel::new(vec![reply(&right())]);
    let _ = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(1, Feedback::ValidatorOnly),
    )
    .await;
    let first = &model.requests()[0];
    assert_eq!(first.schema.as_ref().unwrap().name, "block_graph");
    let palette_lines = first
        .instructions
        .lines()
        .filter(|l| l.starts_with("- ") && l.contains(": in("))
        .count();
    assert_eq!(palette_lines, 53);
    assert!(
        first
            .instructions
            .contains("- Reals.Subtract: in(u1: Real, u2: Real) -> out(y: Real); params: none")
    );
    assert!(first.instructions.contains("delayOnInit = true"));
    let task_text = first.input[0]["content"][0]["text"].as_str().unwrap();
    assert!(task_text.contains("- mat (real, degC):"));
    assert!(task_text.contains("gap = mat - sat"));
    // Vectors never reach the prompt.
    let everything: Value = json!([first.instructions, first.input]);
    assert!(!everything.to_string().contains("cooling_coil"));
}

/// Live call to Muse Spark. Run with `MODEL_API_KEY=... cargo test -p mm-agent --test
/// author_loop -- --ignored --nocapture`. Uses the standard (non-contributor) model.
#[tokio::test]
#[ignore = "calls the Meta Model API; needs MODEL_API_KEY"]
async fn live_spark_authors_ahu_0005() {
    let key = std::env::var("MODEL_API_KEY").expect("set MODEL_API_KEY");
    let client = mm_agent::SparkClient::new(key).unwrap();
    let mut cfg = config(3, Feedback::ValidatorOnly);
    cfg.effort = Some(mm_agent::Effort::Medium);
    let trace = author(
        &client,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &cfg,
    )
    .await;
    for a in &trace.attempts {
        let outcome = serde_json::to_string(&a.outcome).unwrap();
        eprintln!(
            "attempt {}: {}",
            a.index,
            outcome.chars().take(400).collect::<String>()
        );
    }
    eprintln!(
        "verified: {}  usage: {:?}",
        trace.verified.is_some(),
        trace.usage
    );
    assert!(!trace.attempts.is_empty());
}

#[tokio::test]
async fn showing_vectors_on_the_last_attempt_does_not_count() {
    // One attempt: the failure details are never sent, so the run stays first-try clean.
    let model = ScriptedModel::new(vec![reply(&swapped())]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(1, Feedback::WithVectors),
    )
    .await;
    assert!(trace.verified.is_none());
    assert!(!trace.repair_saw_vectors);
}

#[tokio::test]
async fn a_bad_vector_file_stops_before_any_model_call() {
    let model = ScriptedModel::new(vec![]);
    let mut bad = vectors();
    bad.scenarios[0].expect.clear();
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &bad,
        &[],
        &config(3, Feedback::ValidatorOnly),
    )
    .await;
    assert!(trace.setup_error.unwrap().contains("no-expectations"));
    assert!(trace.attempts.is_empty());
    assert!(model.requests().is_empty());
}

#[tokio::test]
async fn max_effort_on_a_contributor_model_is_refused() {
    let model = ScriptedModel::new(vec![]);
    let mut cfg = config(1, Feedback::ValidatorOnly);
    cfg.model = "muse-spark-1.3-contributor".into();
    cfg.effort = Some(mm_agent::Effort::Max);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &cfg,
    )
    .await;
    assert!(trace.setup_error.is_some());
    assert!(model.requests().is_empty());
}

#[tokio::test]
async fn a_truncated_reply_is_retried_with_the_same_input() {
    let model = ScriptedModel::new(vec![
        Err(ModelError::Incomplete {
            reason: "max_output_tokens".into(),
        }),
        reply(&right()),
    ]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(2, Feedback::ValidatorOnly),
    )
    .await;
    assert!(trace.verified.is_some());
    let requests = model.requests();
    assert_eq!(requests[0].input, requests[1].input);
}

#[tokio::test]
async fn the_sequence_name_comes_from_the_task() {
    let renamed = right().replace(
        "\"name\": \"ahu_0005\"",
        "\"name\": \"whatever_the_model_said\"",
    );
    assert_ne!(renamed, right(), "mutation did not apply");
    let model = ScriptedModel::new(vec![reply(&renamed)]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(1, Feedback::ValidatorOnly),
    )
    .await;
    assert_eq!(trace.verified.unwrap().graph.name, "ahu_0005");
}

#[tokio::test]
async fn an_algebraic_loop_is_explained_to_the_model() {
    // y = u + y with no delay: the engine refuses it at the schedule stage with no diagnostic
    // records, so the feedback must carry the engine's own explanation.
    let looped = right()
        .replace(
            r#"{"id": "gap", "class": "Reals.Subtract", "params": []}"#,
            r#"{"id": "gap", "class": "Reals.Subtract", "params": []}, {"id": "loopAdd", "class": "Reals.Add", "params": []}"#,
        )
        .replace(
            r#""gap.y -> gapBig.u""#,
            r#""gap.y -> loopAdd.u1", "loopAdd.y -> loopAdd.u2", "loopAdd.y -> gapBig.u""#,
        );
    assert_ne!(looped, right(), "mutation did not apply");
    let model = ScriptedModel::new(vec![reply(&looped), reply(&right())]);
    let trace = author(
        &model,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(2, Feedback::ValidatorOnly),
    )
    .await;
    assert!(matches!(
        trace.attempts[0].outcome,
        AttemptOutcome::LoadFailed { .. }
    ));
    let feedback = last_input_text(&model.requests()[1]);
    assert!(feedback.contains("algebraic loop"), "{feedback}");
}
