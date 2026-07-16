//! Streaming tests: fixture SSE byte streams (the documented example sequence, a
//! tool-use stream with split `partial_json` fragments, a thinking stream with a
//! `signature_delta`, an in-stream `error` event, and chunk boundaries that
//! split records mid-line) plus a wiremock SSE endpoint test. Each fixture
//! asserts both the decoded event sequence and that `collect_final()` equals the
//! equivalent non-streaming `Message`.

use bytes::Bytes;
use crimson_crab::api::MessagesRequest;
use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::streaming::{ContentDelta, MessageStream, StreamEvent};
use crimson_crab::types::{
    ContentBlock, Message, MessageParam, Role, StopReason, TextBlock, ThinkingBlock, ToolUseBlock,
    Usage,
};
use crimson_crab::Client;
use futures_util::{stream, StreamExt};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Builds a `MessageStream` that reads the SSE `body`, delivered in a single
/// chunk.
fn stream_from(body: &str) -> MessageStream {
    let chunk: Vec<crimson_crab::Result<Bytes>> = vec![Ok(Bytes::copy_from_slice(body.as_bytes()))];
    MessageStream::from_byte_stream(stream::iter(chunk))
}

/// Builds a `MessageStream` that reads `body` split into fixed-size `chunk_size`
/// byte pieces, to prove the decoder buffers across chunks (boundaries land
/// mid-line, mid-JSON, and inside record separators).
fn stream_from_chunks(body: &str, chunk_size: usize) -> MessageStream {
    let chunks: Vec<crimson_crab::Result<Bytes>> = body
        .as_bytes()
        .chunks(chunk_size)
        .map(|piece| Ok(Bytes::copy_from_slice(piece)))
        .collect();
    MessageStream::from_byte_stream(stream::iter(chunks))
}

/// Collects every event from a stream, panicking on the first `Err`.
async fn collect_events(mut s: MessageStream) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    while let Some(item) = s.next().await {
        events.push(item.expect("event should be Ok"));
    }
    events
}

// The documented simple text-response sequence (docs/wire-api.md "Streaming").
const TEXT_SSE: &str = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":25,\"output_tokens\":1}}}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}

event: ping
data: {\"type\":\"ping\"}

event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":12}}

event: message_stop
data: {\"type\":\"message_stop\"}

";

/// The non-streaming `Message` the text SSE fixture must accumulate to.
fn expected_text_message() -> Message {
    Message {
        id: "msg_1".into(),
        message_type: "message".into(),
        role: Role::Assistant,
        model: "claude-opus-4-8".into(),
        content: vec![ContentBlock::Text(TextBlock::new("Hello world"))],
        stop_reason: Some(StopReason::EndTurn),
        stop_sequence: None,
        stop_details: None,
        usage: Usage {
            input_tokens: 25,
            output_tokens: 12,
            ..Usage::default()
        },
        container: None,
    }
}

#[tokio::test]
async fn text_stream_event_sequence_and_final_message() {
    let events = collect_events(stream_from(TEXT_SSE)).await;

    // Ping is tolerated and surfaced; the sequence matches the fixture order.
    assert!(matches!(events[0], StreamEvent::MessageStart { .. }));
    assert!(matches!(
        events[1],
        StreamEvent::ContentBlockStart { index: 0, .. }
    ));
    assert!(matches!(
        events[2],
        StreamEvent::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::TextDelta { .. }
        }
    ));
    assert!(matches!(
        events[3],
        StreamEvent::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::TextDelta { .. }
        }
    ));
    assert!(matches!(
        events[4],
        StreamEvent::ContentBlockStop { index: 0 }
    ));
    assert!(matches!(events[5], StreamEvent::Ping));
    assert!(matches!(events[6], StreamEvent::MessageDelta { .. }));
    assert!(matches!(events[7], StreamEvent::MessageStop));
    assert_eq!(events.len(), 8);

    // collect_final equals the equivalent non-streaming Message.
    let final_message = stream_from(TEXT_SSE)
        .collect_final()
        .await
        .expect("collect_final");
    assert_eq!(final_message, expected_text_message());
    assert_eq!(final_message.text(), "Hello world");
}

