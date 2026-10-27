//! Re-scoring stored replies: the same drafts get checked again, and any verdict that differs
//! from the record is reported.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use mm_agent::TaskSpec;
use mm_bridge::VectorFile;
use mm_graph::Palette;
use serde_json::json;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn inputs() -> BTreeMap<String, (TaskSpec, VectorFile)> {
    let task: TaskSpec = serde_json::from_slice(
        &std::fs::read(repo("crates/mm-agent/tests/data/ahu_0005.task.json")).unwrap(),
    )
    .unwrap();
    let vectors: VectorFile = serde_json::from_slice(
        &std::fs::read(repo("fixtures/open-control-library/AHU-0005/vectors.json")).unwrap(),
    )
    .unwrap();
    BTreeMap::from([("AHU-0005".to_owned(), (task, vectors))])
}

fn right() -> String {
    std::fs::read_to_string(repo("crates/mm-graph/tests/golden/ahu_0005.graph.json")).unwrap()
}

fn swapped() -> String {
    let s = right()
        .replace("\"mat -> gap.u1\"", "\"MAT_TMP\"")
        .replace("\"sat -> gap.u2\"", "\"sat -> gap.u1\"")
        .replace("\"MAT_TMP\"", "\"mat -> gap.u2\"");
    assert_ne!(s, right(), "mutation did not apply");
    s
}

fn attempt(index: u32, reply: Option<String>, result: serde_json::Value) -> serde_json::Value {
    json!({"index": index, "reply": reply, "outcome": result})
}

#[test]
fn unchanged_verdicts_are_counted_and_changed_ones_listed() {
    let palette = Palette::pinned().unwrap();
    let inputs = inputs();
    let (task, vectors) = &inputs["AHU-0005"];
    // What the swapped draft really gets, recorded truthfully below.
    let swapped_outcome =
        serde_json::to_value(mm_agent::check_reply(&swapped(), task, &palette, vectors)).unwrap();
    assert_eq!(swapped_outcome["result"], "replay-failed");

    // Run 0: wrong then right, recorded truthfully. Run 1: a right draft recorded as invalid,
    // then a model error with no reply (skipped).
    let record = json!({"results": [
        {"rule": "AHU-0005", "run": 0, "trace": {"attempts": [
            attempt(1, Some(swapped()), swapped_outcome),
            attempt(2, Some(right()), json!({"result": "passed"})),
        ]}},
        {"rule": "AHU-0005", "run": 1, "trace": {"attempts": [
            attempt(1, Some(right()), json!({"result": "invalid"})),
            attempt(2, None, json!({"result": "model-error"})),
        ]}},
    ]});
    let report = mm_bench::rescore(&record, &inputs, &palette).unwrap();
    assert_eq!(report.rule_runs, 2);
    assert_eq!(report.replies, 3);
    assert_eq!((report.first_try_then, report.first_try_now), (0, 1));
    assert_eq!((report.verified_then, report.verified_now), (1, 2));
    let listed: Vec<_> = report
        .changed
        .iter()
        .map(|c| (c.run, c.attempt, c.recorded.as_str(), c.now.as_str()))
        .collect();
    assert_eq!(listed, [(1, 1, "invalid", "passed")]);
}

#[test]
fn a_rule_with_no_inputs_is_an_error() {
    let record = json!({"results": [{"rule": "AHU-9999", "run": 0, "trace": {"attempts": []}}]});
    let err = mm_bench::rescore(&record, &inputs(), &Palette::pinned().unwrap()).unwrap_err();
    assert!(err.to_string().contains("AHU-9999"), "{err}");
}
