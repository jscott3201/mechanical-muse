//! Spark's own checks while it drafts: `check_graph`, `replay_scenario` and `describe_block`.
//! Each runs our deterministic checks (the same `mm-graph` validator and emitter and `mm-bridge`
//! engine load and replay the receipt uses) on a graph the model passes in. They only help the
//! model draft: a tool result never counts as verification (AGENTS.md hard rule 5). The receipt
//! is still the full replay of every scenario on a fresh engine after the draft is final.
//!
//! Arguments are model output, so every one is checked before use, and every result is capped
//! in size. Only the product's specs and workbench tasks offer these tools; benchmark runs
//! never do (their vectors must stay blind).

use mm_bridge::{ReplayLimits, ScenarioOutcome, VectorFile, load_cxf, replay};
use mm_graph::{BLOCK_GRAPH_SCHEMA, BlockGraph, EmitOptions, Palette, emit_cxf, validate};
use serde_json::{Value, json};

use crate::feedback::{inputs_at, load_failed};
use crate::prompt::{TaskSpec, schema_classes};
use crate::repair::interface_problems;

pub const CHECK_GRAPH: &str = "check_graph";
pub const REPLAY_SCENARIO: &str = "replay_scenario";
pub const DESCRIBE_BLOCK: &str = "describe_block";

/// Largest `arguments` string we parse (a block graph is a few kilobytes).
const MAX_ARGUMENT_BYTES: usize = 256 * 1024;
/// Largest tool output sent back to the model, in bytes.
pub const MAX_OUTPUT_BYTES: usize = 6_000;
/// Problems listed per result; the rest are counted.
const MAX_PROBLEMS: usize = 12;

/// Budgets for the tool loop. Each attempt may use at most `max_turns` model calls that ask
/// for tools; the whole session at most `max_calls` tool calls, of which at most `max_lookups`
/// `describe_block` (the prompt already lists every class; live, Spark looked up almost every
/// class it used and spent its budget before replaying anything).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ToolLimits {
    pub max_turns: u32,
    pub max_calls: u32,
    pub max_lookups: u32,
}

impl Default for ToolLimits {
    fn default() -> Self {
        Self {
            max_turns: 8,
            max_calls: 24,
            max_lookups: 4,
        }
    }
}

/// What one tool call did: the text the model gets back, and a one-line summary for people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub ok: bool,
    pub output: String,
    pub summary: String,
}

/// The tools for one task: its palette, interface and accepted scenarios.
pub struct DraftTools<'a> {
    palette: &'a Palette,
    task: &'a TaskSpec,
    vectors: &'a VectorFile,
}

impl<'a> DraftTools<'a> {
    pub fn new(palette: &'a Palette, task: &'a TaskSpec, vectors: &'a VectorFile) -> Self {
        Self {
            palette,
            task,
            vectors,
        }
    }

