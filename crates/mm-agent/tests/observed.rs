//! Progress reports from the authoring loop, and recorded replies played back through it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mm_agent::{
    AttemptOutcome, AuthorConfig, Feedback, ModelError, Progress, RecordedClient, RecordedReply,
    Recording, TaskSpec, Usage, author_observed,
};
use mm_bridge::VectorFile;
use mm_graph::Palette;

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

fn right() -> String {
    std::fs::read_to_string(repo("crates/mm-graph/tests/golden/ahu_0005.graph.json")).unwrap()
}

/// Loads and runs, but with the subtraction reversed, so replay fails.
fn swapped() -> String {
    let swapped = right()
        .replace("\"mat -> gap.u1\"", "\"MAT_TMP\"")
        .replace("\"sat -> gap.u2\"", "\"sat -> gap.u1\"")
        .replace("\"MAT_TMP\"", "\"mat -> gap.u2\"");
    assert_ne!(swapped, right(), "mutation did not apply");
    swapped
}

fn config(attempts: u32) -> AuthorConfig {
    AuthorConfig {
        model: "muse-spark-1.3".into(),
        effort: None,
        max_output_tokens: 16_000,
        max_attempts: attempts,
        feedback: Feedback::WithVectors,
    }
}

fn recording(replies: Vec<RecordedReply>) -> Recording {
    Recording {
        task_digest: None,
        model: "muse-spark-1.3".into(),
        task: "AHU-0005".into(),
        note: String::new(),
        replies,
    }
}

fn text(t: String) -> RecordedReply {
    RecordedReply::Reply {
        text: t,
        usage: Usage {
            output_tokens: 7,
            ..Usage::default()
        },
    }
}

/// What the observer saw, flattened to strings so the order is easy to assert.
async fn run_observed(
    replies: Vec<RecordedReply>,
    attempts: u32,
) -> (Vec<String>, mm_agent::AuthoringTrace) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let client = RecordedClient::new(recording(replies), Duration::ZERO);
    let trace = author_observed(
        &client,
        &Palette::pinned().unwrap(),
        &task(),
        &vectors(),
        &[],
        &config(attempts),
        move |p| {
            let line = match p {
                Progress::Drafting { attempt, of } => format!("drafting {attempt}/{of}"),
                Progress::Checked(a) => {
                    let json = serde_json::to_value(&a.outcome).unwrap();
                    format!("checked {} {}", a.index, json["result"].as_str().unwrap())
                }
            };
            sink.lock().unwrap().push(line);
        },
    )
    .await;
    let seen = seen.lock().unwrap().clone();
    (seen, trace)
}

#[tokio::test]
async fn progress_is_reported_before_each_call_and_after_each_check() {
    let (seen, trace) = run_observed(vec![text(swapped()), text(right())], 3).await;
    assert_eq!(
        seen,
        [
            "drafting 1/3",
            "checked 1 replay-failed",
            "drafting 2/3",
            "checked 2 passed",
        ]
    );
    assert!(trace.verified.is_some());
    assert_eq!(trace.usage.output_tokens, 14);
}

#[tokio::test]
async fn a_failed_model_call_is_reported_too() {
    let error = ModelError::Http {
        status: 400,
        message: "filtered".into(),
    };
    let (seen, trace) = run_observed(vec![RecordedReply::Error { error }], 3).await;
    assert_eq!(seen, ["drafting 1/3", "checked 1 model-error"]);
    assert!(trace.verified.is_none());
}

#[tokio::test]
async fn a_recording_replays_to_the_same_outcomes_and_receipt() {
    let error = ModelError::Incomplete {
        reason: "max_output_tokens".into(),
    };
    let replies = vec![
        RecordedReply::Error { error },
        text(swapped()),
        text(right()),
    ];
    let (_, first) = run_observed(replies, 4).await;
    assert_eq!(first.attempts.len(), 3);

    // Record the run, round-trip it through JSON (as a fixture file would), and play it back.
    let recorded = Recording::from_trace(&first);
    let json = serde_json::to_string(&recorded).unwrap();
    let back: Recording = serde_json::from_str(&json).unwrap();
    assert_eq!(back, recorded);
    let (_, again) = run_observed(back.replies, 4).await;

    let outcomes = |t: &mm_agent::AuthoringTrace| -> Vec<serde_json::Value> {
        t.attempts
            .iter()
            .map(|a| serde_json::to_value(&a.outcome).unwrap())
            .collect()
    };
    assert_eq!(outcomes(&again), outcomes(&first));
    let receipt = |t: &mm_agent::AuthoringTrace| t.verified.as_ref().map(|v| v.content_id.clone());
    assert!(receipt(&first).is_some());
    assert_eq!(receipt(&again), receipt(&first));
    assert!(matches!(
        again.attempts[0].outcome,
        AttemptOutcome::ModelError { .. }
    ));
}

#[tokio::test]
async fn an_exhausted_recording_is_a_model_error_not_a_hang() {
    let (seen, trace) = run_observed(vec![text(swapped())], 3).await;
    assert_eq!(
        seen,
        [
            "drafting 1/3",
            "checked 1 replay-failed",
            "drafting 2/3",
            "checked 2 model-error",
        ]
    );
    assert!(trace.verified.is_none());
}

/// The demo's recorded plant-reset run must still tell the same story on the pinned engine:
/// refused by the engine, then 8 of 10 scenarios, then green.
#[tokio::test]
async fn the_recorded_plant_reset_run_still_goes_red_amber_green() {
    let recording: Recording = serde_json::from_slice(
        &std::fs::read(repo("fixtures/recordings/chw_plant_reset.json")).unwrap(),
    )
    .unwrap();
    let task: TaskSpec =
        serde_json::from_slice(&std::fs::read(repo("specs/chw_plant_reset/task.json")).unwrap())
            .unwrap();
    let vectors: VectorFile =
        serde_json::from_slice(&std::fs::read(repo("specs/chw_plant_reset/vectors.json")).unwrap())
            .unwrap();
    assert_eq!(recording.task, task.name);
    let client = RecordedClient::new(recording, Duration::ZERO);
    let trace = mm_agent::author(
        &client,
        &Palette::pinned().unwrap(),
        &task,
        &vectors,
        &[],
        &config(5),
    )
    .await;
    let story: Vec<String> = trace
        .attempts
        .iter()
        .map(|a| match &a.outcome {
            AttemptOutcome::LoadFailed { failure } => format!("load-failed {}", failure.code),
            AttemptOutcome::ReplayFailed { passed, total, .. } => format!("{passed}/{total}"),
            AttemptOutcome::Passed { scenarios, .. } => format!("passed {scenarios}"),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(story, ["load-failed algebraic-loop", "8/10", "passed 10"]);
}
