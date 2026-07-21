//! Wiremock integration tests for the Messages, Models, and Batches endpoints:
//! header/body construction, error mapping, retry behavior, token counting,
//! model retrieval/listing, and batch results streaming.

use std::time::{Duration, Instant};

use crimson_crab::api::{
    BatchRequestItem, BatchResultOutcome, BatchStatus, MessagesRequest, ModelListParams,
};
use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::types::{MessageParam, StopReason};
use crimson_crab::{Client, Error};
use futures_util::StreamExt;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .api_key("sk-test")
        .base_url(server.uri())
        .max_retries(2)
        .build()
        .expect("client builds")
}

fn simple_request() -> MessagesRequest {
    MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(1024)
        .messages(vec![MessageParam::user("Hello")])
        .build()
        .expect("request builds")
}

fn simple_message() -> serde_json::Value {
    json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-opus-4-8",
        "content": [{"type": "text", "text": "Hi!"}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {"input_tokens": 5, "output_tokens": 2}
    })
}

#[tokio::test]
async fn create_sends_headers_and_body_and_parses_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-test"))
        .and(header("anthropic-version", "2023-06-01"))
        .and(header("content-type", "application/json"))
        .and(header("anthropic-beta", "fast-mode-2026-02-01"))
        .and(body_partial_json(json!({
            "model": "claude-opus-4-8",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": "Hello"}],
            "speed": "fast"
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_out")
                .set_body_json(simple_message()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(1024)
        .messages(vec![MessageParam::user("Hello")])
        .beta("fast-mode-2026-02-01")
        .extra_field("speed", json!("fast"))
        .build()
        .expect("request builds");

    let message = client_for(&server)
        .messages()
        .create(&request)
        .await
        .expect("create succeeds");
    assert_eq!(message.id, "msg_1");
    assert_eq!(message.stop_reason, Some(StopReason::EndTurn));
    assert_eq!(message.text(), "Hi!");
}

#[tokio::test]
async fn create_maps_400_to_bad_request_with_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(400)
                .insert_header("request-id", "req_hdr")
                .set_body_json(json!({
                    "type": "error",
                    "error": {"type": "invalid_request_error", "message": "bad thing"},
                    "request_id": "req_body"
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    match client_for(&server)
        .messages()
        .create(&simple_request())
        .await
    {
        Err(Error::BadRequest(api)) => {
            assert_eq!(api.error_type, "invalid_request_error");
            assert_eq!(api.message, "bad thing");
            assert_eq!(api.request_id.as_deref(), Some("req_hdr"));
        }
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[tokio::test]
async fn create_maps_401_to_authentication() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "type": "error",
            "error": {"type": "authentication_error", "message": "invalid api key"}
        })))
        .expect(1)
        .mount(&server)
        .await;

    match client_for(&server)
        .messages()
        .create(&simple_request())
        .await
    {
        Err(Error::Authentication(api)) => assert_eq!(api.error_type, "authentication_error"),
        other => panic!("expected Authentication, got {other:?}"),
    }
}

#[tokio::test]
async fn create_retries_429_and_honors_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "1")
                .set_body_json(json!({
                    "type": "error",
                    "error": {"type": "rate_limit_error", "message": "slow down"}
                })),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(simple_message()))
        .with_priority(5)
        .expect(1)
        .mount(&server)
        .await;

    let start = Instant::now();
    let message = client_for(&server)
        .messages()
        .create(&simple_request())
        .await
        .expect("create eventually succeeds");
    let elapsed = start.elapsed();

    assert_eq!(message.id, "msg_1");
    assert!(
        elapsed >= Duration::from_millis(900),
        "expected retry-after ~1s to be honored, elapsed {elapsed:?}"
    );
}

#[tokio::test]
async fn create_retries_500_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "type": "error",
            "error": {"type": "api_error", "message": "boom"}
        })))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(simple_message()))
        .with_priority(5)
        .expect(1)
        .mount(&server)
        .await;

    let message = client_for(&server)
        .messages()
        .create(&simple_request())
        .await
        .expect("create eventually succeeds");
    assert_eq!(message.id, "msg_1");
}