    /// Function tool definitions in the Responses shape, in a fixed order.
    pub fn definitions(&self) -> Vec<Value> {
        let schema: Value = serde_json::from_str(BLOCK_GRAPH_SCHEMA).unwrap_or(Value::Null);
        let defs = schema.get("$defs").cloned().unwrap_or_else(|| json!({}));
        // The graph schema without its root keywords; its `$ref`s point at `#/$defs`, which
        // moves to the parameters' root.
        let mut graph = schema;
        if let Some(object) = graph.as_object_mut() {
            for key in ["$schema", "$defs", "title"] {
                object.remove(key);
            }
            object.insert(
                "description".into(),
                json!("Your current draft, in the block_graph format."),
            );
        }
        let scenarios: Vec<&str> = self
            .vectors
            .scenarios
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        let with_graph = |extra: Option<(&str, Value)>| {
            let mut properties = serde_json::Map::new();
            properties.insert("graph".into(), graph.clone());
            let mut required = vec![json!("graph")];
            if let Some((name, schema)) = extra {
                properties.insert(name.into(), schema);
                required.push(json!(name));
            }
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": properties,
                "required": required,
                "$defs": defs,
            })
        };
        vec![
            json!({
                "type": "function",
                "name": CHECK_GRAPH,
                "description": "Checks a draft without running it: interface against the task, \
                    validator (classes, parameters, connections, drivers), CXF emission and \
                    engine load. Returns ok, or the stage and typed problems.",
                "strict": true,
                "parameters": with_graph(None),
            }),
            json!({
                "type": "function",
                "name": REPLAY_SCENARIO,
                "description": "Replays one of the task's test scenarios on a fresh engine. \
                    Returns pass, or the first failing instant with the output, expected and \
                    actual values and the inputs at that instant. A pass here is your own \
                    check; the final answer is verified separately on every scenario.",
                "strict": true,
                "parameters": with_graph(Some((
                    "scenario",
                    json!({"type": "string", "enum": scenarios,
                           "description": "Scenario name"}),
                ))),
            }),
            json!({
                "type": "function",
                "name": DESCRIBE_BLOCK,
                "description": "Describes one block class from the pinned catalog: input and \
                    output ports in order with their types, parameters (required or not, and \
                    type) and whether it keeps state.",
                "strict": true,
                "parameters": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {"class": {"type": "string",
                        "description": "Class path such as Reals.Line or Logical.TrueDelay"}},
                    "required": ["class"],
                },
            }),
        ]
    }

    /// Runs tool `name` on the raw `arguments` string. Never panics on bad input: a refused
    /// argument is a result the model can read.
    pub fn call(&self, name: &str, arguments: &str) -> ToolResult {
        let result = match self.parse(arguments) {
            Err(message) => refused(&message),
            Ok(args) => match name {
                CHECK_GRAPH => self.check(&args),
                REPLAY_SCENARIO => self.replay_one(&args),
                DESCRIBE_BLOCK => self.describe(&args),
                _ => refused(&format!("no tool named {}", clip(name, 64))),
            },
        };
        cap(result)
    }

    fn parse(&self, arguments: &str) -> Result<Value, String> {
        if arguments.len() > MAX_ARGUMENT_BYTES {
            return Err(format!(
                "arguments are {} bytes; the limit is {MAX_ARGUMENT_BYTES}",
                arguments.len()
            ));
        }
        match serde_json::from_str::<Value>(arguments) {
            Ok(v @ Value::Object(_)) => Ok(v),
            Ok(_) => Err("arguments must be a JSON object".into()),
            Err(e) => Err(format!("arguments are not JSON: {e}")),
        }
    }

    fn graph(&self, args: &Value) -> Result<BlockGraph, String> {
        let raw = args.get("graph").ok_or("missing argument: graph")?;
        let parsed = match raw {
            // Tolerate a graph sent as a JSON string.
            Value::String(text) => serde_json::from_str(text),
            other => serde_json::from_value(other.clone()),
        };
        let mut graph: BlockGraph =
            parsed.map_err(|e| format!("graph is not in the block_graph format: {e}"))?;
        // The loop names the sequence after the task; engine subjects use that name.
        graph.name.clone_from(&self.task.name);
        Ok(graph)
    }

    /// Interface, validator, emitter, engine load. `Ok` carries the CXF.
    fn static_checks(&self, graph: &BlockGraph) -> Result<Vec<u8>, ToolResult> {
        let problems = interface_problems(graph, self.task);
        if !problems.is_empty() {
            let shown: Vec<&String> = problems.iter().take(MAX_PROBLEMS).collect();
            return Err(ToolResult {
                ok: false,
                summary: format!("interface: {}", clip(&problems.join("; "), 120)),
                output: json!({"ok": false, "stage": "interface", "problems": shown,
                               "more": problems.len().saturating_sub(MAX_PROBLEMS)})
                .to_string(),
            });
        }
        let findings = validate(graph, self.palette);
        if let Some(first) = findings.first() {
            let shown: Vec<Value> = findings
                .iter()
                .take(MAX_PROBLEMS)
                .map(|f| json!({"code": f.code.as_str(), "subject": f.subject, "detail": f.detail}))
                .collect();
            return Err(ToolResult {
                ok: false,
                summary: format!(
                    "validator: {} finding(s), first {} at {}",
                    findings.len(),
                    first.code.as_str(),
                    clip(&first.subject, 40)
                ),
                output: json!({"ok": false, "stage": "validate", "problems": shown,
                               "more": findings.len().saturating_sub(MAX_PROBLEMS)})
                .to_string(),
            });
        }
        let cxf = emit_cxf(graph, self.palette, &EmitOptions::default()).map_err(|e| {
            let detail = e.to_string();
            ToolResult {
                ok: false,
                summary: format!("emit failed: {}", clip(&detail, 100)),
                output: json!({"ok": false, "stage": "emit", "detail": detail}).to_string(),
            }
        })?;
        if let Err(failure) = load_cxf(&cxf) {
            let mut text = String::new();
            load_failed(&mut text, &failure);
            return Err(ToolResult {
                ok: false,
                summary: format!("engine refused it: {} ({})", failure.code, failure.stage),
                output: json!({"ok": false, "stage": failure.stage, "code": failure.code,
                               "problems": text})
                .to_string(),
            });
        }
        Ok(cxf)
    }

    fn check(&self, args: &Value) -> ToolResult {
        let graph = match self.graph(args) {
            Ok(graph) => graph,
            Err(message) => return refused(&message),
        };
        match self.static_checks(&graph) {
            Ok(_) => ToolResult {
                ok: true,
                summary: "checks pass: interface, validator, emitter, engine load".into(),
                output: json!({"ok": true, "stage": "load",
                    "note": "Structure is fine. Behavior is not checked here."})
                .to_string(),
            },
            Err(result) => result,
        }
    }

    fn replay_one(&self, args: &Value) -> ToolResult {
        let Some(name) = args.get("scenario").and_then(Value::as_str) else {
            return refused("missing argument: scenario (a string)");
        };
        let Some(scenario) = self.vectors.scenarios.iter().find(|s| s.name == name) else {
            let known: Vec<&str> = self
                .vectors
                .scenarios
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            return refused(&format!(
                "no scenario named {}; the task has: {}",
                clip(name, 64),
                known.join(", ")
            ));
        };
        let graph = match self.graph(args) {
            Ok(graph) => graph,
            Err(message) => return refused(&message),
        };
        let cxf = match self.static_checks(&graph) {
            Ok(cxf) => cxf,
            Err(result) => return result,
        };
        let one = VectorFile {
            schema: self.vectors.schema.clone(),
            clock: self.vectors.clock,
            scenarios: vec![scenario.clone()],
        };
        let report = match replay(&cxf, &one, &ReplayLimits::default()) {
            Ok(report) => report,
            Err(e) => {
                return ToolResult {
                    ok: false,
                    summary: format!("replay could not run: {}", clip(&e.to_string(), 100)),
                    output: json!({"ok": false, "stage": "replay", "detail": e.to_string()})
                        .to_string(),
                };
            }
        };
        let outcome = report.scenarios.first().map(|s| s.outcome.clone());
        match outcome {
            Some(ScenarioOutcome::Pass) => ToolResult {
                ok: true,
                summary: format!("scenario {name} passed"),
                output: json!({"ok": true, "scenario": name, "result": "pass"}).to_string(),
            },
            Some(ScenarioOutcome::Fail {
                t,
                output,
                expected,
                actual,
                from_s,
                to_s,
            }) => {
                let wire = |v: &mm_bridge::WireValue| serde_json::to_value(v).unwrap_or_default();
                ToolResult {
                    ok: false,
                    summary: format!(
                        "scenario {name} failed at {t} s: {output} expected {}, got {}",
                        wire(&expected),
                        wire(&actual)
                    ),
                    output: json!({
                        "ok": false, "scenario": name, "result": "fail",
                        "description": scenario.description,
                        "t_s": t, "output": output,
                        "expected": wire(&expected), "actual": wire(&actual),
                        "checked_from_s": from_s, "checked_to_s": to_s,
                        "inputs_at_t": inputs_at(scenario, t),
                    })
                    .to_string(),
                }
            }
            Some(ScenarioOutcome::Error { code, detail, t }) => ToolResult {
                ok: false,
                summary: format!("scenario {name} stopped: {code}"),
                output: json!({"ok": false, "scenario": name, "result": "error",
                               "code": code, "detail": detail, "t_s": t})
                .to_string(),
            },
            None => refused("the replay returned no scenario"),
        }
    }

    fn describe(&self, args: &Value) -> ToolResult {
        let Some(class) = args.get("class").and_then(Value::as_str) else {
            return refused("missing argument: class (a string)");
        };
        let class = class.trim().trim_start_matches("CDL.");
        let offered = schema_classes();
        let spec = self
            .palette
            .class(class)
            .filter(|_| offered.iter().any(|c| c == class));
        let Some(spec) = spec else {
            let needle = class.to_lowercase();
            let tail = needle.rsplit('.').next().unwrap_or(&needle).to_owned();
            let similar: Vec<&String> = offered
                .iter()
                .filter(|c| !tail.is_empty() && c.to_lowercase().contains(&tail))
                .take(8)
                .collect();
            return ToolResult {
                ok: false,
                summary: format!("{} is not an offered class", clip(class, 64)),
                output: json!({"ok": false, "detail": "not an offered class",
                               "similar": similar})
                .to_string(),
            };
        };
        let ports = |ports: &[mm_graph::PortSpec]| -> Vec<Value> {
            ports
                .iter()
                .map(|p| json!({"name": p.name, "type": p.kind.as_str()}))
                .collect()
        };
        let params: Vec<Value> = spec
            .params
            .iter()
            .map(|(name, p)| {
                let kind = match p.kind {
                    mm_graph::ParamKind::Real => "Real",
                    mm_graph::ParamKind::Integer => "Integer",
                    mm_graph::ParamKind::Boolean => "Boolean",
                    _ => "leave default",
                };
                json!({"name": name, "type": kind, "required": p.required})
            })
            .collect();
        ToolResult {
            ok: true,
            summary: format!("described {}", spec.class),
            output: json!({
                "ok": true, "class": spec.class,
                "inputs": match spec.width {
                    Some(kind) => json!([{"name": format!("u1..u<{}>", mm_graph::WIDTH_PARAM),
                                          "type": kind.as_str(),
                                          "note": format!("one input per count of {}: u1, u2, ... u<{}>",
                                                          mm_graph::WIDTH_PARAM, mm_graph::WIDTH_PARAM)}]),
                    None => json!(ports(&spec.inputs)),
                },
                "outputs": ports(&spec.outputs),
                "params": params, "keeps_state": spec.stateful,
            })
            .to_string(),
        }
    }
}

