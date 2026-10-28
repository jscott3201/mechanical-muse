//! A run explanation may quote the ledger facts it was given, never a number of its own.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use mm_agent::{ModelClient, ModelError, ModelReply, ModelRequest, Usage, explain_run};
use serde_json::json;

/// Always answers with one fixed explanation.
struct Fixed(String);

impl ModelClient for Fixed {
    async fn complete(&self, _request: &ModelRequest) -> Result<ModelReply, ModelError> {
        Ok(ModelReply {
            output_items: Vec::new(),
            text: self.0.clone(),
            usage: Usage::default(),
        })
    }
}

fn context() -> serde_json::Value {
    json!({
        "run": {"from": "day 143 06:00", "steps": 240, "step_s": 120.0},
        "events": [],
        "ledger": {
            "this_run": {"scope1_gas": {"kwh": 4395, "kg_co2e": 796, "usd": 105}},
            "vs_baseline": {"change": {"scope1_kg_co2e": -848, "total_usd": -269, "total_pct": -27.9}}
        }
    })
}

fn reply(summary: &str) -> Fixed {
    Fixed(
        json!({"summary": summary, "items": [
            {"when": "day 143 06:00", "title": "Run", "text": "Ran 240 steps."}
        ]})
        .to_string(),
    )
}

#[tokio::test]
async fn quoted_ledger_numbers_pass() {
    let ok = reply(
        "In simulation, boiler gas was 796 kg CO2e, 848 kg less than the baseline, \
         and cost fell by $269 (27.9 %).",
    );
    let (e, _) = explain_run(&ok, "muse-spark-1.3", &context())
        .await
        .unwrap();
    assert!(e.summary.contains("796"));
}

#[tokio::test]
async fn an_invented_number_is_refused() {
    let made_up = reply("The run saved 1,150 kg CO2e, about 26 % of the baseline.");
    let err = explain_run(&made_up, "muse-spark-1.3", &context())
        .await
        .unwrap_err();
    let ModelError::Malformed { message } = err else {
        panic!("{err:?}")
    };
    assert!(
        message.contains("1,150") && message.contains("26"),
        "{message}"
    );
}
