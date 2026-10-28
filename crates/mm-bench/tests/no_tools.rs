//! Benchmark hygiene: a benchmark run never offers Spark's tools. `replay_scenario` would show
//! the library's scoring vectors to the model, so the bench drafts blind (`mm_agent::author`,
//! never `author_with_tools`). This test fails if a request ever carries a tool, or if the
//! bench's source starts calling the tools entry point.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mm_agent::{
    AuthorConfig, Feedback, ModelClient, ModelError, ModelReply, ModelRequest, TaskSpec, Usage,
};
use mm_bench::{BenchConfig, Tier, run_bench};
use mm_bridge::VectorFile;
use mm_graph::Palette;
use serde_json::json;

/// Answers every call with the reference graph and keeps every request.
struct Watcher {
    answer: String,
    requests: Mutex<Vec<ModelRequest>>,
}

impl ModelClient for Watcher {
    async fn complete(&self, request: &ModelRequest) -> Result<ModelReply, ModelError> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(ModelReply {
            output_items: vec![json!({"type": "message", "role": "assistant",
                "content": [{"type": "output_text", "text": self.answer}]})],
            text: self.answer.clone(),
            usage: Usage::default(),
        })
    }
}

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[tokio::test]
async fn bench_requests_never_offer_tools() {
    let task: TaskSpec = serde_json::from_slice(
        &std::fs::read(repo("crates/mm-agent/tests/data/ahu_0005.task.json")).unwrap(),
    )
    .unwrap();
    let vectors: VectorFile = serde_json::from_slice(
        &std::fs::read(repo("fixtures/open-control-library/AHU-0005/vectors.json")).unwrap(),
    )
    .unwrap();
    // A wrong answer, so the repair turns run too.
    let answer = std::fs::read_to_string(repo("crates/mm-graph/tests/golden/ahu_0005.graph.json"))
        .unwrap()
        .replace("\"mat -> gap.u1\"", "\"MAT_TMP\"")
        .replace("\"sat -> gap.u2\"", "\"sat -> gap.u1\"")
        .replace("\"MAT_TMP\"", "\"mat -> gap.u2\"");
    let client = Arc::new(Watcher {
        answer,
        requests: Mutex::new(Vec::new()),
    });
    let config = BenchConfig {
        author: AuthorConfig {
            model: "muse-spark-1.3-contributor".into(),
            effort: None,
            max_output_tokens: 1000,
            max_attempts: 2,
            feedback: Feedback::WithVectors,
        },
        tier: Tier::A,
        subset: "test".into(),
        runs: 1,
        exemplar_ids: Vec::new(),
        concurrency: 1,
        library_commit: "124427b".into(),
    };
    let results = run_bench(
        Arc::clone(&client),
        Arc::new(Palette::pinned().unwrap()),
        vec![("AHU-0005".into(), task, vectors)],
        Arc::new(Vec::new()),
        &config,
    )
    .await;
    assert_eq!(results.len(), 1);
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "a draft and a repair");
    for r in requests.iter() {
        assert!(r.tools.is_empty(), "a bench request offered tools");
        for name in ["replay_scenario", "check_graph", "describe_block"] {
            assert!(
                !r.instructions.contains(name),
                "instructions mention {name}"
            );
        }
    }
    assert!(
        results[0]
            .trace
            .attempts
            .iter()
            .all(|a| a.tool_turns.is_empty())
    );
}

#[test]
fn bench_source_never_calls_the_tools_entry_point() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = std::fs::read_dir(&src)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for banned in ["author_with_tools", "DraftTools", "ToolLimits"] {
            assert!(!text.contains(banned), "{} uses {banned}", file.display());
        }
    }
}
