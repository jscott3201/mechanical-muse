//! The tool list and the read-only tools (specs and palette). Graph checks are in `checks.rs`.
//! Every tool reuses the code our own authoring agent and HTTP API run; nothing here decides
//! whether a graph is right.

use mm_graph::{BLOCK_GRAPH_SCHEMA, Palette};
use mm_server::SpecStore;
use mm_server::specs::Spec;
use serde_json::{Value, json};

use crate::checks;

/// A tool failure the client's model can act on. `code` is one of our stable diagnostic codes
/// (validator finding, engine diagnostic, or `unknown-spec`, `invalid-arguments`, ...).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct ToolError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

impl ToolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn to_json(&self) -> Value {
        let mut body = json!({"code": self.code, "message": self.message});
        if let (Some(details), Some(object)) = (&self.details, body.as_object_mut()) {
            object.insert("details".into(), details.clone());
        }
        body
    }
}

pub type ToolResult = Result<Value, ToolError>;

/// The palette and specs every tool works against, loaded once at start-up.
pub struct Tools {
    pub(crate) palette: Palette,
    pub(crate) specs: SpecStore,
}

const GRAPH_ARG: &str = "A block graph: {name, inputs, outputs, blocks, connections}, as \
described by get_spec's graph_schema. A JSON string holding the graph is accepted too.";

/// Tool definitions, in a fixed order.
pub fn definitions() -> Value {
    let spec_arg =
        json!({"type": "string", "description": "Spec name from list_specs, e.g. chw_plant_reset"});
    let graph_arg = json!({"type": "object", "description": GRAPH_ARG});
    let read_only = json!({"readOnlyHint": true, "idempotentHint": true, "openWorldHint": false});
    json!([
        {
            "name": "list_specs",
            "title": "List control sequence specs",
            "description": "Lists the control sequence specs in this repository: name, title, \
                one-paragraph summary, typed inputs and outputs, and how many test scenarios \
                verify_graph replays.",
            "inputSchema": {"type": "object", "additionalProperties": false},
            "annotations": read_only,
        },
        {
            "name": "get_spec",
            "title": "Get one spec",
            "description": "One spec as a task: the task text our own agent receives, inputs, \
                outputs, parameters, scenario names and descriptions (expected values are not \
                shown), the authoring rules and the block-graph JSON Schema.",
            "inputSchema": {
                "type": "object",
                "properties": {"spec": spec_arg},
                "required": ["spec"],
                "additionalProperties": false,
            },
            "annotations": read_only,
        },
        {
            "name": "palette",
            "title": "Block palette",
            "description": "Authorable CDL block classes from the pinned engine catalog, one \
                line each: input and output ports in declaration order and parameters (* = \
                required). The same listing our own agent sees. Optional case-insensitive \
                filter on the class name, e.g. 'delay' or 'Reals.'.",
            "inputSchema": {
                "type": "object",
                "properties": {"query": {"type": "string", "description": "Substring of the class name"}},
                "additionalProperties": false,
            },
            "annotations": read_only,
        },
        {
            "name": "validate_graph",
            "title": "Validate a block graph",
            "description": "Structural checks before the engine: classes, parameters, \
                connections, drivers, names. Returns typed findings (code, subject, detail) and, \
                when a spec is given, interface problems against that spec. Fast; does not \
                run the engine.",
            "inputSchema": {
                "type": "object",
                "properties": {"graph": graph_arg, "spec": spec_arg},
                "required": ["graph"],
                "additionalProperties": false,
            },
            "annotations": read_only,
        },
        {
            "name": "emit_cxf",
            "title": "Emit CXF",
            "description": "Expands a valid block graph to CXF (CDL as JSON-LD) with our \
                deterministic emitter, loads it into the pinned engine and returns the CXF \
                text and the engine's content id. With a spec, the graph takes the spec's \
                name, as verify_graph does.",
            "inputSchema": {
                "type": "object",
                "properties": {"graph": graph_arg, "spec": spec_arg},
                "required": ["graph"],
                "additionalProperties": false,
            },
            "annotations": read_only,
        },
        {
            "name": "verify_graph",
            "title": "Verify a block graph against a spec",
            "description": "Runs the full check our authoring loop runs: interface, validator, \
                emitter, engine load (typed diagnostics), then replays every test scenario of \
                the spec on a fresh engine. Returns passes X of Y, each scenario's result \
                (first failing time, expected and actual), the content id, and `feedback`: \
                the repair text our own agent would receive.",
            "inputSchema": {
                "type": "object",
                "properties": {"graph": graph_arg, "spec": spec_arg},
                "required": ["graph", "spec"],
                "additionalProperties": false,
            },
            "annotations": read_only,
        },
    ])
}

