//! "What changed between attempts": the exact difference between two consecutive drafts of an
//! authoring session (computed here, not by the model) and the earlier draft's failures, explained
//! by the live model. Keyed by a digest of that context, so a replayed recorded session reuses the
//! explanation (and a committed one serves without a key). Advisory only: the verification
//! receipt is the verdict, and nothing here changes a draft.

use std::collections::{BTreeMap, BTreeSet};

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::author_dto::{AttemptResult, AttemptView, AuthorEvent};
use crate::dto::{BlockView, ConnectionView, EndView, GraphView, ScenarioResult};
use crate::explain::Stored;
use crate::routes::{AppState, Failure};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FixChangeView {
    pub title: String,
    pub text: String,
}

/// A model-written account of how attempt `after` repaired attempt `before`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FixExplanationView {
    /// Digest of the results and difference explained (`fix:<hex>`).
    pub key: String,
    pub model: String,
    /// `live` or `recorded`.
    pub source: String,
    pub before: u32,
    pub after: u32,
    pub summary: String,
    pub changes: Vec<FixChangeView>,
}

impl Stored for FixExplanationView {
    fn key(&self) -> &str {
        &self.key
    }
    fn mark_recorded(&mut self) {
        self.source = "recorded".into();
    }
}

/// Failures quoted to the model: the first few, which are the ones a repair answers.
const MAX_FAILURES: usize = 6;

fn end(e: &EndView) -> String {
    match &e.port {
        Some(p) => format!("{}.{p}", e.node),
        None => e.node.clone(),
    }
}

fn wire(c: &ConnectionView) -> String {
    format!("{} -> {}", end(&c.from), end(&c.to))
}

fn params(b: &BlockView) -> BTreeMap<String, Value> {
    b.params
        .iter()
        .map(|p| (p.name.clone(), p.value.clone()))
        .collect()
}

/// The exact difference from graph `a` to graph `b`: blocks by id, parameters by name, wires as
/// `from -> to`. Sorted, so the same two graphs always give the same difference.
pub(crate) fn diff(a: &GraphView, b: &GraphView) -> Value {
    let by_id = |g: &GraphView| -> BTreeMap<String, BlockView> {
        g.blocks.iter().map(|x| (x.id.clone(), x.clone())).collect()
    };
    let (ba, bb) = (by_id(a), by_id(b));
    let added: Vec<Value> = bb
        .iter()
        .filter(|(id, _)| !ba.contains_key(*id))
        .map(|(id, x)| json!({"id": id, "class": x.class, "params": params(x)}))
        .collect();
    let removed: Vec<Value> = ba
        .iter()
        .filter(|(id, _)| !bb.contains_key(*id))
        .map(|(id, x)| json!({"id": id, "class": x.class, "params": params(x)}))
        .collect();
    let mut changed = Vec::new();
    for (id, xa) in &ba {
        let Some(xb) = bb.get(id) else { continue };
        let (pa, pb) = (params(xa), params(xb));
        let names: BTreeSet<&String> = pa.keys().chain(pb.keys()).collect();
        let param_changes: Vec<Value> = names
            .into_iter()
            .filter(|n| pa.get(*n) != pb.get(*n))
            .map(|n| json!({"param": n, "before": pa.get(n), "after": pb.get(n)}))
            .collect();
        if xa.class != xb.class || !param_changes.is_empty() {
            changed.push(json!({
                "id": id,
                "class_before": xa.class,
                "class_after": xb.class,
                "params": param_changes,
            }));
        }
    }
    let wires = |g: &GraphView| -> BTreeSet<String> { g.connections.iter().map(wire).collect() };
    let (wa, wb) = (wires(a), wires(b));
    json!({
        "blocks_added": added,
        "blocks_removed": removed,
        "blocks_changed": changed,
        "wires_added": wb.difference(&wa).collect::<Vec<_>>(),
        "wires_removed": wa.difference(&wb).collect::<Vec<_>>(),
    })
}

fn minutes(s: f64) -> f64 {
    (s / 60.0 * 10.0).round() / 10.0
}