fn refused(message: &str) -> ToolResult {
    ToolResult {
        ok: false,
        summary: format!("refused: {}", clip(message, 100)),
        output: json!({"ok": false, "error": "invalid-arguments", "detail": clip(message, 600)})
            .to_string(),
    }
}

/// Keeps the output within [`MAX_OUTPUT_BYTES`]: an oversized result becomes its summary.
fn cap(mut result: ToolResult) -> ToolResult {
    if result.output.len() > MAX_OUTPUT_BYTES {
        result.output = json!({"ok": result.ok, "summary": result.summary,
                               "truncated": true})
        .to_string();
    }
    result.summary = clip(&result.summary, 200);
    result
}

/// At most `max` characters, with an ellipsis when cut.
pub(crate) fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A short, human summary of a call's arguments (never the whole graph).
pub fn summarize_arguments(name: &str, arguments: &str) -> String {
    let args: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
    let blocks = args
        .pointer("/graph/blocks")
        .and_then(Value::as_array)
        .map(Vec::len);
    let graph = blocks.map_or(String::new(), |n| format!("draft with {n} blocks"));
    match name {
        REPLAY_SCENARIO => {
            let scenario = args.get("scenario").and_then(Value::as_str).unwrap_or("?");
            clip(&format!("{scenario}, {graph}"), 120)
        }
        DESCRIBE_BLOCK => clip(args.get("class").and_then(Value::as_str).unwrap_or("?"), 80),
        _ => graph,
    }
}
