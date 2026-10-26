//! Reading a library rule card (`card.md`): YAML front matter plus Markdown sections. Only the
//! fields a scrubbed task may use are kept; `cxf` paths, diagrams, Deviations and Notes are
//! never read into memory here.

use std::collections::BTreeMap;

use mm_graph::ParamValue;
use serde::Deserialize;

use crate::BenchError;

/// The parts of a card a task may draw on.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub points: Vec<String>,
    pub outputs: Vec<CardOutput>,
    pub params: BTreeMap<String, CardParam>,
    /// The "Description" section.
    pub description: String,
    /// The first fenced block in "Detection Logic" (pseudo-code), if any.
    pub logic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CardOutput {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardParam {
    pub default: ParamValue,
    pub unit: String,
    pub description: String,
}

#[derive(Deserialize)]
struct Front {
    id: String,
    name: String,
    points: Vec<String>,
    outputs: Vec<CardOutput>,
    #[serde(default)]
    params: BTreeMap<String, FrontParam>,
}

#[derive(Deserialize)]
struct FrontParam {
    default: serde_yaml_ng::Value,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    description: String,
}

pub fn parse_card(text: &str) -> Result<Card, BenchError> {
    let bad = |why: &str| BenchError::Card(why.to_owned());
    let rest = text
        .strip_prefix("---\n")
        .ok_or_else(|| bad("no front matter"))?;
    let (front, body) = rest
        .split_once("\n---\n")
        .ok_or_else(|| bad("unterminated front matter"))?;
    let front: Front =
        serde_yaml_ng::from_str(front).map_err(|e| bad(&format!("front matter: {e}")))?;
    let mut params = BTreeMap::new();
    for (name, p) in front.params {
        let default = match &p.default {
            serde_yaml_ng::Value::Bool(b) => ParamValue::Bool(*b),
            serde_yaml_ng::Value::Number(n) => {
                ParamValue::Number(n.as_f64().ok_or_else(|| bad("param default"))?)
            }
            _ => return Err(bad(&format!("param {name} has a non-scalar default"))),
        };
        params.insert(
            name,
            CardParam {
                default,
                unit: p.unit.unwrap_or_default(),
                description: p.description,
            },
        );
    }
    let sections = sections(body);
    let description = sections
        .get("Description")
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
    let logic = sections.get("Detection Logic").and_then(|s| first_fence(s));
    Ok(Card {
        id: front.id,
        name: front.name,
        points: front.points,
        outputs: front.outputs,
        params,
        description,
        logic,
    })
}

/// `## Heading` sections of a Markdown body, keyed by heading text.
fn sections(body: &str) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in body.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            current = Some(title.trim().to_owned());
            continue;
        }
        if let Some(title) = &current {
            let text = out.entry(title.clone()).or_default();
            text.push_str(line);
            text.push('\n');
        }
    }
    out
}

fn first_fence(section: &str) -> Option<String> {
    let mut lines = section
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("```"));
    lines.next()?;
    let body: Vec<&str> = lines
        .take_while(|l| !l.trim_start().starts_with("```"))
        .collect();
    let text = body.join("\n").trim().to_owned();
    (!text.is_empty()).then_some(text)
}
