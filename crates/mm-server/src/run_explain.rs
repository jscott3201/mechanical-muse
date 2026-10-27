//! What happened in a run, in an operator's words: the run's settings and the runtime's own
//! events (faults, sequences that stopped writing, setpoints handed back), grouped into spans
//! and explained by the live model. Keyed by a digest of that context, so a replay of the same
//! recording reuses the explanation (and a committed one serves without a key). Never used for
//! control.

use std::collections::BTreeMap;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::explain::Stored;
use crate::routes::{AppState, Failure};
use crate::run_dto::{Envelope, RunEvent, SupervisorEvent};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunExplainedItemView {
    pub when: String,
    pub title: String,
    pub text: String,
}

/// A model-written account of one run's events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunExplanationView {
    /// Digest of the settings and events explained (`run:<hex>`).
    pub key: String,
    pub model: String,
    /// `live` or `recorded`.
    pub source: String,
    pub summary: String,
    pub items: Vec<RunExplainedItemView>,
    /// How many event spans the model was given.
    pub spans: u32,
}

impl Stored for RunExplanationView {
    fn key(&self) -> &str {
        &self.key
    }
    fn mark_recorded(&mut self) {
        self.source = "recorded".into();
    }
}

/// At most this many event spans go to the model; a longer run keeps its first ones.
const MAX_SPANS: usize = 40;

/// BOPTEST time as "day D HH:MM" (the web app's format).
fn sim_time(s: f64) -> String {
    let total = (s / 60.0).round();
    let day = (total / 1440.0).floor();
    let minute = total - day * 1440.0;
    format!(
        "day {day} {:02.0}:{:02.0}",
        (minute / 60.0).floor(),
        minute % 60.0
    )
}

/// What makes two events the same: kind, sequence, point and detail.
type EventKey = (String, Option<String>, Option<String>, Option<String>);

#[derive(Debug, Clone, PartialEq)]
struct Span {
    key: EventKey,
    from: f64,
    to: f64,
    steps: u32,
}

/// Repeated events become one span per contiguous stretch of steps.
fn spans(events: &[(f64, &SupervisorEvent)], step_s: f64) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut open: BTreeMap<EventKey, usize> = BTreeMap::new();
    for (t, e) in events {
        let key = (
            e.kind.clone(),
            e.sequence.clone(),
            e.point.clone(),
            e.reason.clone(),
        );
        match open.get(&key).and_then(|&i| out.get_mut(i)) {
            Some(span) if *t - span.to <= step_s * 1.5 => {
                span.to = *t;
                span.steps += 1;
            }
            _ => {
                open.insert(key.clone(), out.len());
                out.push(Span {
                    key,
                    from: *t,
                    to: *t,
                    steps: 1,
                });
            }
        }
    }
    out.sort_by(|a, b| a.from.total_cmp(&b.from));
    out.truncate(MAX_SPANS);
    out
}

/// The run's settings and grouped events, as the model sees them.
pub(crate) fn context(events: &[Envelope]) -> Option<Value> {
    let mut config = None;
    let mut sequences = Vec::new();
    let mut logged: Vec<(f64, &SupervisorEvent)> = Vec::new();
    let mut ended = None;
    for env in events {
        match &env.event {
            RunEvent::Started {
                config: c,
                sequences: s,
            } => {
                config = Some(c);
                sequences = s.iter().collect();
            }
            RunEvent::Tick { events, .. } => {
                if let Some(t) = env.sim_time_s {
                    logged.extend(events.iter().map(|e| (t, e)));
                }
            }
            RunEvent::Ended { state, message } => ended = Some((state, message)),
            _ => {}
        }
    }
    let config = config?;
    let mut by_spec: BTreeMap<&str, u32> = BTreeMap::new();
    for s in &sequences {
        *by_spec.entry(s.spec.as_str()).or_default() += 1;
    }
    let grouped = spans(&logged, config.step_s);
    Some(json!({
        "run": {
            "baseline_only": config.baseline,
            "air_handler_requests": config.requests,
            "static_pressure_reset": config.pressure,
            "from": sim_time(config.start_s),
            "steps": config.steps,
            "step_s": config.step_s,
            "replay_of_recorded_run": config.replay.is_some(),
            "test_fault": config.fault.as_ref().map(|f| json!({
                "point": f.point,
                "from": sim_time(config.start_s + f64::from(f.from_step) * config.step_s),
                "steps": f.steps,
            })),
        },
        "sequences": by_spec.iter().map(|(spec, n)| json!({"spec": spec, "instances": n})).collect::<Vec<_>>(),
        "events": grouped.iter().map(|s| json!({
            "when": if s.steps > 1 { format!("{} to {}", sim_time(s.from), sim_time(s.to)) } else { sim_time(s.from) },
            "steps": s.steps,
            "kind": s.key.0,
            "sequence": s.key.1,
            "point": s.key.2,
            "detail": s.key.3,
        })).collect::<Vec<_>>(),
        "ended": ended.map(|(state, message)| json!({"state": state, "message": message})),
    }))
}

