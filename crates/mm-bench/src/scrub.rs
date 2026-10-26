//! Turning a card into a task the model may see. The card is both the problem statement and a
//! partial answer, so anything that names a block class or one of the rule's own block ids is
//! removed, sentence by sentence, and a final check refuses any task that still leaks.

use std::collections::{BTreeMap, BTreeSet};

use mm_agent::{ParamHint, PointSpec, TaskSpec};
use mm_graph::{BlockGraph, Palette};
use serde::{Deserialize, Serialize};

use crate::BenchError;
use crate::card::Card;

/// Tier A: prose, points and parameters. Tier B adds the card's pseudo-code, which is close to a
/// direct specification, so results must name the tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    A,
    B,
}

/// One entry of a family point dictionary (`points/<family>.points.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct PointInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub unit: String,
}

#[derive(Deserialize)]
struct PointFile {
    points: Vec<PointInfo>,
}

pub fn parse_points(json: &[u8]) -> Result<BTreeMap<String, PointInfo>, BenchError> {
    let file: PointFile = serde_json::from_slice(json)?;
    Ok(file
        .points
        .into_iter()
        .map(|p| (p.name.clone(), p))
        .collect())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scrubbed {
    pub task: TaskSpec,
    /// Sentences removed from prose because they named a class or a block id.
    pub dropped_sentences: usize,
    /// False when the pseudo-code leaks, so the rule cannot run at tier B.
    pub tier_b_available: bool,
}

/// Words that would hand the model part of the answer: class names (full paths and distinctive
/// short names), the rule's own camelCase block ids, and implementation vocabulary.
///
/// Plain short class names such as `And`, `Not` or `Switch` are not on the list: they are
/// ordinary words in prose. All-lowercase block ids (`persist`, `gap`) only count when written
/// as code (in backticks), for the same reason.
pub fn leak_tokens(palette: &Palette, graph: &BlockGraph) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    for class in palette.classes() {
        tokens.insert(class.class.clone());
        let short = class.class.rsplit('.').next().unwrap_or_default();
        let capitals = short.chars().filter(char::is_ascii_uppercase).count();
        if capitals >= 2 || ["Hysteresis", "Limiter", "Sampler", "Latch"].contains(&short) {
            tokens.insert(short.to_owned());
        }
    }
    for block in &graph.blocks {
        let id = &block.id;
        if id
            .chars()
            .any(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            tokens.insert(id.clone());
        } else {
            tokens.insert(format!("`{id}`"));
        }
    }
    for word in ["CDL", "Buildings.Controls", "cxf", "CXF", "delayOnInit"] {
        tokens.insert(word.to_owned());
    }
    tokens.into_iter().collect()
}

/// Tokens found in `text` as whole words (backticked tokens match literally).
pub fn find_leaks(text: &str, tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .filter(|t| contains_word(text, t))
        .cloned()
        .collect()
}

fn contains_word(text: &str, token: &str) -> bool {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(token).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + token.len()..].chars().next();
        let edge = |c: Option<char>, t: Option<char>| match (c, t) {
            (Some(c), Some(t)) if is_word(t) => !is_word(c),
            _ => true,
        };
        edge(before, token.chars().next()) && edge(after, token.chars().next_back())
    })
}

/// Splits prose into sentences, keeps those without leaks, and joins them on one line.
fn redact(text: &str, tokens: &[String]) -> (String, usize) {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut kept = Vec::new();
    let mut dropped = 0;
    // A sentence ends at '.', '!' or '?' followed by a space or the end of the text.
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = flat.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if matches!(c, '.' | '!' | '?') && chars.peek().is_none_or(|n| n.is_whitespace()) {
            sentences.push(std::mem::take(&mut current));
        }
    }
    sentences.push(current);
    for sentence in sentences {
        let sentence = sentence.trim().to_owned();
        if sentence.is_empty() {
            continue;
        }
        if find_leaks(&sentence, tokens).is_empty() {
            kept.push(sentence);
        } else {
            dropped += 1;
        }
    }
    (kept.join(" "), dropped)
}

/// Builds the task for one rule. `interface` is the rule's own graph, used only for boundary
/// names and types and to know which block ids must not appear.
pub fn scrub(
    card: &Card,
    points: &BTreeMap<String, PointInfo>,
    interface: &BlockGraph,
    palette: &Palette,
    tier: Tier,
) -> Result<Scrubbed, BenchError> {
    let tokens = leak_tokens(palette, interface);
    let mut dropped = 0;
    let mut clean = |text: &str| {
        let (kept, n) = redact(text, &tokens);
        dropped += n;
        kept
    };

    let description = clean(&card.description);
    let mut inputs = Vec::new();
    for input in &interface.inputs {
        let info = points.get(&input.name);
        inputs.push(PointSpec {
            name: input.name.clone(),
            ty: input.ty,
            unit: info.map(|p| p.unit.clone()).unwrap_or_default(),
            meaning: clean(info.map_or("", |p| p.description.as_str())),
        });
    }
    let mut outputs = Vec::new();
    for output in &interface.outputs {
        let meaning = card
            .outputs
            .iter()
            .find(|o| o.name == output.name)
            .map_or("", |o| o.description.as_str());
        outputs.push(PointSpec {
            name: output.name.clone(),
            ty: output.ty,
            unit: String::new(),
            meaning: clean(meaning),
        });
    }
    let mut params = Vec::new();
    for (name, p) in &card.params {
        params.push(ParamHint {
            name: name.clone(),
            value: p.default.clone(),
            unit: p.unit.clone(),
            meaning: clean(&p.description),
        });
    }
    let logic_leaks = card
        .logic
        .as_deref()
        .map(|l| find_leaks(l, &tokens))
        .unwrap_or_default();
    let tier_b_available = card.logic.is_some() && logic_leaks.is_empty();
    let logic = match tier {
        Tier::B if tier_b_available => card.logic.clone(),
        Tier::B => {
            return Err(BenchError::Leak {
                rule: card.id.clone(),
                tokens: logic_leaks,
            });
        }
        Tier::A => None,
    };
    let task = TaskSpec {
        name: snake(&card.id),
        description: format!("{}. {description}", card.name),
        inputs,
        outputs,
        params,
        logic,
    };

    // Last line of defence: the whole task, as the model will see it, must be clean.
    let leaks = find_leaks(&mm_agent::task_message(&task), &tokens);
    if !leaks.is_empty() {
        return Err(BenchError::Leak {
            rule: card.id.clone(),
            tokens: leaks,
        });
    }
    Ok(Scrubbed {
        task,
        dropped_sentences: dropped,
        tier_b_available,
    })
}

/// `AHU-0005` -> `ahu_0005`.
pub fn snake(id: &str) -> String {
    id.to_ascii_lowercase().replace('-', "_")
}
