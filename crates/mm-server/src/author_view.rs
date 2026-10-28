//! How an attempt and a tool call look to the web app: `mm_agent` records turned into DTOs.

use mm_agent::{Attempt, AttemptOutcome, ToolCallRecord, Usage};
use mm_graph::{BlockGraph, Palette};

use crate::author_dto::{AttemptResult, AttemptView, Problem, TokenUsage, ToolCallView};
use crate::specs::graph_view;

/// One of Spark's own checks, without its arguments (the draft can be large).
pub(crate) fn tool_call_view(call: &ToolCallRecord) -> ToolCallView {
    ToolCallView {
        name: call.name.clone(),
        args: call.args_summary.clone(),
        ok: call.ok,
        summary: call.summary.clone(),
    }
}

pub(crate) fn tokens(u: Usage) -> TokenUsage {
    TokenUsage {
        input: u.input_tokens,
        output: u.output_tokens,
        reasoning: u.reasoning_tokens,
    }
}

/// The block or boundary point a problem's subject names, if it names one of `graph`'s. Engine
/// subjects look like `urn:...#<sequence>.<block>.<port>`; validator subjects like `<block>`,
/// `<block>.<port>` or a boundary name.
pub(crate) fn node_of(subject: &str, graph: Option<&BlockGraph>) -> Option<String> {
    let graph = graph?;
    let local = subject.rsplit_once('#').map_or(subject, |(_, l)| l);
    let local = local
        .strip_prefix(graph.name.as_str())
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or(local);
    let head = local.split('.').next().unwrap_or(local);
    let known = graph.blocks.iter().any(|b| b.id == head)
        || graph.inputs.iter().any(|p| p.name == head)
        || graph.outputs.iter().any(|p| p.name == head);
    known.then(|| head.to_owned())
}

pub(crate) fn attempt_view(attempt: &Attempt, task: &str, palette: &Palette) -> AttemptView {
    let graph: Option<BlockGraph> = attempt
        .reply
        .as_deref()
        .and_then(|text| serde_json::from_str::<BlockGraph>(text).ok())
        .map(|mut g| {
            // The loop names the sequence after the task; engine subjects use that name.
            g.name = task.to_owned();
            g
        });
    let problem = |code: String, subject: String, message: String| Problem {
        node: node_of(&subject, graph.as_ref()),
        code,
        subject,
        message,
    };
    let result = match &attempt.outcome {
        AttemptOutcome::ModelError { error } => AttemptResult::ModelError {
            message: error.to_string(),
        },
        AttemptOutcome::HarnessError { detail } => AttemptResult::HarnessError {
            message: detail.clone(),
        },
        AttemptOutcome::NotAGraph { detail } => AttemptResult::NotAGraph {
            message: detail.clone(),
        },
        AttemptOutcome::InterfaceMismatch { problems } => AttemptResult::InterfaceMismatch {
            problems: problems.clone(),
        },
        AttemptOutcome::Invalid { findings } => AttemptResult::Invalid {
            problems: findings
                .iter()
                .map(|f| {
                    problem(
                        f.code.as_str().to_owned(),
                        f.subject.clone(),
                        f.detail.clone(),
                    )
                })
                .collect(),
        },
        AttemptOutcome::LoadFailed { failure } => AttemptResult::LoadFailed {
            stage: failure.stage.to_owned(),
            code: failure.code.clone(),
            problems: failure
                .diagnostics
                .iter()
                .map(|d| {
                    problem(
                        d.code.clone(),
                        d.subject.clone().unwrap_or_default(),
                        d.message.clone(),
                    )
                })
                .collect(),
            detail: failure.error.as_ref().map(|e| e.detail.clone()),
        },
        AttemptOutcome::ReplayFailed {
            passed,
            total,
            failures,
        } => AttemptResult::ReplayFailed {
            passed: u32::try_from(*passed).unwrap_or(u32::MAX),
            total: u32::try_from(*total).unwrap_or(u32::MAX),
            failures: failures
                .iter()
                .cloned()
                .map(crate::dto::scenario_report)
                .collect(),
        },
        AttemptOutcome::Passed {
            content_id,
            scenarios,
        } => AttemptResult::Passed {
            content_id: content_id.clone(),
            scenarios: u32::try_from(*scenarios).unwrap_or(u32::MAX),
        },
    };
    AttemptView {
        index: attempt.index,
        result,
        usage: tokens(attempt.usage),
        calls: attempt.calls,
        tool_calls: attempt
            .tool_turns
            .iter()
            .flat_map(|t| &t.calls)
            .map(tool_call_view)
            .collect(),
        // A graph with an unknown class has no typed view; the problems list still names it.
        graph: graph.and_then(|g| graph_view(&g, palette).ok()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn graph() -> BlockGraph {
        serde_json::from_value(serde_json::json!({
            "name": "seq",
            "inputs": [{"name": "xin", "type": "real", "unit": "1", "description": ""}],
            "outputs": [{"name": "xout", "type": "real", "description": ""}],
            "blocks": [{"id": "add", "class": "Reals.Add", "params": []}],
            "connections": ["xin -> add.u1", "add.y -> add.u2", "add.y -> xout"]
        }))
        .unwrap()
    }

    #[test]
    fn subjects_map_to_the_block_or_point_they_name() {
        let g = graph();
        let node = |s: &str| node_of(s, Some(&g));
        assert_eq!(
            node("urn:mechanical-muse:seq#seq.add.u2").as_deref(),
            Some("add")
        );
        assert_eq!(node("add.y").as_deref(), Some("add"));
        assert_eq!(node("add").as_deref(), Some("add"));
        assert_eq!(node("xin").as_deref(), Some("xin"));
        assert_eq!(
            node("urn:mechanical-muse:seq#seq.xout").as_deref(),
            Some("xout")
        );
        // Names that are not in the graph map to nothing rather than to a guess.
        assert_eq!(node("gain.y"), None);
        assert_eq!(node(""), None);
        assert_eq!(node_of("add.y", None), None);
    }
}
