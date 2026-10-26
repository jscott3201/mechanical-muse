//! Loading rules and the fixed split from a library checkout (layout
//! `faults/<family>/<ID>/{card.md,rule.cxf.jsonld,vectors.json}`, `points/<family>.points.json`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mm_bridge::VectorFile;
use mm_graph::{BlockGraph, Palette, cxf_to_graph};
use serde::Deserialize;

use crate::BenchError;
use crate::card::{Card, parse_card};
use crate::scrub::{PointInfo, Scrubbed, Tier, parse_points, scrub, snake};

/// Everything the harness needs about one rule. `reference` is the library's own graph: it gives
/// the interface and the baselines, and it is the answer, so it never reaches a prompt (except
/// for train-split exemplars).
#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    pub card: Card,
    pub reference: BlockGraph,
    pub vectors: VectorFile,
    pub points: BTreeMap<String, PointInfo>,
}

impl Rule {
    pub fn scrubbed(&self, palette: &Palette, tier: Tier) -> Result<Scrubbed, BenchError> {
        scrub(&self.card, &self.points, &self.reference, palette, tier)
    }

    /// The reference graph named like the task, for use as a few-shot exemplar.
    pub fn exemplar_graph(&self) -> BlockGraph {
        let mut graph = self.reference.clone();
        graph.name = snake(&self.id);
        graph
    }
}

fn read(path: &Path) -> Result<Vec<u8>, BenchError> {
    std::fs::read(path).map_err(|source| BenchError::Io {
        path: path.display().to_string(),
        source,
    })
}

/// Loads one rule from explicit file paths (the fixture layout is flat).
pub fn load_rule_files(
    id: &str,
    card: &Path,
    cxf: &Path,
    vectors: &Path,
    points: &Path,
) -> Result<Rule, BenchError> {
    let card_text = String::from_utf8(read(card)?)
        .map_err(|_| BenchError::Card(format!("{id}: card is not UTF-8")))?;
    Ok(Rule {
        id: id.to_owned(),
        card: parse_card(&card_text)?,
        reference: cxf_to_graph(&read(cxf)?)?.graph,
        vectors: serde_json::from_slice(&read(vectors)?)?,
        points: parse_points(&read(points)?)?,
    })
}

/// Loads `id` (e.g. `AHU-0005`) from a library checkout.
pub fn load_rule(library: &Path, id: &str) -> Result<Rule, BenchError> {
    let family = id
        .split('-')
        .next()
        .ok_or_else(|| BenchError::Other(format!("bad rule id {id}")))?
        .to_ascii_lowercase();
    let dir: PathBuf = library.join("faults").join(&family).join(id);
    load_rule_files(
        id,
        &dir.join("card.md"),
        &dir.join("rule.cxf.jsonld"),
        &dir.join("vectors.json"),
        &library.join("points").join(format!("{family}.points.json")),
    )
}

/// `docs/benchmark-split.json`. Never re-rolled or re-sorted.
#[derive(Debug, Clone, Deserialize)]
pub struct Split {
    pub seed: u64,
    pub test: Vec<String>,
    pub train: Vec<String>,
    pub test_clean_no_param_identical_train_twin: Vec<String>,
    pub test_strict_no_skeleton_train_twin: Vec<String>,
    pub few_shot_exemplars: Vec<String>,
}

impl Split {
    /// Rule ids for a named subset: `test`, `train`, `test_clean`, `test_strict`.
    pub fn subset(&self, name: &str) -> Option<&[String]> {
        match name {
            "test" => Some(&self.test),
            "train" => Some(&self.train),
            "test_clean" => Some(&self.test_clean_no_param_identical_train_twin),
            "test_strict" => Some(&self.test_strict_no_skeleton_train_twin),
            _ => None,
        }
    }
}

pub fn load_split(path: &Path) -> Result<Split, BenchError> {
    Ok(serde_json::from_slice(&read(path)?)?)
}
