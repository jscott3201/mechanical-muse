//! Recorded model replies, played back in order. This is the demo's "recorded model" fallback
//! and an offline test double: the validator, the engine and the vectors still run for real on
//! every draft; only the model call is a recording, and the UI must say so.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

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
    pub fn from_trace(trace: &AuthoringTrace) -> Self {
        let replies = trace
            .attempts
            .iter()
            .filter_map(|a| match (&a.reply, &a.outcome) {
                (Some(text), _) => Some(RecordedReply::Reply {
                    text: text.clone(),
                    usage: a.usage,
                }),
                (None, AttemptOutcome::ModelError { error }) => Some(RecordedReply::Error {
                    error: error.clone(),
                }),
                (None, _) => None,
            })
            .collect();
        Self {
            model: trace.config.model.clone(),
            task: trace.task.clone(),
            note: String::new(),
            task_digest: None,
            replies,
        }
    }
}

/// Plays a [`Recording`] back, one reply per call, each after `pace` (so a live view shows the
/// drafting state the way a real call would).
pub struct RecordedClient {
    replies: Mutex<VecDeque<RecordedReply>>,
    pace: Duration,
}

impl RecordedClient {
    pub fn new(recording: Recording, pace: Duration) -> Self {
        Self {
            replies: Mutex::new(recording.replies.into()),
            pace,
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
            None => Err(ModelError::Config {
                message: "the recording has no more replies".into(),
            }),
        }
    }
}
