//! The model-facing seam. Spark, Wonder and a Glimmer endpoint each implement `ModelClient`;
//! wire types stay inside each implementation, and tests use scripted fakes.

use std::future::Future;

use serde::Serialize;
use serde_json::Value;

/// Reasoning effort. `none` is not offered: Muse models always reason and the API rejects it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Minimal => "minimal",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

/// A JSON Schema the reply must follow (sent with `strict: true`).
#[derive(Debug, Clone, PartialEq)]
pub struct OutputSchema {
    pub name: String,
    pub schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequest {
    pub model: String,
    /// Stable instructions first, so the provider's prefix cache can reuse them.
    pub instructions: String,
    /// Conversation items in the Responses `input` shape. Earlier replies' output items are
    /// replayed verbatim, which carries encrypted reasoning across repair turns.
    pub input: Vec<Value>,
    pub schema: Option<OutputSchema>,
    pub effort: Option<Effort>,
    /// Reasoning tokens count against this budget too.
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, other: Usage) {
        self.input_tokens += other.input_tokens;
        self.cached_tokens += other.cached_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelReply {
    /// Every output item, unmodified, for replay in the next turn.
    pub output_items: Vec<Value>,
    /// The assistant's text (the JSON document when a schema was set).
    pub text: String,
    pub usage: Usage,
}

/// Why a call produced no usable text. Messages never contain the API key or request bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ModelError {
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("transport error: {message}")]
    Transport { message: String },
    #[error("model refused: {message}")]
    Refusal { message: String },
    #[error("response incomplete: {reason}")]
    Incomplete { reason: String },
    #[error("unexpected response shape: {message}")]
    Malformed { message: String },
    #[error("gave up after {attempts} attempts: {last}")]
    RetriesExhausted { attempts: u32, last: String },
    #[error("configuration: {message}")]
    Config { message: String },
}

pub trait ModelClient {
    fn complete(
        &self,
        request: &ModelRequest,
    ) -> impl Future<Output = Result<ModelReply, ModelError>> + Send;
}