/// FNV-1a 64, hex: a stable key for a context.
fn digest(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("run:{hash:016x}")
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/runs/{id}/explain", post(explain))
}

async fn explain(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<String>,
) -> Result<Json<RunExplanationView>, Failure> {
    let handle = state
        .runs()
        .get(&id)
        .ok_or_else(|| Failure::new(StatusCode::NOT_FOUND, "unknown-run", "no run with that id"))?;
    let events = handle.journal().since(None);
    let context = context(&events).ok_or_else(|| {
        Failure::new(
            StatusCode::CONFLICT,
            "not-started",
            "the run has not started yet",
        )
    })?;
    let key = digest(&context.to_string());
    let ex = state.explanations();
    if let Some(found) = ex.known(&ex.runs, &key) {
        return Ok(Json(found));
    }
    let client = state.authoring().config.spark.clone().ok_or_else(|| {
        Failure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "no-model",
            "no model key on this server and no recorded explanation of these events",
        )
    })?;
    let (explanation, _usage) =
        mm_agent::explain_run(client.as_ref(), crate::authoring::MODEL, &context)
            .await
            .map_err(|e| Failure::new(StatusCode::BAD_GATEWAY, "model-error", e.to_string()))?;
    let view = RunExplanationView {
        key,
        model: crate::authoring::MODEL.to_owned(),
        source: "live".into(),
        summary: explanation.summary,
        items: explanation
            .items
            .into_iter()
            .map(|i| RunExplainedItemView {
                when: i.when,
                title: i.title,
                text: i.text,
            })
            .collect(),
        spans: u32::try_from(
            context
                .pointer("/events")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        )
        .unwrap_or(u32::MAX),
    };
    ex.remember(&ex.runs, &view);
    Ok(Json(view))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn ev(kind: &str, sequence: &str) -> SupervisorEvent {
        SupervisorEvent {
            kind: kind.into(),
            sequence: Some(sequence.into()),
            point: None,
            value: None,
            sent: None,
            reason: Some("reading is not finite".into()),
        }
    }

    #[test]
    fn repeated_events_become_spans() {
        let a = ev("degraded", "ahu1");
        let b = ev("released", "chw");
        let events = vec![
            (2400.0, &a),
            (2520.0, &a),
            (2640.0, &a),
            (2640.0, &b),
            (6000.0, &a),
        ];
        let s = spans(&events, 120.0);
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].from, s[0].to, s[0].steps), (2400.0, 2640.0, 3));
        assert_eq!(s[1].key.1.as_deref(), Some("chw"));
        assert_eq!(
            (s[2].from, s[2].steps),
            (6000.0, 1),
            "a later recurrence is a new span"
        );
    }

    #[test]
    fn times_read_like_the_web_app() {
        assert_eq!(sim_time(16070400.0 + 40.0 * 60.0), "day 186 00:40");
        assert_eq!(digest("x"), digest("x"));
        assert!(digest("x").starts_with("run:"));
    }
}
