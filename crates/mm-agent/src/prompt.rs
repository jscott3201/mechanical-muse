//! Prompts. The block palette comes from the pinned catalog and the schema's class list, never
//! from hand-typed text. A task is a scrubbed spec (`TaskSpec`): point names, meanings,
//! parameter hints and, for the easier tier, pseudo-code. It must never carry a graph, class
//! names from the answer, or test vectors (`mm-benchmark-hygiene`).

use mm_graph::{BLOCK_GRAPH_SCHEMA, BlockGraph, Palette, ParamKind, ParamValue, SignalType};
use serde::{Deserialize, Serialize};

/// What the model is asked to build, in our own words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSpec {
    /// snake_case sequence name.
    pub name: String,
    pub description: String,
    pub inputs: Vec<PointSpec>,
    pub outputs: Vec<PointSpec>,
    #[serde(default)]
    pub params: Vec<ParamHint>,
    /// Optional pseudo-code ("tier B"); without it the model works from prose ("tier A").
    #[serde(default)]
    pub logic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: SignalType,
    #[serde(default)]
    pub unit: String,
    pub meaning: String,
}

/// A threshold or timing the sequence should use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamHint {
    pub name: String,
    pub value: ParamValue,
    #[serde(default)]
    pub unit: String,
    pub meaning: String,
}

/// A worked example: a task and a graph that passes its vectors. Train-split rules only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exemplar {
    pub task: TaskSpec,
    pub graph: BlockGraph,
}

const RULES: &str = "\
You write building control sequences as compact CDL block graphs. Reply with one JSON object \
that follows the block_graph schema, and nothing else.

Rules:
1. Interface: use exactly the task's input and output names and types. Copy each input's unit \
and meaning into `unit` and `description`, and each output's meaning into `description`.
2. Blocks: `class` is one of the classes listed below. Block ids are camelCase, contain no \
dots, and differ from each other and from every input and output name. Input and output names must not look like ports (u, y, u1, y2).
3. Connections are strings `\"<src> -> <dst>\"`. A source is a task input name or \
`<blockId>.<outputPort>`; a destination is a task output name or `<blockId>.<inputPort>`.
4. Every block input port and every task output has exactly one incoming connection. A block \
output may feed many destinations. Every task input must feed at least one destination.
5. Port order matters: Reals.Subtract gives u1 - u2, Reals.Divide gives u1 / u2, and \
Reals.Greater / Reals.Less compare u1 against u2.
6. Parameters: list only the ones you set, and always list the required ones (marked *). Real \
and Integer values are JSON numbers, Boolean values are true or false. Leave enum-valued \
parameters at their defaults.
7. Convention: set delayOnInit = true on every Logical.TrueDelay, so a condition that is \
already true at start-up still has to persist for the full delay.
8. Use the task's parameter values exactly. Do not add behavior the task does not ask for.
9. Feedback: a path from a block's output back to its own input (a stored value, a hold that \
clears itself) must pass through Discrete.UnitDelay (Real) or Logical.Pre (Boolean), which \
output the previous step's value. Sampler, ZeroOrderHold, Latch, TrueDelay and TrueFalseHold \
pass the current value through, so a loop through them alone is refused.";

/// The authorable classes the schema offers, in schema order.
pub(crate) fn schema_classes() -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(BLOCK_GRAPH_SCHEMA)
        .ok()
        .and_then(|schema| {
            schema
                .pointer("/$defs/block/properties/class/enum")?
                .as_array()
                .map(|classes| {
                    classes
                        .iter()
                        .filter_map(|c| c.as_str().map(str::to_owned))
                        .collect()
                })
        })
        .unwrap_or_default()
}

/// One line per schema class: ports in declaration order and parameters (`*` = required).
pub fn palette_listing(palette: &Palette) -> String {
    let mut out = String::new();
    for class in schema_classes() {
        let Some(spec) = palette.class(&class) else {
            continue;
        };
        let ports = |ports: &[mm_graph::PortSpec]| {
            ports
                .iter()
                .map(|p| format!("{}: {}", p.name, p.kind.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let params: Vec<String> = spec
            .params
            .iter()
            .map(|(name, p)| {
                let kind = match p.kind {
                    ParamKind::Real => "Real",
                    ParamKind::Integer => "Integer",
                    ParamKind::Boolean => "Boolean",
                    ParamKind::Enum | ParamKind::String | ParamKind::Unspecified => "leave default",
                };
                let star = if p.required { "*" } else { "" };
                format!("{name}{star} ({kind})")
            })
            .collect();
        let params = if params.is_empty() {
            "none".to_owned()
        } else {
            params.join(", ")
        };
        out.push_str(&format!(
            "- {class}: in({}) -> out({}); params: {params}\n",
            ports(&spec.inputs),
            ports(&spec.outputs)
        ));
    }
    out
}

/// The stable prefix: rules, palette, then worked examples.
pub fn system_instructions(palette: &Palette, exemplars: &[Exemplar]) -> String {
    let mut out = format!("{RULES}\n\nBlock classes:\n{}", palette_listing(palette));
    for (i, example) in exemplars.iter().enumerate() {
        let answer = serde_json::to_string(&example.graph).unwrap_or_default();
        out.push_str(&format!(
            "\nExample {}\nTask:\n{}\nAnswer:\n{answer}\n",
            i + 1,
            task_message(&example.task)
        ));
    }
    out
}

/// The task as the user turn.
pub fn task_message(task: &TaskSpec) -> String {
    let point = |p: &PointSpec| {
        let ty = match p.ty {
            SignalType::Bool => "bool",
            SignalType::Real => "real",
            SignalType::Int => "int",
        };
        let unit = if p.unit.is_empty() {
            String::new()
        } else {
            format!(", {}", p.unit)
        };
        format!("- {} ({ty}{unit}): {}\n", p.name, p.meaning)
    };
    let mut out = format!("Sequence: {}\n{}\n\nInputs:\n", task.name, task.description);
    task.inputs.iter().for_each(|p| out.push_str(&point(p)));
    out.push_str("\nOutputs:\n");
    task.outputs.iter().for_each(|p| out.push_str(&point(p)));
    if !task.params.is_empty() {
        out.push_str("\nParameters:\n");
        for p in &task.params {
            let value = match &p.value {
                ParamValue::Bool(b) => b.to_string(),
                ParamValue::Number(x) => x.to_string(),
                ParamValue::Text(s) => s.clone(),
            };
            let unit = if p.unit.is_empty() {
                String::new()
            } else {
                format!(" {}", p.unit)
            };
            out.push_str(&format!("- {} = {value}{unit}: {}\n", p.name, p.meaning));
        }
    }
    if let Some(logic) = &task.logic {
        out.push_str(&format!(
            "\nPseudo-code to turn into a block graph:\n```\n{logic}\n```\n"
        ));
    }
    out
}