impl Tools {
    pub fn new(palette: Palette, specs: SpecStore) -> Self {
        Self { palette, specs }
    }

    /// Runs tool `name`; `None` when there is no such tool.
    pub fn call(&self, name: &str, args: &Value) -> Option<ToolResult> {
        Some(match name {
            "list_specs" => Ok(self.list_specs()),
            "get_spec" => self.get_spec(args),
            "palette" => self.palette_tool(args),
            "validate_graph" => checks::validate_graph(self, args),
            "emit_cxf" => checks::emit(self, args),
            "verify_graph" => checks::verify(self, args),
            _ => return None,
        })
    }

    pub(crate) fn spec(&self, args: &Value) -> Result<&Spec, ToolError> {
        let name = string_arg(args, "spec")?
            .ok_or_else(|| ToolError::new("invalid-arguments", "missing argument: spec"))?;
        self.spec_named(name)
    }

    pub(crate) fn spec_named(&self, name: &str) -> Result<&Spec, ToolError> {
        self.specs.get(name).ok_or_else(|| {
            let known: Vec<&str> = self
                .specs
                .iter()
                .map(|s| s.detail.summary.name.as_str())
                .collect();
            ToolError::new("unknown-spec", format!("no spec named {name}"))
                .with_details(json!({"known": known}))
        })
    }

    fn list_specs(&self) -> Value {
        let specs: Vec<Value> = self
            .specs
            .iter()
            .map(|spec| {
                let s = &spec.detail.summary;
                json!({
                    "name": s.name,
                    "title": s.title,
                    "about": spec.detail.about,
                    "inputs": s.inputs,
                    "outputs": s.outputs,
                    "scenarios": s.scenarios,
                })
            })
            .collect();
        json!({"specs": specs})
    }

    fn get_spec(&self, args: &Value) -> ToolResult {
        let spec = self.spec(args)?;
        let d = &spec.detail;
        let schema: Value = serde_json::from_str(BLOCK_GRAPH_SCHEMA).unwrap_or(Value::Null);
        Ok(json!({
            "name": d.summary.name,
            "title": d.summary.title,
            "about": d.about,
            "task": mm_agent::task_message(&spec.task),
            "inputs": d.summary.inputs,
            "outputs": d.summary.outputs,
            "params": d.params,
            "scenarios": d.scenarios,
            "step_s": d.step_s,
            "rules": mm_agent::AUTHORING_RULES,
            "graph_schema": schema,
        }))
    }

    fn palette_tool(&self, args: &Value) -> ToolResult {
        let query = string_arg(args, "query")?.map(str::to_lowercase);
        let classes: Vec<String> = mm_agent::palette_listing(&self.palette)
            .lines()
            .filter(|line| {
                let class = line
                    .strip_prefix("- ")
                    .and_then(|l| l.split_once(':'))
                    .map_or("", |(class, _)| class);
                query
                    .as_deref()
                    .is_none_or(|q| class.to_lowercase().contains(q))
            })
            .map(str::to_owned)
            .collect();
        Ok(json!({"count": classes.len(), "classes": classes}))
    }
}

/// An optional string argument; present but not a string is an error.
pub(crate) fn string_arg<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(ToolError::new(
            "invalid-arguments",
            format!("argument {key} must be a string"),
        )),
    }
}
