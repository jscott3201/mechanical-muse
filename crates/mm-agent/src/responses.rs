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
            let field = |key: &str| {
                body.pointer(&format!("/error/{key}"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            return Err(ModelError::Failed {
                code: field("code"),
                message: field("message"),
            });
        }
        Some(other) => return Err(malformed(&format!("status {other}"))),
        None => return Err(malformed("no status")),
    }
    let items = body
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed("no output array"))?;
    // The answer is the last message item that is not commentary; earlier messages (phase
    // "commentary") are preambles, and joining them would break the JSON.
    let answer = items.iter().rev().find(|item| {
        item.get("type").and_then(Value::as_str) == Some("message")
            && item.get("phase").and_then(Value::as_str) != Some("commentary")
    });
    let mut text = String::new();
    for part in answer
        .and_then(|item| item.get("content"))
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
    if text.is_empty() {
        return Err(malformed("no output_text in the final message item"));
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

/// Longest wait we accept from a `Retry-After` header before giving up instead.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(120);

/// How long to wait before retrying, or `None` if the status is not retryable. Retries 429, 500
/// and 503 with doubling backoff from 1 s (`attempt` counts from 0), never shorter than the
/// server's `Retry-After`, which is capped at two minutes. No jitter: one process, few calls.
pub fn retry_delay(status: u16, retry_after: Option<Duration>, attempt: u32) -> Option<Duration> {
    if !matches!(status, 429 | 500 | 503) {
        return None;
    }
    Some(backoff(attempt).max(retry_after.unwrap_or_default().min(MAX_RETRY_AFTER)))
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(1u64 << attempt.min(6))
}

/// `status: "failed"` codes that the docs say to retry.
fn transient_failure(code: &str) -> bool {
    matches!(
        code,
        "server_shutting_down" | "server_error" | "service_overloaded" | "backend_unavailable"
    )
}

/// A failed send: the error, plus whether and when it may be retried.
struct SendFailure {
    error: ModelError,
    retry: Option<Duration>,
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

    async fn send_once(&self, body: &Value, attempt: u32) -> Result<Value, SendFailure> {
        // Only a failed connection is retried: after a timeout or a broken body the request may
        // already be running and billed, so resending could double the cost.
        let transport = |e: reqwest::Error, connect_only: bool| SendFailure {
            retry: (connect_only && e.is_connect()).then(|| backoff(attempt)),
            error: ModelError::Transport {
                message: self.scrub(&e.without_url().to_string()),
            },
        };
        let response = self
            .http
            .post(format!("{}/responses", self.base_url))
            .bearer_auth(&self.api_key.0)
            .json(body)
            .send()
            .await
            .map_err(|e| transport(e, true))?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let text = response.text().await.map_err(|e| transport(e, false))?;
        if (200..300).contains(&status) {
            return serde_json::from_str(&text).map_err(|e| SendFailure {
                error: ModelError::Malformed {
                    message: format!("body is not JSON: {e}"),
                },
                retry: None,
            });
        }
        // Error bodies are usually {"error":{"message":...}}, sometimes empty or plain text.
        let message = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.pointer("/error/message")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| text.chars().take(300).collect());
        Err(SendFailure {
            error: ModelError::Http {
                status,
                message: self.scrub(&message),
            },
            retry: retry_delay(status, retry_after, attempt),
        })
    }

    /// Error text can echo request details; make sure the key is never among them.
    fn scrub(&self, text: &str) -> String {
        text.replace(self.api_key.0.as_str(), "<redacted>")
    }
}

impl ModelClient for SparkClient {
    async fn complete(&self, request: &ModelRequest) -> Result<ModelReply, ModelError> {
        let body = request_body(request);
        let mut attempt = 0;
        loop {
            let failure = match self.send_once(&body, attempt).await {
                Ok(json) => match parse_response(&json) {
                    Err(ModelError::Failed { code, message }) if transient_failure(&code) => {
                        SendFailure {
                            error: ModelError::Failed { code, message },
                            retry: Some(backoff(attempt)),
                        }
                    }
                    parsed => return parsed,
                },
                Err(failure) => failure,
            };
            attempt += 1;
            match failure.retry {
                Some(delay) if attempt < self.max_attempts => tokio::time::sleep(delay).await,
                Some(_) => {
                    return Err(ModelError::RetriesExhausted {
                        attempts: attempt,
                        last: failure.error.to_string(),
                    });
                }
                None => return Err(failure.error),
            }
        }
    }
}
