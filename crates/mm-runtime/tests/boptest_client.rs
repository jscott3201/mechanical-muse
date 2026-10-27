//! The BOPTEST client against canned HTTP replies (a local one-shot server per request), for
//! the reply shapes BOPTEST v0.9.0 actually sends.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use mm_runtime::{BoptestClient, Plant, PlantError, Writes};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Serves `replies` in order, one per connection, and returns the base URL.
async fn serve(replies: Vec<(u16, &'static str)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        for (status, body) in replies {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16 * 1024];
            // Read the request head (and any small body) before answering.
            let _ = sock.read(&mut buf).await.unwrap();
            let reply = format!(
                "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(reply.as_bytes()).await.unwrap();
        }
    });
    base
}

const SELECTED: (u16, &str) = (200, r#"{"testid":"t1"}"#);

async fn client(rest: Vec<(u16, &'static str)>) -> BoptestClient {
    let mut replies = vec![SELECTED];
    replies.extend(rest);
    let base = serve(replies).await;
    BoptestClient::select(&base, "case").await.unwrap()
}

#[tokio::test]
async fn stop_accepts_a_plain_ok() {
    let plant = client(vec![(200, "OK")]).await;
    assert_eq!(plant.testid(), "t1");
    plant.stop().await.unwrap();
}

#[tokio::test]
async fn a_non_json_error_keeps_its_status() {
    let mut plant = client(vec![(500, "Internal Server Error")]).await;
    let err = plant.advance(&Writes::new()).await.unwrap_err();
    assert_eq!(
        err,
        PlantError::Refused {
            status: 500,
            message: "Internal Server Error".into()
        }
    );
}

#[tokio::test]
async fn a_json_error_reports_its_message() {
    let mut plant = client(vec![(400, r#"{"status":400,"message":"bad key"}"#)]).await;
    let err = plant.advance(&Writes::new()).await.unwrap_err();
    assert_eq!(
        err,
        PlantError::Refused {
            status: 400,
            message: "bad key".into()
        }
    );
}

#[tokio::test]
async fn advance_reads_numbers_and_detects_the_end() {
    let mut plant = client(vec![
        (
            200,
            r#"{"status":200,"payload":{"time":120,"x":1.5,"s":"no"}}"#,
        ),
        (200, r#"{"status":200,"payload":{}}"#),
        (200, r#"{"status":200,"message":"no payload"}"#),
    ])
    .await;
    let m = plant.advance(&Writes::new()).await.unwrap();
    assert_eq!(m.len(), 2, "the string reading is dropped: {m:?}");
    assert_eq!(m["x"], 1.5);
    assert_eq!(
        plant.advance(&Writes::new()).await.unwrap_err(),
        PlantError::Ended
    );
    assert!(matches!(
        plant.advance(&Writes::new()).await.unwrap_err(),
        PlantError::Malformed { .. }
    ));
}

#[tokio::test]
async fn select_without_a_testid_is_malformed() {
    let base = serve(vec![(200, r#"{"status":200}"#)]).await;
    let err = BoptestClient::select(&base, "case").await.unwrap_err();
    assert!(matches!(err, PlantError::Malformed { .. }), "{err:?}");
}
