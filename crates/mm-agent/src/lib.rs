//! Muse Spark authoring agent: a `ModelClient` trait, the Meta Model API (Responses) client,
//! prompts built from the engine catalog, and the draft-verify-repair loop.
//!
//! Model output is untrusted (AGENTS.md hard rule 5): a draft becomes a verified sequence only
//! when `mm-graph` accepts it and every `mm-bridge` replay scenario passes.

#![forbid(unsafe_code)]

mod client;
mod prompt;
mod recorded;
mod repair;
mod responses;

pub use client::{Effort, ModelClient, ModelError, ModelReply, ModelRequest, OutputSchema, Usage};
pub use prompt::{
    Exemplar, ParamHint, PointSpec, TaskSpec, palette_listing, system_instructions, task_message,
};
pub use recorded::{RecordedClient, RecordedReply, Recording};
pub use repair::{
    Attempt, AttemptOutcome, AuthorConfig, AuthoringTrace, Feedback, Progress, VerifiedDraft,
    author, author_observed, check_reply, interface_problems,
};
pub use responses::{SparkClient, parse_response, request_body, retry_delay};
