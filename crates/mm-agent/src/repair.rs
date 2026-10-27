//! The authoring loop: draft, check, repair, until the vectors pass or the budget runs out.
//! Each attempt is recorded, so a run can be shown step by step and scored honestly.

use mm_bridge::{
    LoadFailure, ReplayLimits, ScenarioOutcome, ScenarioReport, VectorFile, check_vectors, replay,
};

use crate::feedback::feedback_message;
use mm_graph::{BLOCK_GRAPH_SCHEMA, BlockGraph, EmitOptions, Finding, Palette, emit_cxf, validate};
use serde::Serialize;
use serde_json::{Value, json};

use crate::client::{Effort, ModelClient, ModelError, ModelRequest, OutputSchema, Usage};
use crate::prompt::{Exemplar, TaskSpec, system_instructions, task_message};

/// What the model hears after a failed replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Feedback {
    /// Validator and load problems in full; a replay failure only as "behavior does not match"
    /// (one bit, no names, counts or values). Use this when the same vectors score the result.
    ValidatorOnly,
    /// Also each failing scenario's name, description, time, expected and actual values. A score
    /// after this kind of repair must be labelled "repaired against the scoring vectors".
    WithVectors,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuthorConfig {
    pub model: String,
    pub effort: Option<Effort>,
    pub max_output_tokens: u32,
    /// Total drafts allowed, including the first.
    pub max_attempts: u32,
    pub feedback: Feedback,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum AttemptOutcome {
    /// The model call failed. A truncated reply is retried as a new attempt; anything else
    /// stops the loop.
    ModelError { error: ModelError },
    /// Our side failed (e.g. the vector file was rejected). Never shown to the model; the loop
    /// stops and the run must not count as the model's failure.
    HarnessError { detail: String },
    /// The reply was not a block graph.
    NotAGraph { detail: String },
    /// Input or output names or types differ from the task.
    InterfaceMismatch { problems: Vec<String> },
    /// `mm-graph` findings.
    Invalid { findings: Vec<Finding> },
    /// The engine refused the CXF.
    LoadFailed { failure: LoadFailure },
    /// Some scenarios did not pass.
    ReplayFailed {
        passed: usize,
        total: usize,
        failures: Vec<ScenarioReport>,
    },
    Passed {
        content_id: String,
        scenarios: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Attempt {
    pub index: u32,
    /// The reply text as received, when there was one.
    pub reply: Option<String>,
    pub outcome: AttemptOutcome,
    pub usage: Usage,
}

/// A draft that passed every scenario: the receipt is the content id plus the replay result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VerifiedDraft {
    pub graph: BlockGraph,
    #[serde(skip)]
    pub cxf: Vec<u8>,
    pub content_id: String,
    pub scenarios: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuthoringTrace {
    pub task: String,
    pub config: AuthorConfig,
    pub attempts: Vec<Attempt>,
    pub verified: Option<VerifiedDraft>,
    /// True if any repair message showed scenario details from the scoring vectors.
    pub repair_saw_vectors: bool,
    /// Set when the run could not start (bad vector file, bad configuration); no model call
    /// was made.
    pub setup_error: Option<String>,
    pub usage: Usage,
}

/// What the loop reports while it works, for live views.
#[derive(Debug, Clone, Copy)]
pub enum Progress<'a> {
    /// A model call for draft `attempt` (of at most `of`) is about to start.
    Drafting { attempt: u32, of: u32 },
    /// A draft was checked (or its model call failed).
    Checked(&'a Attempt),
}

/// Drafts `task`, checks each draft with `mm-graph` and `mm-bridge` against `vectors`, and
/// feeds problems back until a draft passes or `config.max_attempts` is reached.
pub async fn author<C: ModelClient>(
    client: &C,
    palette: &Palette,
    task: &TaskSpec,
    vectors: &VectorFile,
    exemplars: &[Exemplar],
    config: &AuthorConfig,
) -> AuthoringTrace {
    author_observed(client, palette, task, vectors, exemplars, config, |_| {}).await
}

/// [`author`], calling `observe` before each model call and after each checked attempt.
pub async fn author_observed<C: ModelClient, F: FnMut(Progress<'_>) + Send>(
    client: &C,
    palette: &Palette,
    task: &TaskSpec,
    vectors: &VectorFile,
    exemplars: &[Exemplar],
    config: &AuthorConfig,
    mut observe: F,
) -> AuthoringTrace {
    let mut trace = AuthoringTrace {
        task: task.name.clone(),
        config: config.clone(),
        attempts: Vec::new(),
        verified: None,
        repair_saw_vectors: false,
        setup_error: None,
        usage: Usage::default(),
    };
    if let Err(e) = check_vectors(vectors, &ReplayLimits::default()) {
        trace.setup_error = Some(format!("vector file rejected: {e}"));
        return trace;
    }
    if config.effort == Some(Effort::Max) && config.model.ends_with("-contributor") {
        trace.setup_error = Some("the contributor models do not support effort max".into());
        return trace;
    }
    let schema = serde_json::from_str(BLOCK_GRAPH_SCHEMA).unwrap_or(Value::Null);
    let mut request = ModelRequest {
        model: config.model.clone(),
        instructions: system_instructions(palette, exemplars),
        input: vec![user_message(&task_message(task))],
        schema: Some(OutputSchema {
            name: "block_graph".into(),
            schema,
        }),
        effort: config.effort,
        max_output_tokens: config.max_output_tokens,
    };

    for index in 1..=config.max_attempts {
        observe(Progress::Drafting {
            attempt: index,
            of: config.max_attempts,
        });
        let reply = match client.complete(&request).await {
            Ok(reply) => reply,
            Err(error) => {
                // A truncated reply costs an attempt and is re-asked with the same input; its
                // partial items are not replayed (reasoning with no following message is a 400).
                let truncated = matches!(error, ModelError::Incomplete { .. });
                trace.attempts.push(Attempt {
                    index,
                    reply: None,
                    outcome: AttemptOutcome::ModelError { error },
                    usage: Usage::default(),
                });
                if let Some(last) = trace.attempts.last() {
                    observe(Progress::Checked(last));
                }
                if truncated {
                    continue;
                }
                break;
            }
        };
        trace.usage.add(reply.usage);
        let (outcome, verified) = check_draft(&reply.text, task, palette, vectors);
        let feedback = feedback_message(&outcome, config.feedback, vectors);
        let harness_failed = matches!(outcome, AttemptOutcome::HarnessError { .. });
        if config.feedback == Feedback::WithVectors
            && matches!(outcome, AttemptOutcome::ReplayFailed { .. })
        {
            trace.repair_saw_vectors |= index < config.max_attempts;
        }
        trace.attempts.push(Attempt {
            index,
            reply: Some(reply.text),
            outcome,
            usage: reply.usage,
        });
        if let Some(last) = trace.attempts.last() {
            observe(Progress::Checked(last));
        }
        if verified.is_some() {
            trace.verified = verified;
            break;
        }
        if harness_failed {
            break;
        }
        // Replay the model's own output items (with encrypted reasoning), then the feedback.
        request.input.extend(reply.output_items);
        request.input.push(user_message(&feedback));
    }
    trace
}

fn user_message(text: &str) -> Value {
    json!({"role": "user", "content": [{"type": "input_text", "text": text}]})
}

/// Input and output names and types must match the task exactly.
pub fn interface_problems(graph: &BlockGraph, task: &TaskSpec) -> Vec<String> {
    let mut problems = Vec::new();
    let mut compare = |kind: &str,
                       want: Vec<(&str, mm_graph::SignalType)>,
                       got: Vec<(&str, mm_graph::SignalType)>| {
        for (name, ty) in &want {
            match got.iter().find(|(n, _)| n == name) {
                None => problems.push(format!("missing {kind} {name}")),
                Some((_, t)) if t != ty => {
                    problems.push(format!("{kind} {name} must be {ty:?}, not {t:?}"));
                }
                Some(_) => {}
            }
        }
        for (name, _) in &got {
            if !want.iter().any(|(n, _)| n == name) {
                problems.push(format!(
                    "unexpected {kind} {name}; the task does not define it"
                ));
            }
        }
    };
    compare(
        "input",
        task.inputs
            .iter()
            .map(|p| (p.name.as_str(), p.ty))
            .collect(),
        graph
            .inputs
            .iter()
            .map(|p| (p.name.as_str(), p.ty))
            .collect(),
    );
    compare(
        "output",
        task.outputs
            .iter()
            .map(|p| (p.name.as_str(), p.ty))
            .collect(),
        graph
            .outputs
            .iter()
            .map(|p| (p.name.as_str(), p.ty))
            .collect(),
    );
    problems
}

/// Checks one reply the way the loop does (interface, validator, emitter, engine load, replay),
/// without a model call. Used to re-score stored drafts on another engine revision.
pub fn check_reply(
    text: &str,
    task: &TaskSpec,
    palette: &Palette,
    vectors: &VectorFile,
) -> AttemptOutcome {
    check_draft(text, task, palette, vectors).0
}

fn check_draft(
    text: &str,
    task: &TaskSpec,
    palette: &Palette,
    vectors: &VectorFile,
) -> (AttemptOutcome, Option<VerifiedDraft>) {
    let mut graph: BlockGraph = match serde_json::from_str(text) {
        Ok(graph) => graph,
        Err(e) => {
            let detail = e.to_string();
            return (AttemptOutcome::NotAGraph { detail }, None);
        }
    };
    // The sequence name is ours to choose, not the model's: it names the CXF namespace.
    graph.name.clone_from(&task.name);
    let problems = interface_problems(&graph, task);
    if !problems.is_empty() {
        return (AttemptOutcome::InterfaceMismatch { problems }, None);
    }
    let findings = validate(&graph, palette);
    if !findings.is_empty() {
        return (AttemptOutcome::Invalid { findings }, None);
    }
    let cxf = match emit_cxf(&graph, palette, &EmitOptions::default()) {
        Ok(cxf) => cxf,
        Err(e) => {
            let detail = e.to_string();
            return (AttemptOutcome::NotAGraph { detail }, None);
        }
    };
    let report = match replay(&cxf, vectors, &ReplayLimits::default()) {
        Ok(report) => report,
        Err(mm_bridge::ReplayError::Load(failure)) => {
            return (AttemptOutcome::LoadFailed { failure }, None);
        }
        Err(mm_bridge::ReplayError::Invalid { code, detail }) => {
            // Checked before the loop, so this is a harness bug; never shown to the model.
            let detail = format!("vector file rejected ({code}): {detail}");
            return (AttemptOutcome::HarnessError { detail }, None);
        }
    };
    if report.passed() {
        let outcome = AttemptOutcome::Passed {
            content_id: report.content_id.clone(),
            scenarios: report.scenarios.len(),
        };
        let verified = VerifiedDraft {
            graph,
            cxf,
            content_id: report.content_id,
            scenarios: report.scenarios.len(),
        };
        return (outcome, Some(verified));
    }
    let failures: Vec<ScenarioReport> = report
        .scenarios
        .iter()
        .filter(|s| s.outcome != ScenarioOutcome::Pass)
        .cloned()
        .collect();
    let outcome = AttemptOutcome::ReplayFailed {
        passed: report.pass_count(),
        total: report.scenarios.len(),
        failures,
    };
    (outcome, None)
}
