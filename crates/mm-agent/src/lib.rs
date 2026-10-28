//! Muse Spark authoring agent: a `ModelClient` trait, the Meta Model API (Responses) client,
//! prompts built from the engine catalog, and the draft-verify-repair loop.
//!
//! Model output is untrusted (AGENTS.md hard rule 5): a draft becomes a verified sequence only
//! when `mm-graph` accepts it and every `mm-bridge` replay scenario passes.

#![forbid(unsafe_code)]

mod binder;
mod client;
mod explain;
mod feedback;
mod numbers;
mod prompt;
mod reader;
mod recorded;
mod repair;
mod responses;
mod task_check;
mod tool_loop;
mod tools;

pub use binder::{bind_request, propose_binding};
pub use client::{
    Effort, FunctionCall, ModelClient, ModelError, ModelReply, ModelRequest, OutputSchema, Usage,
    function_calls,
};
pub use explain::{
    Explained, ExplainedPart, Explanation, RunExplainedItem, RunExplanation, check_explanation,
    check_run_explanation, explain, explain_request, explain_run, explain_run_request,
};
pub use numbers::unsupported_numbers;
pub use prompt::{
    AUTHORING_RULES, Exemplar, ParamHint, PointSpec, TaskSpec, palette_listing,
    system_instructions, task_message,
};
pub use reader::{
    Attachment, ReadOutcome, ReadSequence, SequenceSource, check_reading, read_request,
    read_sequence,
};
pub use recorded::{RecordedCall, RecordedClient, RecordedReply, Recording};
pub use repair::{
    Attempt, AttemptOutcome, AuthorConfig, AuthoringTrace, Feedback, GraphCheck, Progress,
    VerifiedDraft, author, author_observed, author_with_tools, check_graph, check_reply,
    interface_problems, repair_message,
};
pub use responses::{SparkClient, parse_response, request_body, retry_delay};
pub use task_check::check_task;
pub use tool_loop::{ToolCallRecord, ToolTurn};
pub use tools::{
    CHECK_GRAPH, DESCRIBE_BLOCK, DraftTools, MAX_OUTPUT_BYTES, REPLAY_SCENARIO, ToolLimits,
    ToolResult, summarize_arguments,
};