/// An attempt's result as the model sees it: what refused it, or which scenarios failed where.
fn result(r: &AttemptResult) -> Value {
    match r {
        AttemptResult::Passed { scenarios, .. } => {
            json!({"verdict": "passed every scenario", "scenarios": scenarios})
        }
        AttemptResult::ReplayFailed {
            passed,
            total,
            failures,
        } => json!({
            "verdict": "failed scenarios",
            "passed": passed,
            "total": total,
            "failures": failures.iter().take(MAX_FAILURES).map(|f| match &f.result {
                ScenarioResult::Fail { t, output, expected, actual, from_s, to_s } => json!({
                    "scenario": f.name,
                    "output": output,
                    "at_s": t, "at_min": minutes(*t),
                    "expected": expected, "actual": actual,
                    "window_s": [from_s, to_s], "window_min": [minutes(*from_s), minutes(*to_s)],
                }),
                ScenarioResult::Error { code, detail, .. } => {
                    json!({"scenario": f.name, "error": code, "detail": detail})
                }
                ScenarioResult::Pass => json!({"scenario": f.name, "passed": true}),
            }).collect::<Vec<_>>(),
        }),
        AttemptResult::LoadFailed {
            stage,
            code,
            problems,
            ..
        } => json!({
            "verdict": "refused by the engine",
            "stage": stage,
            "code": code,
            "problems": problems.iter().map(|p| serde_json::to_value(p).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        }),
        AttemptResult::Invalid { problems } => json!({
            "verdict": "refused by the validator",
            "problems": problems.iter().map(|p| serde_json::to_value(p).unwrap_or(Value::Null)).collect::<Vec<_>>(),
        }),
        AttemptResult::InterfaceMismatch { problems } => {
            json!({"verdict": "did not match the task's interface", "problems": problems})
        }
        AttemptResult::NotAGraph { message } => {
            json!({"verdict": "not a graph", "detail": message})
        }
        AttemptResult::ModelError { message } | AttemptResult::HarnessError { message } => {
            json!({"verdict": "no draft", "detail": message})
        }
    }
}

/// The context for attempt `after` of `spec`, repairing `before`; both must carry graphs.
pub(crate) fn context(spec: &str, before: &AttemptView, after: &AttemptView) -> Option<Value> {
    let (a, b) = (before.graph.as_ref()?, after.graph.as_ref()?);
    Some(json!({
        "spec": spec,
        "earlier_attempt": {"index": before.index, "result": result(&before.result), "blocks": a.blocks.len()},
        "later_attempt": {"index": after.index, "result": result(&after.result), "blocks": b.blocks.len()},
        "difference": diff(a, b),
    }))
}

/// FNV-1a 64, hex: a stable key for a context.
fn digest(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("fix:{hash:016x}")
}

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/authoring/{id}/attempts/{after}/explain",
        post(explain),
    )
}