#[tokio::test]
async fn text_stream_survives_chunk_boundaries_splitting_mid_line() {
    // 7-byte chunks force boundaries inside `data:` JSON payloads and inside the
    // blank-line record separators, proving the decoder buffers partial lines.
    for chunk_size in [1, 3, 7, 13] {
        let final_message = stream_from_chunks(TEXT_SSE, chunk_size)
            .collect_final()
            .await
            .expect("collect_final across chunk boundaries");
        assert_eq!(
            final_message,
            expected_text_message(),
            "mismatch at chunk_size {chunk_size}"
        );
    }
}

// A tool_use stream whose input arrives as split `partial_json` fragments.
const TOOL_SSE: &str = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_tool\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":40,\"output_tokens\":1}}}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"get_weather\",\"input\":{}}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"loc\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"ation\\\":\\\"San\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\" Francisco\\\"}\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}

event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":20}}

event: message_stop
data: {\"type\":\"message_stop\"}

";

#[tokio::test]
async fn tool_use_stream_assembles_partial_json_into_input() {
    let final_message = stream_from(TOOL_SSE)
        .collect_final()
        .await
        .expect("collect_final");

    let expected = Message {
        id: "msg_tool".into(),
        message_type: "message".into(),
        role: Role::Assistant,
        model: "claude-opus-4-8".into(),
        content: vec![ContentBlock::ToolUse(ToolUseBlock {
            id: "toolu_1".into(),
            name: "get_weather".into(),
            input: json!({"location": "San Francisco"}),
        })],
        stop_reason: Some(StopReason::ToolUse),
        stop_sequence: None,
        stop_details: None,
        usage: Usage {
            input_tokens: 40,
            output_tokens: 20,
            ..Usage::default()
        },
        container: None,
    };
    assert_eq!(final_message, expected);
}

// A thinking stream: thinking_delta fragments then a signature_delta that
// arrives before the block's content_block_stop.
const THINKING_SSE: &str = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_think\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":15,\"output_tokens\":1}}}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Let me \"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"reason.\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sigABC\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}

event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":9}}

event: message_stop
data: {\"type\":\"message_stop\"}

";

#[tokio::test]
async fn thinking_stream_accumulates_text_and_signature() {
    let events = collect_events(stream_from(THINKING_SSE)).await;
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::ContentBlockDelta {
            delta: ContentDelta::SignatureDelta { .. },
            ..
        }
    )));

    let final_message = stream_from(THINKING_SSE)
        .collect_final()
        .await
        .expect("collect_final");
    assert_eq!(
        final_message.content,
        vec![ContentBlock::Thinking(ThinkingBlock {
            thinking: "Let me reason.".into(),
            signature: "sigABC".into(),
        })]
    );
    assert_eq!(final_message.stop_reason, Some(StopReason::EndTurn));
    assert_eq!(final_message.usage.output_tokens, 9);
}

// An in-stream error event mid-message.
const ERROR_SSE: &str = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_err\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":1}}}

event: error
data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}

";

#[tokio::test]
async fn in_stream_error_is_surfaced_as_err_and_terminates() {
    let mut s = stream_from(ERROR_SSE);
    let first = s.next().await.expect("first event");
    assert!(matches!(
        first.expect("message_start ok"),
        StreamEvent::MessageStart { .. }
    ));

    let second = s.next().await.expect("second item");
    match second {
        Err(crimson_crab::Error::Overloaded(api)) => {
            assert_eq!(api.error_type, "overloaded_error");
            assert_eq!(api.message, "Overloaded");
        }
        other => panic!("expected Overloaded error, got {other:?}"),
    }

    // The stream is terminated after an in-stream error.
    assert!(s.next().await.is_none());
}

#[tokio::test]
async fn collect_final_propagates_in_stream_error() {
    let result = stream_from(ERROR_SSE).collect_final().await;
    assert!(matches!(result, Err(crimson_crab::Error::Overloaded(_))));
}

fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .api_key("sk-test")
        .base_url(server.uri())
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

#[tokio::test]
async fn wiremock_sse_endpoint_streams_and_sets_stream_flag() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_partial_json(json!({
            "model": "claude-opus-4-8",
            "stream": true
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(TEXT_SSE),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let stream = client
        .messages()
        .stream(&simple_request())
        .await
        .expect("stream opens");
    let final_message = stream.collect_final().await.expect("collect_final");
    assert_eq!(final_message, expected_text_message());
}
