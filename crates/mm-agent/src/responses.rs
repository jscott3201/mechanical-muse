//! Meta Model API, Responses endpoint (`POST /v1/responses`). Shapes follow
//! `docs/model-api.md`; request building and response parsing are pure functions so they are
//! tested offline.

use std::fmt;
use std::time::Duration;

use serde_json::{Value, json};

use crate::client::{ModelClient, ModelError, ModelReply, ModelRequest, Usage};

const DEFAULT_BASE_URL: &str = "https://api.meta.ai/v1";

/// Builds the request body. `store: false` keeps nothing server-side, and the encrypted
/// reasoning comes back so the next turn can replay it (the two must not be combined with
/// `previous_response_id`, which we never send).
pub fn request_body(request: &ModelRequest) -> Value {
    let mut body = serde_json::Map::new();
    body.insert("model".into(), json!(request.model));
    body.insert("instructions".into(), json!(request.instructions));
    body.insert("input".into(), json!(request.input));
    body.insert("store".into(), json!(false));
    body.insert("include".into(), json!(["reasoning.encrypted_content"]));
    body.insert("max_output_tokens".into(), json!(request.max_output_tokens));
    if let Some(effort) = request.effort {
        body.insert("reasoning".into(), json!({ "effort": effort.as_str() }));
    }
    if let Some(schema) = &request.schema {
        // Flat `text.format`; the nested `response_format` shape is Chat Completions (a 400).
        body.insert(
            "text".into(),
            json!({
                "format": {
                    "type": "json_schema",
                    "name": schema.name,
                    "schema": schema.schema,
                    "strict": true,
                }
            }),
        );
    }
    Value::Object(body)
}

/// Reads a Responses body: checks `status`, then walks `output[]` for message text and refusals.
pub fn parse_response(body: &Value) -> Result<ModelReply, ModelError> {
    let malformed = |message: &str| ModelError::Malformed {
        message: message.to_owned(),
    };
    match body.get("status").and_then(Value::as_str) {
        Some("completed") => {}
        Some("incomplete") => {
            let reason = body
                .pointer("/incomplete_details/reason")
                .and_then(Value::as_str)
                .unwrap_or("unspecified");
            return Err(ModelError::Incomplete {
                reason: reason.to_owned(),
            });
        }
        Some("failed") => {
            let message = body
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("response failed");
            return Err(malformed(message));
        }
        Some(other) => return Err(malformed(&format!("status {other}"))),
        None => return Err(malformed("no status")),
    }
    let items = body
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed("no output array"))?;
    let mut text = String::new();
    for item in items {
        if item.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        for part in item
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match part.get("type").and_then(Value::as_str) {
                Some("output_text") => {
                    text.push_str(part.get("text").and_then(Value::as_str).unwrap_or_default());
                }
                Some("refusal") => {
                    let message = part
                        .get("refusal")
                        .and_then(Value::as_str)
                        .unwrap_or("refused");
                    return Err(ModelError::Refusal {
                        message: message.to_owned(),
                    });
                }
                _ => {}
            }
        }
    }
    if text.is_empty() {
        return Err(malformed("no output_text in any message item"));
    }
    let count = |path: &str| body.pointer(path).and_then(Value::as_u64).unwrap_or(0);
    Ok(ModelReply {
        output_items: items.clone(),
        text,
        usage: Usage {
            input_tokens: count("/usage/input_tokens"),
            cached_tokens: count("/usage/input_tokens_details/cached_tokens"),
            output_tokens: count("/usage/output_tokens"),
            reasoning_tokens: count("/usage/output_tokens_details/reasoning_tokens"),
        },
    })
}

/// How long to wait before retrying, or `None` if the status is not retryable. Retries 429, 500
/// and 503 with doubling backoff from 1 s, never shorter than the server's `Retry-After`.
pub fn retry_delay(status: u16, retry_after: Option<Duration>, attempt: u32) -> Option<Duration> {
    if !matches!(status, 429 | 500 | 503) {
        return None;
    }
    let backoff = Duration::from_secs(1u64 << attempt.min(6));
    Some(retry_after.map_or(backoff, |server| server.max(backoff)))
}

/// The key, kept out of `Debug` output and logs.
struct ApiKey(String);

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

/// Muse Spark (or any model served by the Responses endpoint).
#[derive(Debug)]
pub struct SparkClient {
    http: reqwest::Client,
    base_url: String,
    api_key: ApiKey,
    max_attempts: u32,
}

impl SparkClient {
    /// `mm-server` reads the key from `MODEL_API_KEY` and passes it here; nothing else should.
    pub fn new(api_key: String) -> Result<Self, ModelError> {
        if api_key.trim().is_empty() {
            return Err(ModelError::Config {
                message: "empty API key".into(),
            });
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            // High-effort reasoning can take minutes; streaming will replace this cap later.
            .timeout(Duration::from_secs(900))
            .build()
            .map_err(|e| ModelError::Config {
                message: e.to_string(),
            })?;
        Ok(Self {
            http,
            base_url: DEFAULT_BASE_URL.to_owned(),
            api_key: ApiKey(api_key),
            max_attempts: 4,
        })
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    async fn send_once(&self, body: &Value) -> Result<Value, (ModelError, Option<Duration>)> {
        let response = self
            .http
            .post(format!("{}/responses", self.base_url))
            .bearer_auth(&self.api_key.0)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                let message = e.without_url().to_string();
                (ModelError::Transport { message }, None)
            })?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let text = response.text().await.map_err(|e| {
            let message = e.without_url().to_string();
            (ModelError::Transport { message }, None)
        })?;
        if (200..300).contains(&status) {
            return serde_json::from_str(&text).map_err(|e| {
                let message = format!("body is not JSON: {e}");
                (ModelError::Malformed { message }, None)
            });
        }
        // Error bodies are usually {"error":{"message":...}}, sometimes empty or plain text.
        let message = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.pointer("/error/message")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| text.chars().take(300).collect());
        Err((ModelError::Http { status, message }, retry_after))
    }
}

impl ModelClient for SparkClient {
    async fn complete(&self, request: &ModelRequest) -> Result<ModelReply, ModelError> {
        let body = request_body(request);
        let mut attempt = 0;
        loop {
            attempt += 1;
            let (error, retry_after) = match self.send_once(&body).await {
                Ok(json) => return parse_response(&json),
                Err(failure) => failure,
            };
            let delay = match &error {
                ModelError::Http { status, .. } => retry_delay(*status, retry_after, attempt - 1),
                ModelError::Transport { .. } => Some(Duration::from_secs(1u64 << attempt.min(6))),
                _ => None,
            };
            match delay {
                Some(delay) if attempt < self.max_attempts => tokio::time::sleep(delay).await,
                Some(_) => {
                    return Err(ModelError::RetriesExhausted {
                        attempts: attempt,
                        last: error.to_string(),
                    });
                }
                None => return Err(error),
            }
        }
    }
}