async fn explain(
    State(state): State<AppState>,
    UrlPath((id, after)): UrlPath<(String, u32)>,
) -> Result<Json<FixExplanationView>, Failure> {
    let session = state.authoring().get(&id).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "unknown-session",
            "no authoring session with that id",
        )
    })?;
    let attempts: BTreeMap<u32, AttemptView> = session
        .journal()
        .since(None)
        .into_iter()
        .filter_map(|e| match e.event {
            AuthorEvent::Checked { attempt } => Some((attempt.index, attempt)),
            _ => None,
        })
        .collect();
    let not_two = || {
        Failure::new(
            StatusCode::CONFLICT,
            "no-repair",
            "the attempt and the one before it must both have a drafted graph",
        )
    };
    let later = attempts.get(&after).ok_or_else(not_two)?;
    let earlier = after
        .checked_sub(1)
        .and_then(|i| attempts.get(&i))
        .ok_or_else(not_two)?;
    let context = context(&session.config.spec, earlier, later).ok_or_else(not_two)?;
    let key = digest(&context.to_string());
    let ex = state.explanations();
    if let Some(found) = ex.known(&ex.fixes, &key) {
        return Ok(Json(found));
    }
    let client = state.authoring().config.spark.clone().ok_or_else(|| {
        Failure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "no-model",
            "no model key on this server and no recorded explanation of this repair",
        )
    })?;
    let (e, _usage) = mm_agent::explain_fix(client.as_ref(), crate::authoring::MODEL, &context)
        .await
        .map_err(|e| Failure::new(StatusCode::BAD_GATEWAY, "model-error", e.to_string()))?;
    let view = FixExplanationView {
        key,
        model: crate::authoring::MODEL.to_owned(),
        source: "live".into(),
        before: earlier.index,
        after: later.index,
        summary: e.summary,
        changes: e
            .changes
            .into_iter()
            .map(|c| FixChangeView {
                title: c.title,
                text: c.text,
            })
            .collect(),
    };
    ex.remember(&ex.fixes, &view);
    Ok(Json(view))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::dto::{ParamView, Signal};

    fn block(id: &str, class: &str, params: &[(&str, Value)]) -> BlockView {
        BlockView {
            id: id.into(),
            class: class.into(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: params
                .iter()
                .map(|(n, v)| ParamView {
                    name: (*n).into(),
                    value: v.clone(),
                })
                .collect(),
        }
    }

    fn wire_of(from: &str, to: &str) -> ConnectionView {
        let e = |s: &str| {
            let (node, port) = s.split_once('.').map_or((s, None), |(n, p)| (n, Some(p)));
            EndView {
                node: node.into(),
                port: port.map(str::to_owned),
            }
        };
        ConnectionView {
            from: e(from),
            to: e(to),
            signal: Signal::Real,
        }
    }

    fn graph(blocks: Vec<BlockView>, connections: Vec<ConnectionView>) -> GraphView {
        GraphView {
            name: "g".into(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            blocks,
            connections,
        }
    }

    #[test]
    fn the_difference_names_blocks_params_and_wires() {
        let a = graph(
            vec![
                block("hold", "Logical.TrueDelay", &[("delayTime", json!(300.0))]),
                block("old", "Reals.Max", &[]),
            ],
            vec![wire_of("u1", "hold.u"), wire_of("hold.y", "out")],
        );
        let b = graph(
            vec![
                block("hold", "Logical.TrueDelay", &[("delayTime", json!(600.0))]),
                block("new", "Reals.Min", &[]),
            ],
            vec![wire_of("u1", "hold.u"), wire_of("new.y", "out")],
        );
        let d = diff(&a, &b);
        assert_eq!(d["blocks_added"][0]["id"], "new");
        assert_eq!(d["blocks_removed"][0]["id"], "old");
        let changed = &d["blocks_changed"][0];
        assert_eq!(changed["id"], "hold");
        assert_eq!(changed["params"][0]["before"], 300.0);
        assert_eq!(changed["params"][0]["after"], 600.0);
        assert_eq!(d["wires_added"], json!(["new.y -> out"]));
        assert_eq!(d["wires_removed"], json!(["hold.y -> out"]));
        // Same graphs, empty difference; and deterministic.
        let same = diff(&a, &a);
        assert_eq!(same["blocks_changed"], json!([]));
        assert_eq!(diff(&a, &b), d);
    }

    #[test]
    fn a_context_needs_both_graphs_and_quotes_failures_in_s_and_min() {
        let view = |index: u32, result: AttemptResult, g: Option<GraphView>| AttemptView {
            index,
            result,
            usage: crate::author_dto::TokenUsage::default(),
            calls: 1,
            tool_calls: Vec::new(),
            graph: g,
        };
        let failing = AttemptResult::ReplayFailed {
            passed: 4,
            total: 6,
            failures: vec![crate::dto::ScenarioReport {
                name: "start_up_hold".into(),
                ticks: 10,
                result: ScenarioResult::Fail {
                    t: 300.0,
                    output: "TSupSet".into(),
                    expected: json!(291.48),
                    actual: json!(285.93),
                    from_s: 0.0,
                    to_s: 540.0,
                },
            }],
        };
        let g = graph(vec![block("x", "Reals.Max", &[])], Vec::new());
        let before = view(1, failing.clone(), Some(g.clone()));
        let after = view(
            2,
            AttemptResult::Passed {
                content_id: "cxf:x".into(),
                scenarios: 6,
            },
            Some(g.clone()),
        );
        let c = context("rev_b", &before, &after).unwrap();
        let f = &c["earlier_attempt"]["result"]["failures"][0];
        assert_eq!(f["at_s"], 300.0);
        assert_eq!(f["at_min"], 5.0);
        assert_eq!(f["window_min"], json!([0.0, 9.0]));
        assert_eq!(c["later_attempt"]["result"]["scenarios"], 6);
        assert!(context("rev_b", &view(1, failing, None), &after).is_none());
    }
}
