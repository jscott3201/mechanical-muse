//! SparkClient's retry loop against a local stand-in server (no network beyond loopback).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mm_agent::{ModelClient, ModelError, ModelRequest, SparkClient};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const KEY: &str = "LLM_test_key_do_not_echo";

/// Serves one canned HTTP response per connection, in order, and counts requests.
async fn serve(responses: Vec<(u16, &'static str, String)>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    tokio::spawn(async move {
        for (status, extra_headers, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_request(&mut socket).await;
            seen.fetch_add(1, Ordering::SeqCst);
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n{extra_headers}\r\n{body}",
                body.len()
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
            socket.shutdown().await.ok();
        }
    });
    (url, count)
}

/// Reads headers and a Content-Length body so the client sees its request fully consumed.
async fn read_request(socket: &mut tokio::net::TcpStream) {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        if n == 0 {
            return;
        }
        data.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&data);
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if data.len() >= end + 4 + length {
                return;
            }
        }
    }
}

fn ok_body() -> String {
    json!({"status": "completed", "output": [
        {"type": "message", "content": [{"type": "output_text", "text": "{}"}]}
    ]})
    .to_string()
}

fn request() -> ModelRequest {
    ModelRequest {
        model: "muse-spark-1.3".into(),
        instructions: "rules".into(),
        input: vec![],
        schema: None,
        effort: None,
        max_output_tokens: 1000,
    }
}

fn client(url: &str) -> SparkClient {
    SparkClient::new(KEY.into()).unwrap().with_base_url(url)
}

#[tokio::test]
async fn retries_a_503_then_succeeds() {
    let (url, count) = serve(vec![
        (
            503,
            "Retry-After: 0\r\n",
            r#"{"error":{"message":"overloaded"}}"#.into(),
        ),
        (200, "", ok_body()),
    ])
    .await;
    let reply = client(&url).complete(&request()).await.unwrap();
    assert_eq!(reply.text, "{}");
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_400_is_not_retried() {
    let (url, count) = serve(vec![(
        400,
        "",
        r#"{"error":{"message":"bad schema"}}"#.into(),
    )])
    .await;
    let err = client(&url).complete(&request()).await.unwrap_err();
    assert_eq!(
        err,
        ModelError::Http {
            status: 400,
            message: "bad schema".into()
        }
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_echoed_key_is_scrubbed_from_errors() {
    let body = format!(r#"{{"error":{{"message":"invalid key {KEY}"}}}}"#);
    let (url, _) = serve(vec![(401, "", body)]).await;
    let err = client(&url).complete(&request()).await.unwrap_err();
    let shown = err.to_string();
    assert!(!shown.contains(KEY), "{shown}");
    assert!(shown.contains("<redacted>"), "{shown}");
}

#[tokio::test]
async fn a_transient_failed_status_is_retried() {
    let failed = json!({"status": "failed",
        "error": {"code": "server_shutting_down", "message": "restarting"}})
    .to_string();
    let (url, count) = serve(vec![(200, "", failed), (200, "", ok_body())]).await;
    let reply = client(&url).complete(&request()).await.unwrap();
    assert_eq!(reply.text, "{}");
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
