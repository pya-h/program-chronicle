use std::time::Duration;

use svm_indexer::core::sse::SseEvent;

use crate::common::{pubkey, reset_db, server, signature};

/// Reads chunks off a live SSE response until `needle` shows up or the budget runs out.
async fn read_until(resp: &mut reqwest::Response, needle: &str) -> String {
    let mut buf = String::new();
    for _ in 0..40 {
        match tokio::time::timeout(Duration::from_millis(500), resp.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                buf.push_str(&String::from_utf8_lossy(&chunk));
                if buf.contains(needle) {
                    return buf;
                }
            }
            Ok(Ok(None)) => break, // stream closed
            Ok(Err(_)) => break,   // transport error
            Err(_) => {}           // per-chunk timeout: keep waiting for the publisher
        }
    }
    buf
}

#[tokio::test]
async fn sse_streams_a_published_account_event() {
    let _guard = reset_db().await;
    let srv = server();

    let mut resp = reqwest::Client::new()
        .get(format!("{}/sse/programs/7/accounts", srv.base_url))
        .send()
        .await
        .expect("failed to open sse stream");
    assert_eq!(resp.status(), 200);

    // publish repeatedly so the test tolerates the subscribe/connect race
    let publisher = {
        let tx = srv.sse_tx.clone();
        let address = pubkey(9);
        tokio::spawn(async move {
            for _ in 0..10 {
                let _ = tx.send(SseEvent::account_changed(7, address, 123, 0, vec![1, 2, 3]));
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    };

    let frame = read_until(&mut resp, "\"slot\":123").await;
    publisher.abort();

    assert!(frame.contains("account"), "expected an account event frame, got: {frame}");
    assert!(frame.contains(&pubkey(9).to_string()), "frame should carry the address");
}

#[tokio::test]
async fn sse_streams_a_published_program_event() {
    let _guard = reset_db().await;
    let srv = server();

    let mut resp = reqwest::Client::new()
        .get(format!("{}/sse/programs/8/events", srv.base_url))
        .send()
        .await
        .expect("failed to open sse stream");
    assert_eq!(resp.status(), 200);

    let publisher = {
        let tx = srv.sse_tx.clone();
        let sig = signature(5).as_ref().to_vec();
        tokio::spawn(async move {
            for _ in 0..10 {
                let _ = tx.send(SseEvent::program_event(8, sig.clone(), 0, 456, vec![9, 9, 9]));
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    };

    let frame = read_until(&mut resp, "\"slot\":456").await;
    publisher.abort();

    assert!(frame.contains("event"), "expected a program event frame, got: {frame}");
    assert!(frame.contains(&signature(5).to_string()), "frame should carry the signature");
}
