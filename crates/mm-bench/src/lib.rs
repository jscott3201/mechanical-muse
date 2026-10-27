//! Fault-rule benchmark: rebuild each open-control-library rule from a scrubbed task, replay its
//! vectors, and record the run (`.agents/skills/mm-benchmark-hygiene`).
//!
//! The library is test data at a pinned commit; nothing here modifies it.

#![forbid(unsafe_code)]

mod baseline;
mod card;
mod library;
mod rescore;
mod run;
mod scrub;

pub use baseline::{Baseline, baseline_graph, baselines, interface_of};
pub use card::{Card, CardOutput, CardParam, parse_card};
pub use library::{Rule, Split, load_rule, load_rule_files, load_split};
pub use rescore::{Changed, RescoreReport, rescore, verdict};
pub use run::{
    BenchConfig, ENGINE_REV, FailureKind, RuleResult, RunRecord, RunSummary, prompt_hash,
    run_bench, summarize,
};
pub use scrub::{PointInfo, Scrubbed, Tier, find_leaks, leak_tokens, parse_points, scrub, snake};

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("card: {0}")]
    Card(String),
    #[error("{rule}: task would leak {tokens:?}")]
    Leak { rule: String, tokens: Vec<String> },
    #[error("io {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cxf: {0}")]
    Import(#[from] mm_graph::ImportError),
    #[error("{0}")]
    Other(String),
}
