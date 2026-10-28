//! A run explanation may quote the ledger facts it was given, never a number of its own. A reply
//! with one is sent back once, naming it; a second such reply is refused.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::VecDeque;
use std::sync::Mutex;

use mm_agent::{ModelClient, ModelError, ModelReply, ModelRequest, Usage, explain_run};
use serde_json::{Value, json};

/// Answers with the scripted replies in order and keeps every request.
struct Scripted {
    replies: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl Scripted {
    fn new(replies: &[String]) -> Self {
        Self {
            replies: Mutex::new(replies.iter().cloned().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl ModelClient for Scripted {
    async fn complete(&self, request: &ModelRequest) -> Result<ModelReply, ModelError> {
        self.requests.lock().unwrap().push(request.clone());
        let text = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("more calls than replies");
        Ok(ModelReply {
            output_items: vec![json!({"type": "message", "role": "assistant",
                "content": [{"type": "output_text", "text": text}]})],
            text,
            usage: Usage::default(),
        })
    }
}

fn context() -> Value {
    json!({
        "run": {"from": "day 143 06:00", "steps": 240, "step_s": 120.0},
        "events": [{"when": "day 143 08:00 to day 143 08:18", "steps": 10}],
        "ledger": {
            "this_run": {"scope1_gas": {"kwh": 4395, "kg_co2e": 796, "usd": 105}},
            "vs_baseline": {"change": {"scope1_kg_co2e": -848, "total_usd": -269, "total_pct": -27.9}}
        }
    })
}

fn reply(summary: &str) -> String {
    json!({"summary": summary, "items": [
        {"when": "day 143 06:00", "title": "Run", "text": "Ran 240 steps."}
    ]})
    .to_string()
}

#[tokio::test]
async fn quoted_and_rounded_ledger_numbers_and_step_durations_pass() {
    let ok = Scripted::new(&[reply(
        "In simulation, boiler gas was 796 kg CO2e (about 4,400 kWh), about 850 kg less than \
         the baseline; cost fell by about $270 (27.9 %). The fault lasted 20 minutes.",
    )]);
    let (e, _) = explain_run(&ok, "muse-spark-1.3", &context())
        .await
        .unwrap();
    assert!(e.summary.contains("796"));
    assert_eq!(ok.requests.lock().unwrap().len(), 1, "no retry needed");
}

#[tokio::test]
async fn an_invented_number_is_sent_back_once_then_fixed() {
    let client = Scripted::new(&[
        reply("The run saved 1,150 kg CO2e, about 26 % of the baseline."),
        reply("The run saved 848 kg CO2e of gas in simulation."),
    ]);
    let (e, _) = explain_run(&client, "muse-spark-1.3", &context())
        .await
        .unwrap();
    assert!(e.summary.contains("848"));
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let last = requests[1].input.last().unwrap().to_string();
    assert!(last.contains("1,150") && last.contains("26"), "{last}");
}

#[tokio::test]
async fn a_second_invented_number_is_refused() {
    let client = Scripted::new(&[
        reply("The run saved 1,150 kg CO2e."),
        reply("The run saved 1,149 kg CO2e."),
    ]);
    let err = explain_run(&client, "muse-spark-1.3", &context())
        .await
        .unwrap_err();
    let ModelError::Malformed { message } = err else {
        panic!("{err:?}")
    };
    assert!(message.contains("1,149"), "{message}");
    assert_eq!(client.requests.lock().unwrap().len(), 2, "one retry only");
}
