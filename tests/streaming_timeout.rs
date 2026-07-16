//! Regression test for the streaming-timeout fix.
//!
//! The shared reqwest client must NOT apply its total-request `timeout` to a
//! streaming SSE response: a long-but-actively-flowing generation whose total
//! duration exceeds the configured timeout — yet is never idle for more than a
//! fraction of it — must complete, not be truncated with `Error::Timeout`.
//!
//! This drives a raw TCP server that trickles a complete, valid SSE message in
//! steps whose sum exceeds the client timeout while each inter-write gap stays
//! well under it. Before the fix (total `timeout` applied to streams) this
//! returned `Err(Error::Timeout)`; after it (idle `read_timeout` instead) it
//! succeeds.

use std::time::Duration;

use crimson_crab::api::MessagesRequest;
use crimson_crab::types::MessageParam;
use crimson_crab::Client;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// The SSE event records, delivered one per step.
const RECORDS: &[&str] = &[
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":12}}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
];

#[tokio::test]
async fn streaming_is_not_bounded_by_total_request_timeout() {
    // Each inter-write gap (90ms) is far below the client timeout (400ms), but
    // the total trickle duration (~630ms across 7 records) exceeds it.
    let step = Duration::from_millis(90);
    let client_timeout = Duration::from_millis(400);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        // Drain the request head (a small POST fits in one read).
        let mut buf = [0u8; 8192];
        let _ = socket.read(&mut buf).await.expect("read request");

        // Respond with a chunked SSE body, flushing one record per step.
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n",
            )
            .await
            .expect("write head");
        socket.flush().await.expect("flush head");

        for record in RECORDS {
            let chunk = format!("{:x}\r\n{record}\r\n", record.len());
            socket
                .write_all(chunk.as_bytes())
                .await
                .expect("write chunk");
            socket.flush().await.expect("flush chunk");
            tokio::time::sleep(step).await;
        }
        socket
            .write_all(b"0\r\n\r\n")
            .await
            .expect("write terminator");
        socket.flush().await.expect("flush terminator");
    });

    let client = Client::builder()
        .api_key("sk-test")
        .base_url(format!("http://{addr}"))
        .timeout(client_timeout)
        .max_retries(0)
        .build()
        .expect("client builds");

    let request = MessagesRequest::builder()
        .model("claude-opus-4-8")
        .max_tokens(16)
        .messages(vec![MessageParam::user("Hi")])
        .build()
        .expect("request builds");

    let message = client
        .messages()
        .stream(&request)
        .await
        .expect("stream opens")
        .collect_final()
        .await
        .expect("stream must complete despite total time exceeding the timeout");

    assert_eq!(message.text(), "Hello world");
    server.await.expect("server task");
}
