//! Wiremock integration tests for the Message Batches endpoint: retrieval,
//! listing with pagination, cancellation, and JSONL results decoding (including
//! forward-compatible tolerance of unknown result variants).

use crimson_crab::api::{BatchListParams, BatchResultOutcome, BatchStatus};
use crimson_crab::Client;
use futures_util::StreamExt;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .api_key("sk-test")
        .base_url(server.uri())
        .build()
        .expect("client builds")
}

fn batch_json(status: &str) -> serde_json::Value {
    json!({
        "id": "msgbatch_1",
        "type": "message_batch",
        "processing_status": status,
        "request_counts": {"processing": 0, "succeeded": 2, "errored": 0, "canceled": 0, "expired": 0},
        "results_url": "https://example.com/results"
    })
}

fn succeeded_message() -> serde_json::Value {
    json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-opus-4-8",
        "content": [{"type": "text", "text": "ok"}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {"input_tokens": 5, "output_tokens": 2}
    })
}

#[tokio::test]
async fn batches_get_parses_ended_batch() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches/msgbatch_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(batch_json("ended")))
        .expect(1)
        .mount(&server)
        .await;

    let batch = client_for(&server)
        .batches()
        .get("msgbatch_1")
        .await
        .expect("get succeeds");
    assert_eq!(batch.id, "msgbatch_1");
    assert_eq!(batch.processing_status, BatchStatus::Ended);
    assert_eq!(batch.request_counts.succeeded, 2);
    assert_eq!(
        batch.results_url.as_deref(),
        Some("https://example.com/results")
    );
}

#[tokio::test]
async fn batches_list_sends_pagination_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches"))
        .and(query_param("limit", "1"))
        .and(query_param("after_id", "msgbatch_0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [batch_json("in_progress")],
            "has_more": false,
            "first_id": "msgbatch_1",
            "last_id": "msgbatch_1"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let params = BatchListParams {
        after_id: Some("msgbatch_0".to_string()),
        limit: Some(1),
        ..Default::default()
    };
    let page = client_for(&server)
        .batches()
        .list(&params)
        .await
        .expect("list succeeds");
    assert_eq!(page.data.len(), 1);
    assert!(!page.has_more);
    assert_eq!(page.data[0].processing_status, BatchStatus::InProgress);
}

#[tokio::test]
async fn batches_cancel_returns_canceling_batch() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches/msgbatch_1/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(batch_json("canceling")))
        .expect(1)
        .mount(&server)
        .await;

    let batch = client_for(&server)
        .batches()
        .cancel("msgbatch_1")
        .await
        .expect("cancel succeeds");
    assert_eq!(batch.processing_status, BatchStatus::Canceling);
}

#[tokio::test]
async fn batches_results_decodes_mixed_and_unknown_outcomes() {
    let server = MockServer::start().await;
    // Blank lines between records exercise the line skipping; an unknown result
    // `type` must deserialize to `Unknown`, not error.
    let body = format!(
        "{}\n\n{}\n{}\n",
        json!({"custom_id": "r1", "result": {"type": "succeeded", "message": succeeded_message()}}),
        json!({"custom_id": "r2", "result": {"type": "errored", "error": {"type": "error", "error": {"type": "invalid_request_error", "message": "bad"}}}}),
        json!({"custom_id": "r3", "result": {"type": "future_state", "detail": 1}}),
    );

    Mock::given(method("GET"))
        .and(path("/v1/messages/batches/msgbatch_1/results"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let mut stream = client
        .batches()
        .results("msgbatch_1")
        .await
        .expect("results stream opens");

    let mut collected = Vec::new();
    while let Some(item) = stream.next().await {
        collected.push(item.expect("valid batch result"));
    }

    assert_eq!(collected.len(), 3);
    assert_eq!(collected[0].custom_id, "r1");
    assert!(matches!(
        collected[0].result,
        BatchResultOutcome::Succeeded(_)
    ));
    assert!(matches!(
        collected[1].result,
        BatchResultOutcome::Errored(_)
    ));
    // A result variant the SDK does not model round-trips as Unknown.
    assert!(matches!(
        collected[2].result,
        BatchResultOutcome::Unknown(_)
    ));
}
