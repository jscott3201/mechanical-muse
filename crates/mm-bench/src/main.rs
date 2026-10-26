//! `mm-bench`: run the fault-rule benchmark or inspect the scrubbed tasks.
//!
//! ```text
//! mm-bench --library <open-control-library checkout> --library-commit 124427b \
//!   [--subset test_clean|test_strict|test|train|<ID,ID,...>] [--tier a|b] \
//!   [--model muse-spark-1.3-contributor] [--effort medium] [--attempts 3] [--runs 1] \
//!   [--feedback validator-only|with-vectors] [--exemplars 0..8] [--concurrency 4] \
//!   [--out runs/<name>.json] [--dry-run] [--show <ID>]
//! ```
//! The key comes from `MODEL_API_KEY` (`make bench` maps the local `.env`). `--dry-run` makes no
//! model call: it scrubs every task, checks for leaks and computes the baselines.

#![forbid(unsafe_code)]
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow_free::{Error, bail};
use mm_agent::{AuthorConfig, Effort, Exemplar, Feedback, SparkClient, system_instructions};
use mm_bench::{
    BenchConfig, ENGINE_REV, RunRecord, Tier, baselines, interface_of, load_rule, load_split,
    prompt_hash, run_bench, summarize,
};
use mm_graph::{BLOCK_GRAPH_SCHEMA, Palette};

/// Minimal error plumbing for a binary without pulling in another crate.
mod anyhow_free {
    pub type Error = Box<dyn std::error::Error + Send + Sync>;
    macro_rules! bail {
        ($($t:tt)*) => { return Err(format!($($t)*).into()) };
    }
    pub(crate) use bail;
}

/// The checkout must be at the commit the record will claim.
fn check_commit(library: &std::path::Path, commit: &str) -> Result<(), Error> {
    let head = std::process::Command::new("git")
        .arg("-C")
        .arg(library)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| format!("cannot run git in the library checkout: {e}"))?;
    let head = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    if commit.len() < 7 || !head.starts_with(commit) {
        bail!("library checkout is at {head:?}, not {commit}");
    }
    Ok(())
}

struct Args(BTreeMap<String, String>);

impl Args {
    fn parse() -> Result<Self, Error> {
        let mut map = BTreeMap::new();
        let mut it = std::env::args().skip(1);
        while let Some(flag) = it.next() {
            let Some(name) = flag.strip_prefix("--") else {
                bail!("unexpected argument {flag}");
            };
            if name == "dry-run" {
                map.insert(name.to_owned(), "true".to_owned());
                continue;
            }
            let value = it.next().ok_or_else(|| format!("--{name} needs a value"))?;
            map.insert(name.to_owned(), value);
        }
        Ok(Self(map))
    }
    fn get(&self, name: &str, default: &str) -> String {
        self.0
            .get(name)
            .cloned()
            .unwrap_or_else(|| default.to_owned())
    }
    fn required(&self, name: &str) -> Result<String, Error> {
        self.0
            .get(name)
            .cloned()
            .ok_or_else(|| format!("--{name} is required").into())
    }
    fn number<T: std::str::FromStr>(&self, name: &str, default: T) -> Result<T, Error> {
        match self.0.get(name) {
            Some(v) => v
                .parse()
                .map_err(|_| format!("--{name}: not a number: {v}").into()),
            None => Ok(default),
        }
    }
}

