//! Running the model over a subset and recording everything needed to reproduce and judge the
//! numbers: model, effort, tier, subset, prompt hash, engine rev, library commit, budget,
//! baselines, tokens and every attempt.

use std::sync::Arc;

use mm_agent::{
    AttemptOutcome, AuthorConfig, AuthoringTrace, Exemplar, ModelClient, TaskSpec, Usage, author,
};
use mm_bridge::VectorFile;
use mm_graph::Palette;
use serde::Serialize;
use tokio::task::JoinSet;

use crate::baseline::Baseline;
use crate::scrub::Tier;

/// The engine every result is judged on.
pub const ENGINE_REV: &str = "4063202b9a9e50c723bb858d786108571d3d401d";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BenchConfig {
    pub author: AuthorConfig,
    pub tier: Tier,
    pub subset: String,
    /// Independent runs per rule (no documented seed, so report the spread).
    pub runs: u32,
    pub exemplar_ids: Vec<String>,
    pub concurrency: usize,
    pub library_commit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    ModelError,
    HarnessError,
    NotAGraph,
    InterfaceMismatch,
    Invalid,
    LoadFailed,
    ReplayFailed,
}

fn kind(outcome: &AttemptOutcome) -> Option<FailureKind> {
    Some(match outcome {
        AttemptOutcome::Passed { .. } => return None,
        AttemptOutcome::ModelError { .. } => FailureKind::ModelError,
        AttemptOutcome::HarnessError { .. } => FailureKind::HarnessError,
        AttemptOutcome::NotAGraph { .. } => FailureKind::NotAGraph,
        AttemptOutcome::InterfaceMismatch { .. } => FailureKind::InterfaceMismatch,
        AttemptOutcome::Invalid { .. } => FailureKind::Invalid,
        AttemptOutcome::LoadFailed { .. } => FailureKind::LoadFailed,
        AttemptOutcome::ReplayFailed { .. } => FailureKind::ReplayFailed,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuleResult {
    pub rule: String,
    pub run: u32,
    pub verified: bool,
    pub first_try_passed: bool,
    pub attempts_used: usize,
    pub first_failure: Option<FailureKind>,
    pub final_failure: Option<FailureKind>,
    /// Scenarios passed by the best attempt that reached replay (0 if none did).
    pub best_scenarios_passed: usize,
    pub scenarios: usize,
    pub usage: Usage,
    pub trace: AuthoringTrace,
}

impl RuleResult {
    /// A job that crashed before producing a trace: counted, never verified.
    fn harness_failure(
        rule: &str,
        run: u32,
        scenarios: usize,
        config: &AuthorConfig,
        why: &str,
    ) -> Self {
        let trace = AuthoringTrace {
            task: rule.to_owned(),
            config: config.clone(),
            attempts: Vec::new(),
            verified: None,
            repair_saw_vectors: false,
            setup_error: Some(format!("job failed: {why}")),
            usage: Usage::default(),
        };
        let mut result = Self::from_trace(rule, run, scenarios, trace);
        result.first_failure = Some(FailureKind::HarnessError);
        result.final_failure = Some(FailureKind::HarnessError);
        result
    }

    fn from_trace(rule: &str, run: u32, scenarios: usize, trace: AuthoringTrace) -> Self {
        let first = trace.attempts.first().map(|a| &a.outcome);
        let last = trace.attempts.last().map(|a| &a.outcome);
        let best = trace
            .attempts
            .iter()
            .map(|a| match &a.outcome {
                AttemptOutcome::Passed { scenarios, .. } => *scenarios,
                AttemptOutcome::ReplayFailed { passed, .. } => *passed,
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        Self {
            rule: rule.to_owned(),
            run,
            verified: trace.verified.is_some(),
            first_try_passed: matches!(first, Some(AttemptOutcome::Passed { .. })),
            attempts_used: trace.attempts.len(),
            first_failure: first.and_then(kind),
            final_failure: last.and_then(kind),
            best_scenarios_passed: best,
            scenarios,
            usage: trace.usage,
            trace,
        }
    }
}

/// Per-run counts; with several runs the spread shows the model's variance.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RunSummary {
    pub run: u32,
    pub rules: usize,
    pub first_try_rule_passes: usize,
    pub final_rule_passes: usize,
    pub scenarios_passed_best: usize,
    pub scenarios: usize,
    pub first_failures: Vec<(FailureKind, usize)>,
    pub usage: Usage,
}

pub fn summarize(results: &[RuleResult], runs: u32) -> Vec<RunSummary> {
    (0..runs)
        .map(|run| {
            let mut s = RunSummary {
                run,
                ..RunSummary::default()
            };
            let mut kinds: std::collections::BTreeMap<FailureKind, usize> = Default::default();
            for r in results.iter().filter(|r| r.run == run) {
                s.rules += 1;
                s.first_try_rule_passes += usize::from(r.first_try_passed);
                s.final_rule_passes += usize::from(r.verified);
                s.scenarios_passed_best += r.best_scenarios_passed;
                s.scenarios += r.scenarios;
                s.usage.add(r.usage);
                if let Some(k) = r.first_failure {
                    *kinds.entry(k).or_default() += 1;
                }
            }
            s.first_failures = kinds.into_iter().collect();
            s
        })
        .collect()
}

/// The full record written beside each result.
#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    pub schema: &'static str,
    pub config: BenchConfig,
    pub engine_rev: &'static str,
    pub prompt_hash: String,
    pub rules: Vec<String>,
    /// Rules dropped before running and why (e.g. tier B pseudo-code that names a class).
    pub skipped: Vec<(String, String)>,
    pub baseline_constant_false: Baseline,
    pub baseline_constant_true: Baseline,
    pub summary: Vec<RunSummary>,
    pub results: Vec<RuleResult>,
}

/// FNV-1a 64-bit, hex: a stable fingerprint of the exact prompt prefix and schema.
pub fn prompt_hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

/// Runs every (rule, run) pair with at most `config.concurrency` in flight. Results come back
/// sorted by rule then run, whatever order they finished in.
pub async fn run_bench<C>(
    client: Arc<C>,
    palette: Arc<Palette>,
    tasks: Vec<(String, TaskSpec, VectorFile)>,
    exemplars: Arc<Vec<Exemplar>>,
    config: &BenchConfig,
) -> Vec<RuleResult>
where
    C: ModelClient + Send + Sync + 'static,
{
    let mut jobs = Vec::new();
    for run in 0..config.runs {
        for (id, task, vectors) in &tasks {
            jobs.push((id.clone(), run, task.clone(), vectors.clone()));
        }
    }
    let mut pending = jobs.into_iter();
    let mut set = JoinSet::new();
    // Which job each task is, so a panicked or cancelled job still produces a result row.
    let mut running: std::collections::BTreeMap<tokio::task::Id, (String, u32, usize)> =
        std::collections::BTreeMap::new();
    let mut results = Vec::new();
    let spawn = |set: &mut JoinSet<RuleResult>,
                 (id, run, task, vectors): (String, u32, TaskSpec, VectorFile)| {
        let client = client.clone();
        let palette = palette.clone();
        let exemplars = exemplars.clone();
        let author_config = config.author.clone();
        let scenarios = vectors.scenarios.len();
        let rule = id.clone();
        let handle = set.spawn(async move {
            let trace = author(
                client.as_ref(),
                &palette,
                &task,
                &vectors,
                &exemplars,
                &author_config,
            )
            .await;
            RuleResult::from_trace(&id, run, vectors.scenarios.len(), trace)
        });
        (handle.id(), (rule, run, scenarios))
    };
    for job in pending.by_ref().take(config.concurrency.max(1)) {
        let (task_id, info) = spawn(&mut set, job);
        running.insert(task_id, info);
    }
    while let Some(done) = set.join_next_with_id().await {
        match done {
            Ok((task_id, result)) => {
                running.remove(&task_id);
                results.push(result);
            }
            Err(error) => {
                // Never let a crashed job shrink the denominator: record it as a failure.
                if let Some((rule, run, scenarios)) = running.remove(&error.id()) {
                    results.push(RuleResult::harness_failure(
                        &rule,
                        run,
                        scenarios,
                        &config.author,
                        &error.to_string(),
                    ));
                }
            }
        }
        if let Some(job) = pending.next() {
            let (task_id, info) = spawn(&mut set, job);
            running.insert(task_id, info);
        }
    }
    results.sort_by(|a, b| (&a.rule, a.run).cmp(&(&b.rule, b.run)));
    results
}
