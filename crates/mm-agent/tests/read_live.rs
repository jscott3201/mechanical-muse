//! Live: Muse Spark reads a sample sequence of operations, then drafts a graph against the
//! scenarios it proposed. Run with `make live-read`. Writes the reading and the trace to
//! `runs/reads/` (gitignored) for review; asserts only that the calls completed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use mm_agent::{AuthorConfig, Effort, Feedback, SequenceSource, author, read_sequence};
use mm_graph::Palette;

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

#[tokio::test]
#[ignore = "calls the Meta Model API; needs MODEL_API_KEY"]
async fn live_spark_reads_a_sequence_and_drafts_against_its_scenarios() {
    let key = std::env::var("MODEL_API_KEY").expect("set MODEL_API_KEY");
    let client = mm_agent::SparkClient::new(key).unwrap();
    let doc = std::env::var("MM_READ_DOC")
        .unwrap_or_else(|_| "fixtures/sequences/ahu-sat-reset-sample.md".into());
    let text = std::fs::read_to_string(repo(&doc)).unwrap();
    let source = SequenceSource {
        text,
        attachments: vec![],
    };
    let out = read_sequence(&client, "muse-spark-1.3", &source)
        .await
        .unwrap();
    let r = &out.read;
    eprintln!("read in {} call(s), usage {:?}", out.calls, out.usage);
    eprintln!(
        "title: {}\nname: {}\n{}",
        r.title, r.task.name, r.task.description
    );
    for p in r.task.inputs.iter().chain(&r.task.outputs) {
        eprintln!("  point {} ({:?}, {}): {}", p.name, p.ty, p.unit, p.meaning);
    }
    for p in &r.task.params {
        eprintln!("  param {} = {:?} {}", p.name, p.value, p.unit);
    }
    for q in &r.open_questions {
        eprintln!("  question: {q}");
    }
    for (s, why) in r.vectors.scenarios.iter().zip(&r.rationale) {
        eprintln!("  scenario {} ({} checks): {why}", s.name, s.expect.len());
    }
    for p in &r.problems {
        eprintln!("  PROBLEM: {p}");
    }
    let dir = repo("runs/reads");
    std::fs::create_dir_all(&dir).unwrap();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::write(
        dir.join(format!("read-{stamp}.json")),
        serde_json::to_string_pretty(r).unwrap(),
    )
    .unwrap();

    let cfg = AuthorConfig {
        model: "muse-spark-1.3".into(),
        effort: Some(Effort::High),
        max_output_tokens: 32_000,
        max_attempts: 4,
        feedback: Feedback::WithVectors,
    };
    let trace = author(
        &client,
        &Palette::pinned().unwrap(),
        &r.task,
        &r.vectors,
        &[],
        &cfg,
    )
    .await;
    for a in &trace.attempts {
        let outcome = serde_json::to_string(&a.outcome).unwrap();
        eprintln!(
            "attempt {}: {}",
            a.index,
            outcome.chars().take(500).collect::<String>()
        );
    }
    eprintln!(
        "verified: {}  usage: {:?}",
        trace.verified.is_some(),
        trace.usage
    );
    std::fs::write(
        dir.join(format!("trace-{stamp}.json")),
        serde_json::to_string_pretty(&trace).unwrap(),
    )
    .unwrap();
}
