//! Probes for the streaming HTTP path: retry-before-first-byte and non-2xx
//! error mapping (the streaming path goes through `execute`, unlike the JSON
//! path that the existing integration tests cover).

use crimson_crab::api::MessagesRequest;
use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::types::MessageParam;
use crimson_crab::{Client, Error};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEXT_SSE: &str = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\n";

fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .api_key("sk-test")
        .base_url(server.uri())
        .max_retries(2)
        .build()
        .expect("client")
}

fn simple_request() -> MessagesRequest {
    MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(16)
        .messages(vec![MessageParam::user("Hi")])
        .build()
        .expect("request")
}

#[tokio::test]
async fn probe_streaming_retries_529_before_first_byte() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(529).set_body_json(json!({
            "type": "error",
            "error": {"type": "overloaded_error", "message": "overloaded"}
        })))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(TEXT_SSE),
        )
        .with_priority(5)
        .expect(1)
        .mount(&server)
        .await;

    let msg = client_for(&server)
        .messages()
        .stream(&simple_request())
        .await
        .expect("stream opens after retry")
        .collect_final()
        .await
        .expect("collect_final");
    assert_eq!(msg.text(), "Hi");
}

#[tokio::test]
async fn probe_streaming_maps_400_to_bad_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "type": "error",
            "error": {"type": "invalid_request_error", "message": "nope"}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let result = client_for(&server)
        .messages()
        .stream(&simple_request())
        .await;
    match result {
        Err(Error::BadRequest(api)) => assert_eq!(api.error_type, "invalid_request_error"),
        Err(other) => panic!("expected BadRequest from streaming path, got {other:?}"),
        Ok(_) => panic!("expected BadRequest from streaming path, got a stream"),
    }
}
