//! Recorded model replies, played back in order. This is the demo's "recorded model" fallback
//! and an offline test double: the validator, the engine and the vectors still run for real on
//! every draft; only the model call is a recording, and the UI must say so.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::client::{ModelClient, ModelError, ModelReply, ModelRequest, Usage};
use crate::repair::{AttemptOutcome, AuthoringTrace};

/// One model call as it came back: the reply text, or the error the call ended with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RecordedReply {
    Reply {
        text: String,
        #[serde(default)]
        usage: Usage,
    },
    Error {
        error: ModelError,
    },
    /// A tool turn: the calls the model asked for (tools sessions only). Playback answers them
    /// by running our checks again; call ids are made up on playback, none are stored.
    ToolCalls {
        calls: Vec<RecordedCall>,
        #[serde(default)]
        usage: Usage,
    },
}

/// One recorded function call: the tool name and its raw arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedCall {
    pub name: String,
    pub arguments: String,
}

/// The replies of one authoring run, with the model they came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recording {
    pub model: String,
    pub task: String,
    /// Where and when it was recorded, in a few words.
    #[serde(default)]
    pub note: String,
    /// Digest of the task the replies answered, without its name (`task:<hex>`), so the same
    /// task accepted under another name finds this recording.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_digest: Option<String>,
    pub replies: Vec<RecordedReply>,
}

impl Recording {
    /// Every model call of `trace`, in order. Encrypted reasoning items are not kept: a
    /// recording never goes back to the API, so the replay only needs the text.
    /// Tool turns come before their attempt's answer, in call order.
    pub fn from_trace(trace: &AuthoringTrace) -> Self {
        let mut replies = Vec::new();
        for a in &trace.attempts {
            let mut tool_usage = Usage::default();
            for turn in &a.tool_turns {
                tool_usage.add(turn.usage);
                replies.push(RecordedReply::ToolCalls {
                    calls: turn
                        .calls
                        .iter()
                        .map(|c| RecordedCall {
                            name: c.name.clone(),
                            arguments: c.arguments.clone(),
                        })
                        .collect(),
                    usage: turn.usage,
                });
            }
            match (&a.reply, &a.outcome) {
                (Some(text), _) => replies.push(RecordedReply::Reply {
                    text: text.clone(),
                    usage: a.usage.minus(tool_usage),
                }),
                (None, AttemptOutcome::ModelError { error }) => {
                    replies.push(RecordedReply::Error {
                        error: error.clone(),
                    });
                }
                (None, _) => {}
            }
        }
        Self {
            model: trace.config.model.clone(),
            task: trace.task.clone(),
            note: String::new(),
            task_digest: None,
            replies,
        }
    }

    /// True when the recording has tool turns, so playback must offer the tools.
    pub fn uses_tools(&self) -> bool {
        self.replies
            .iter()
            .any(|r| matches!(r, RecordedReply::ToolCalls { .. }))
    }

    /// Drafting attempts in the recording (answers and failed calls, not tool turns).
    pub fn attempts(&self) -> usize {
        self.replies
            .iter()
            .filter(|r| !matches!(r, RecordedReply::ToolCalls { .. }))
            .count()
    }
}

/// Plays a [`Recording`] back, one reply per call, each after `pace` (so a live view shows the
/// drafting state the way a real call would).
pub struct RecordedClient {
    replies: Mutex<VecDeque<RecordedReply>>,
    pace: Duration,
    /// Numbers the made-up call ids of tool turns.
    calls: std::sync::atomic::AtomicU32,
}

impl RecordedClient {
    pub fn new(recording: Recording, pace: Duration) -> Self {
        Self {
            replies: Mutex::new(recording.replies.into()),
            pace,
            calls: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn next(&self) -> Option<RecordedReply> {
        self.replies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()
    }
}

impl ModelClient for RecordedClient {
    async fn complete(&self, _request: &ModelRequest) -> Result<ModelReply, ModelError> {
        tokio::time::sleep(self.pace).await;
        match self.next() {
            Some(RecordedReply::Reply { text, usage }) => Ok(ModelReply {
                output_items: vec![json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": text}],
                })],
                text,
                usage,
            }),
            Some(RecordedReply::Error { error }) => Err(error),
            Some(RecordedReply::ToolCalls { calls, usage }) => {
                let output_items: Vec<Value> = calls
                    .into_iter()
                    .map(|c| {
                        let n = self
                            .calls
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                            + 1;
                        json!({"type": "function_call", "call_id": format!("call_{n}"),
                               "name": c.name, "arguments": c.arguments})
                    })
                    .collect();
                Ok(ModelReply {
                    output_items,
                    text: String::new(),
                    usage,
                })
            }
            None => Err(ModelError::Config {
                message: "the recording has no more replies".into(),
            }),
        }
    }
}
