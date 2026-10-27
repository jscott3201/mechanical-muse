//! Graph tools: validate, emit, verify. Each is a thin wrapper over the checks our authoring
//! loop runs (`mm_agent::check_graph`, `mm_graph::validate`, `mm_graph::emit_cxf`,
//! `mm_bridge::load_cxf`), so a coding agent and Muse Spark get the same verdicts.

use mm_agent::{AttemptOutcome, Feedback, check_graph, interface_problems, repair_message};
use mm_graph::{BlockGraph, EmitOptions, emit_cxf, validate};
use serde_json::{Value, json};

use crate::tools::{ToolError, ToolResult, Tools, string_arg};

/// The `graph` argument as an object, or as a string holding one.
fn graph_arg(args: &Value) -> Result<BlockGraph, ToolError> {
    let parsed = match args.get("graph") {
        None | Some(Value::Null) => {
            return Err(ToolError::new(
                "invalid-arguments",
                "missing argument: graph",
            ));
        }
        Some(Value::String(text)) => serde_json::from_str(text),
        Some(value) => serde_json::from_value(value.clone()),
    };
    parsed.map_err(|e| {
        ToolError::new(
            "not-a-graph",
            format!("graph does not match the block-graph format: {e}"),
        )
    })
}

pub(crate) fn validate_graph(tools: &Tools, args: &Value) -> ToolResult {
    let graph = graph_arg(args)?;
    let problems = match string_arg(args, "spec")? {
        Some(name) => interface_problems(&graph, &tools.spec_named(name)?.task),
        None => Vec::new(),
    };
    let findings = validate(&graph, &tools.palette);
    let code = findings
        .first()
        .map(|f| f.code.as_str())
        .or((!problems.is_empty()).then_some("interface-mismatch"));
    Ok(json!({
        "ok": code.is_none(),
        "code": code,
        "findings": findings,
        "interface_problems": problems,
    }))
}

pub(crate) fn emit(tools: &Tools, args: &Value) -> ToolResult {
    let mut graph = graph_arg(args)?;
    if let Some(name) = string_arg(args, "spec")? {
        graph.name.clone_from(&tools.spec_named(name)?.task.name);
    }
    let findings = validate(&graph, &tools.palette);
    if let Some(first) = findings.first() {
        return Err(ToolError::new(
            first.code.as_str(),
            format!("the graph has {} validator finding(s)", findings.len()),
        )
        .with_details(json!({"findings": findings})));
    }
    let cxf = emit_cxf(&graph, &tools.palette, &EmitOptions::default())
        .map_err(|e| ToolError::new("emit-failed", e.to_string()))?;
    let loaded = mm_bridge::load_cxf(&cxf).map_err(|failure| {
        ToolError::new(failure.code.clone(), failure.to_string())
            .with_details(json!({"load_failure": failure}))
    })?;
    Ok(json!({
        "name": graph.name,
        "content_id": loaded.content_id,
        "bytes": cxf.len(),
        "cxf": String::from_utf8_lossy(&cxf),
    }))
}

pub(crate) fn verify(tools: &Tools, args: &Value) -> ToolResult {
    let spec = tools.spec(args)?;
    let graph = graph_arg(args)?;
    let check = check_graph(graph, &spec.task, &tools.palette, &spec.vectors);
    let mut out = json!({"spec": spec.detail.summary.name});
    let (stage, code) = match &check.outcome {
        AttemptOutcome::Passed { .. } => ("replay", None),
        AttemptOutcome::ReplayFailed { .. } => ("replay", Some("replay-failed".to_owned())),
        AttemptOutcome::InterfaceMismatch { problems } => {
            set(&mut out, "interface_problems", json!(problems));
            ("interface", Some("interface-mismatch".to_owned()))
        }
        AttemptOutcome::Invalid { findings } => {
            set(&mut out, "findings", json!(findings));
            let code = findings.first().map(|f| f.code.as_str().to_owned());
            ("validate", code)
        }
        AttemptOutcome::NotAGraph { detail } => {
            set(&mut out, "detail", json!(detail));
            ("emit", Some("emit-failed".to_owned()))
        }
        AttemptOutcome::LoadFailed { failure } => {
            set(&mut out, "load_failure", json!(failure));
            ("load", Some(failure.code.clone()))
        }
        // Our side failed (the spec's vector file); the graph was never judged.
        AttemptOutcome::HarnessError { detail } => {
            return Err(ToolError::new("harness-error", detail.clone()));
        }
        AttemptOutcome::ModelError { .. } => {
            return Err(ToolError::new("harness-error", "unexpected model outcome"));
        }
    };
    let passed = matches!(check.outcome, AttemptOutcome::Passed { .. });
    let summary = match &check.report {
        Some(report) => {
            set(&mut out, "content_id", json!(report.content_id));
            set(&mut out, "passed_scenarios", json!(report.pass_count()));
            set(&mut out, "total_scenarios", json!(report.scenarios.len()));
            set(&mut out, "scenarios", json!(report.scenarios));
            format!(
                "passes {} of {} scenarios",
                report.pass_count(),
                report.scenarios.len()
            )
        }
        None => format!(
            "stopped at {stage}: {}",
            code.as_deref().unwrap_or("unknown")
        ),
    };
    set(&mut out, "passed", json!(passed));
    set(&mut out, "summary", json!(summary));
    set(&mut out, "stage", json!(stage));
    set(&mut out, "code", json!(code));
    let feedback = repair_message(&check.outcome, Feedback::WithVectors, &spec.vectors);
    set(
        &mut out,
        "feedback",
        if feedback.is_empty() {
            Value::Null
        } else {
            json!(feedback)
        },
    );
    Ok(out)
}

fn set(out: &mut Value, key: &str, value: Value) {
    if let Some(object) = out.as_object_mut() {
        object.insert(key.to_owned(), value);
    }
}