#[tokio::test]
async fn create_rejects_stream_true_request() {
    // A `stream(true)` request routed to `create()` is rejected with a clear
    // config error rather than failing later with a confusing SSE parse error;
    // no HTTP request is sent, so no mock server is needed.
    let client = Client::builder()
        .api_key("sk-test")
        .build()
        .expect("client builds");
    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(16)
        .messages(vec![MessageParam::user("Hi")])
        .stream(true)
        .build()
        .expect("request builds");
    match client.messages().create(&request).await {
        Err(Error::Config(message)) => assert!(message.contains("stream")),
        other => panic!("expected Config error for stream=true, got {other:?}"),
    }
}

#[tokio::test]
async fn count_tokens_parses_input_tokens() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/count_tokens"))
        .and(body_partial_json(json!({
            "model": "claude-opus-4-8",
            "messages": [{"role": "user", "content": "Hello"}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"input_tokens": 2095})))
        .expect(1)
        .mount(&server)
        .await;

    let count_request = simple_request().as_count_request();
    let response = client_for(&server)
        .messages()
        .count_tokens(&count_request)
        .await
        .expect("count_tokens succeeds");
    assert_eq!(response.input_tokens, 2095);
}

#[tokio::test]
async fn models_get_parses_model_info() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/claude-opus-4-8"))
        .and(header("x-api-key", "sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "claude-opus-4-8",
            "display_name": "Claude Opus 4.8",
            "type": "model",
            "max_input_tokens": 1000000,
            "max_tokens": 128000,
            "capabilities": {"vision": {"supported": true}}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let model = client_for(&server)
        .models()
        .get("claude-opus-4-8")
        .await
        .expect("models get succeeds");
    assert_eq!(model.id, "claude-opus-4-8");
    assert_eq!(model.max_input_tokens, Some(1_000_000));
    assert_eq!(model.capabilities["vision"]["supported"], json!(true));
}

#[tokio::test]
async fn models_list_parses_page_and_sends_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(query_param("limit", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {"id": "claude-opus-4-8", "type": "model"},
                {"id": "claude-sonnet-5", "type": "model"}
            ],
            "has_more": true,
            "first_id": "claude-opus-4-8",
            "last_id": "claude-sonnet-5"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let params = ModelListParams {
        limit: Some(2),
        ..Default::default()
    };
    let page = client_for(&server)
        .models()
        .list(&params)
        .await
        .expect("models list succeeds");
    assert_eq!(page.data.len(), 2);
    assert!(page.has_more);
    assert_eq!(page.last_id.as_deref(), Some("claude-sonnet-5"));
}

#[tokio::test]
async fn batches_create_parses_batch() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .and(body_partial_json(json!({
            "requests": [{"custom_id": "r1", "params": {"model": "claude-opus-4-8"}}]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msgbatch_1",
            "type": "message_batch",
            "processing_status": "in_progress",
            "request_counts": {"processing": 1, "succeeded": 0, "errored": 0, "canceled": 0, "expired": 0}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let item = BatchRequestItem::from_request("r1", &simple_request()).expect("item builds");
    let batch = client_for(&server)
        .batches()
        .create(&[item])
        .await
        .expect("batch create succeeds");
    assert_eq!(batch.id, "msgbatch_1");
    assert_eq!(batch.processing_status, BatchStatus::InProgress);
    assert_eq!(batch.request_counts.processing, 1);
}

#[tokio::test]
async fn batches_results_decodes_jsonl_stream() {
    let server = MockServer::start().await;
    let line_one =
        json!({"custom_id": "r1", "result": {"type": "succeeded", "message": simple_message()}});
    let line_two = json!({"custom_id": "r2", "result": {"type": "canceled"}});
    let body = format!("{line_one}\n{line_two}\n");

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

    assert_eq!(collected.len(), 2);
    assert_eq!(collected[0].custom_id, "r1");
    assert!(matches!(
        collected[0].result,
        BatchResultOutcome::Succeeded(_)
    ));
    assert_eq!(collected[1].custom_id, "r2");
    assert!(matches!(
        collected[1].result,
        BatchResultOutcome::Canceled(_)
    ));
}