fn effort(name: &str) -> Result<Option<Effort>, Error> {
    Ok(Some(match name {
        "none" | "" => return Ok(None),
        "minimal" => Effort::Minimal,
        "low" => Effort::Low,
        "medium" => Effort::Medium,
        "high" => Effort::High,
        "xhigh" => Effort::Xhigh,
        "max" => Effort::Max,
        other => bail!("unknown effort {other}"),
    }))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse()?;
    let tier = match args.get("tier", "a").as_str() {
        "a" => Tier::A,
        "b" => Tier::B,
        other => bail!("unknown tier {other}"),
    };
    let palette = Palette::pinned()?;
    let mut rules = Vec::new();
    let mut tasks = Vec::new();
    let mut skipped = Vec::new();
    let mut dropped_sentences = 0;
    let mut exemplars = Vec::new();
    let mut exemplar_ids = Vec::new();
    let exemplar_count: usize = args.number("exemplars", 0)?;
    let (subset, library_commit) = if let Some(spec) = args.0.get("spec") {
        // One of our own sequence specs: task.json and vectors.json, already validated against
        // a reference graph (crates/mm-bridge/tests/specs.rs). The task is ours, so no scrubbing.
        let dir = PathBuf::from(spec);
        let task: mm_agent::TaskSpec =
            serde_json::from_slice(&std::fs::read(dir.join("task.json"))?)?;
        let vectors: mm_bridge::VectorFile =
            serde_json::from_slice(&std::fs::read(dir.join("vectors.json"))?)?;
        if exemplar_count > 0 {
            bail!("--exemplars needs --library; spec runs are zero-shot for now");
        }
        tasks.push((task.name.clone(), task, vectors));
        (format!("spec:{spec}"), "none".to_owned())
    } else {
        let library = PathBuf::from(args.required("library")?);
        let library_commit = args.required("library-commit")?;
        check_commit(&library, &library_commit)?;
        let split = load_split(&PathBuf::from(
            args.get("split", "docs/benchmark-split.json"),
        ))?;
        let subset = args.get("subset", "test_clean");
        let ids: Vec<String> = match split.subset(&subset) {
            Some(ids) => ids.to_vec(),
            None => subset.split(',').map(str::to_owned).collect(),
        };
        // Scrub every task; a rule that would leak is skipped and listed, never hand-edited.
        for id in &ids {
            let rule = load_rule(&library, id)?;
            match rule.scrubbed(&palette, tier) {
                Ok(scrubbed) => {
                    dropped_sentences += scrubbed.dropped_sentences;
                    tasks.push((id.clone(), scrubbed.task, rule.vectors.clone()));
                    rules.push(rule);
                }
                Err(e) => skipped.push((id.clone(), e.to_string())),
            }
        }
        for id in split.few_shot_exemplars.iter().take(exemplar_count) {
            let rule = load_rule(&library, id)?;
            if let Ok(scrubbed) = rule.scrubbed(&palette, tier) {
                exemplars.push(Exemplar {
                    task: scrubbed.task,
                    graph: rule.exemplar_graph(),
                });
                exemplar_ids.push(id.clone());
            }
        }
        (subset, library_commit)
    };
    if let Some(show) = args.0.get("show") {
        match tasks.iter().find(|(id, _, _)| id == show) {
            Some((_, task, _)) => println!("{}", mm_agent::task_message(task)),
            None => println!("{show}: not in this subset, or skipped (see below)"),
        }
    }
    let items: Vec<_> = tasks
        .iter()
        .map(|(id, task, vectors)| (id.clone(), interface_of(task), vectors.clone()))
        .collect();

    let (constant_false, constant_true) = baselines(&items, &palette)?;
    println!(
        "subset {subset} tier {tier:?}: {} rules ({} skipped), {dropped_sentences} leaking \
         sentences removed",
        tasks.len(),
        skipped.len()
    );
    for (id, why) in &skipped {
        println!("  skipped {id}: {why}");
    }
    let scenarios = constant_false.scenarios;
    println!(
        "baselines: constant-false {} rules, {}/{scenarios} scenarios; constant-true {} rules, \
         {}/{scenarios} scenarios",
        constant_false.rules_passed,
        constant_false.scenarios_passed,
        constant_true.rules_passed,
        constant_true.scenarios_passed,
    );
    if args.0.contains_key("dry-run") {
        return Ok(());
    }

    let feedback = match args.get("feedback", "validator-only").as_str() {
        "validator-only" => Feedback::ValidatorOnly,
        "with-vectors" => Feedback::WithVectors,
        other => bail!("unknown feedback {other}"),
    };
    let config = BenchConfig {
        author: AuthorConfig {
            model: args.get("model", "muse-spark-1.3-contributor"),
            effort: effort(&args.get("effort", "medium"))?,
            max_output_tokens: args.number("max-output-tokens", 32_000)?,
            max_attempts: args.number("attempts", 3)?,
            feedback,
        },
        tier,
        subset: subset.clone(),
        runs: args.number("runs", 1)?,
        exemplar_ids,
        concurrency: args.number("concurrency", 4)?,
        library_commit,
    };
    let out = PathBuf::from(args.required("out")?);
    let key = std::env::var("MODEL_API_KEY").map_err(|_| "MODEL_API_KEY is not set")?;
    let client = Arc::new(SparkClient::new(key)?);
    let hash = prompt_hash(&format!(
        "{}\n{BLOCK_GRAPH_SCHEMA}",
        system_instructions(&palette, &exemplars)
    ));
    let rule_ids: Vec<String> = tasks.iter().map(|(id, _, _)| id.clone()).collect();
    let results = run_bench(
        client,
        Arc::new(palette),
        tasks,
        Arc::new(exemplars),
        &config,
    )
    .await;
    let summary = summarize(&results, config.runs);
    for s in &summary {
        println!(
            "run {}: first try {}/{} rules, after repair {}/{} rules, best scenarios {}/{}, \
             tokens in {} out {}; first failures {:?}",
            s.run,
            s.first_try_rule_passes,
            s.rules,
            s.final_rule_passes,
            s.rules,
            s.scenarios_passed_best,
            s.scenarios,
            s.usage.input_tokens,
            s.usage.output_tokens,
            s.first_failures
        );
    }
    let record = RunRecord {
        schema: "mechanical-muse/bench-run/v1",
        config,
        engine_rev: ENGINE_REV,
        prompt_hash: hash,
        rules: rule_ids,
        skipped,
        baseline_constant_false: constant_false,
        baseline_constant_true: constant_true,
        summary,
        results,
    };
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&out, serde_json::to_vec_pretty(&record)?)?;
    println!("wrote {}", out.display());
    Ok(())
}
