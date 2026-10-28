//! The model-facing seam. The live Muse Spark client (`SparkClient`) and recorded playback
//! (`RecordedClient`) implement `ModelClient`, and tests use scripted fakes; wire types stay
//! inside each implementation. Other Meta endpoints would plug in here the same way.

use std::future::Future;

use serde::{Deserialize, Serialize};
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
    /// Function tools in the Responses shape (`{type:"function", name, description,
    /// parameters, strict}`). Empty means none are offered. Only the product's drafting loop
    /// offers any; benchmark runs never do.
    pub tools: Vec<Value>,
    pub effort: Option<Effort>,
    /// Reasoning tokens count against this budget too.
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

    /// `self` less `other`, field by field, never below zero.
    pub fn minus(self, other: Usage) -> Usage {
        Usage {
            input_tokens: self.input_tokens.saturating_sub(other.input_tokens),
            cached_tokens: self.cached_tokens.saturating_sub(other.cached_tokens),
            output_tokens: self.output_tokens.saturating_sub(other.output_tokens),
            reasoning_tokens: self.reasoning_tokens.saturating_sub(other.reasoning_tokens),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelReply {
    /// Every output item, unmodified, for replay in the next turn.
    pub output_items: Vec<Value>,
    /// The assistant's text (the JSON document when a schema was set). Empty when the reply is
    /// only tool calls.
    pub text: String,
    pub usage: Usage,
}

/// Why a call produced no usable text. Messages never contain the API key or request bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
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
    /// The response came back with `status: "failed"`.
    #[error("response failed ({code}): {message}")]
    Failed { code: String, message: String },
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

/// One function call the model asked for. `arguments` is the raw JSON string, untrusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCall {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

/// The `function_call` items of a reply, in output order. Items without a name or call id are
/// skipped (they cannot be answered).
pub fn function_calls(items: &[Value]) -> Vec<FunctionCall> {
    items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .filter_map(|item| {
            let field = |key: &str| item.get(key).and_then(Value::as_str).map(str::to_owned);
            Some(FunctionCall {
                call_id: field("call_id")?,
                name: field("name")?,
                arguments: field("arguments").unwrap_or_default(),
            })
        })
        .collect()
}
