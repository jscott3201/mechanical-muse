//! Responses API wire format, offline. Response bodies here are hand-built from the documented
//! shapes in docs/model-api.md (no live call has been recorded yet).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::time::Duration;

use mm_agent::{
    Effort, ModelError, ModelRequest, OutputSchema, SparkClient, parse_response, request_body,
    retry_delay,
};
use serde_json::json;

fn request() -> ModelRequest {
    ModelRequest {
        model: "muse-spark-1.3".into(),
        instructions: "rules".into(),
        input: vec![json!({"role": "user", "content": [{"type": "input_text", "text": "task"}]})],
        schema: Some(OutputSchema {
            name: "block_graph".into(),
            schema: json!({"type": "object"}),
        }),
        effort: Some(Effort::High),
        max_output_tokens: 32_000,
    }
}

#[test]
fn request_body_uses_the_responses_shapes() {
    let body = request_body(&request());
    assert_eq!(body["store"], false);
    assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
    assert_eq!(body["reasoning"]["effort"], "high");
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert_eq!(body["text"]["format"]["name"], "block_graph");
    assert_eq!(body["text"]["format"]["strict"], true);
    assert_eq!(body["max_output_tokens"], 32_000);
    for banned in [
        "response_format",
        "previous_response_id",
        "temperature",
        "stop",
    ] {
        assert!(body.get(banned).is_none(), "{banned} must not be sent");
    }
}

#[test]
fn optional_fields_are_omitted() {
    let mut r = request();
    r.effort = None;
    r.schema = None;
    let body = request_body(&r);
    assert!(body.get("reasoning").is_none());
    assert!(body.get("text").is_none());
}

#[test]
fn completed_response_yields_text_items_and_usage() {
    let body = json!({
        "id": "resp_1", "object": "response", "status": "completed",
        "output": [
            {"id": "rs_1", "type": "reasoning", "summary": [], "encrypted_content": "opaque"},
            {"id": "msg_1", "type": "message", "role": "assistant", "status": "completed",
             "content": [{"type": "output_text", "text": "{\"name\":\"x\"}", "annotations": []}]}
        ],
        "usage": {"input_tokens": 1200, "input_tokens_details": {"cached_tokens": 1000},
                  "output_tokens": 300, "output_tokens_details": {"reasoning_tokens": 250}}
    });
    let reply = parse_response(&body).unwrap();
    assert_eq!(reply.text, "{\"name\":\"x\"}");
    assert_eq!(reply.output_items.len(), 2);
    assert_eq!(reply.output_items[0]["encrypted_content"], "opaque");
    assert_eq!(
        (reply.usage.input_tokens, reply.usage.cached_tokens),
        (1200, 1000)
    );
    assert_eq!(
        (reply.usage.output_tokens, reply.usage.reasoning_tokens),
        (300, 250)
    );
}

#[test]
fn incomplete_refused_failed_and_empty_responses_are_errors() {
    let incomplete = json!({"status": "incomplete",
        "incomplete_details": {"reason": "max_output_tokens"}, "output": []});
    assert_eq!(
        parse_response(&incomplete).unwrap_err(),
        ModelError::Incomplete {
            reason: "max_output_tokens".into()
        }
    );
    let refused = json!({"status": "completed", "output": [{"type": "message",
        "content": [{"type": "refusal", "refusal": "cannot help"}]}]});
    assert!(matches!(
        parse_response(&refused).unwrap_err(),
        ModelError::Refusal { .. }
    ));
    let failed = json!({"status": "failed",
        "error": {"code": "server_shutting_down", "message": "restarting"}});
    assert_eq!(
        parse_response(&failed).unwrap_err(),
        ModelError::Failed {
            code: "server_shutting_down".into(),
            message: "restarting".into()
        }
    );
    let only_reasoning = json!({"status": "completed",
        "output": [{"type": "reasoning", "summary": []}]});
    assert!(matches!(
        parse_response(&only_reasoning).unwrap_err(),
        ModelError::Malformed { .. }
    ));
}

#[test]
fn retries_follow_the_documented_policy() {
    let s = Duration::from_secs;
    assert_eq!(retry_delay(429, Some(s(30)), 0), Some(s(30)));
    assert_eq!(retry_delay(503, Some(s(1)), 3), Some(s(8)));
    assert_eq!(retry_delay(500, None, 0), Some(s(1)));
    assert_eq!(retry_delay(500, None, 2), Some(s(4)));
    // A huge Retry-After is capped at two minutes.
    assert_eq!(retry_delay(429, Some(s(3600)), 0), Some(s(120)));
    for status in [400, 401, 402, 403, 404, 413, 504] {
        assert_eq!(retry_delay(status, None, 0), None, "{status}");
    }
}

#[test]
fn the_key_never_shows_in_debug_output() {
    assert!(matches!(
        SparkClient::new("  ".into()),
        Err(ModelError::Config { .. })
    ));
    let client = SparkClient::new("LLM_secret_value".into()).unwrap();
    let shown = format!("{client:?}");
    assert!(!shown.contains("LLM_secret_value"), "{shown}");
}

#[test]
fn commentary_messages_are_skipped() {
    let body = json!({"status": "completed", "output": [
        {"type": "message", "phase": "commentary",
         "content": [{"type": "output_text", "text": "Let me think about the wiring."}]},
        {"type": "message", "content": [{"type": "output_text", "text": "{\"a\":1}"}]}
    ]});
    assert_eq!(parse_response(&body).unwrap().text, "{\"a\":1}");
}
