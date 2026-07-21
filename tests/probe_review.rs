//! Adversarial probe tests for the SSE parser, JSONL decoder, and accumulator.
//! These exercise chunk-boundary edge cases the existing tests don't cover.

use bytes::Bytes;
use crimson_crab::streaming::{MessageStream, StreamEvent};
use crimson_crab::types::ContentBlock;
use futures_util::{stream, StreamExt};

fn stream_from_chunks(bytes: &[u8], chunk_size: usize) -> MessageStream {
    let chunks: Vec<crimson_crab::Result<Bytes>> = bytes
        .chunks(chunk_size)
        .map(|piece| Ok(Bytes::copy_from_slice(piece)))
        .collect();
    MessageStream::from_byte_stream(stream::iter(chunks))
}

// Probe 1: a 4-byte emoji and multibyte text inside SSE `data:` JSON, split at
// EVERY byte boundary. If the parser sliced a line mid-UTF-8 and used
// from_utf8_lossy, the text would get replacement chars.
#[tokio::test]
async fn probe_utf8_multibyte_split_across_chunks() {
    // "Héllo 🌍 世界" contains 2-, 3-, and 4-byte sequences.
    let text = "Héllo 🌍 世界";
    let sse = format!(
        "event: message_start\n\
         data: {{\"type\":\"message_start\",\"message\":{{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}}}\n\n\
         event: content_block_start\n\
         data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n\
         event: content_block_delta\n\
         data: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{text}\"}}}}\n\n\
         event: message_stop\n\
         data: {{\"type\":\"message_stop\"}}\n\n"
    );
    let bytes = sse.as_bytes();
    for chunk_size in 1..=bytes.len() {
        let msg = stream_from_chunks(bytes, chunk_size)
            .collect_final()
            .await
            .unwrap_or_else(|e| panic!("collect_final failed at chunk {chunk_size}: {e:?}"));
        assert_eq!(
            msg.text(),
            text,
            "text corrupted at chunk_size {chunk_size}"
        );
    }
}

// Probe 2: multibyte UTF-8 inside a tool_use input_json_delta, split across
// chunks. The concatenated JSON must still parse into the input object.
#[tokio::test]
async fn probe_utf8_in_partial_json_split() {
    let sse = "event: message_start\n\
         data: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n\
         event: content_block_start\n\
         data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"f\",\"input\":{}}}\n\n\
         event: content_block_delta\n\
         data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\":\\\"S\"}}\n\n\
         event: content_block_delta\n\
         data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"ão Paulo 世界\\\"}\"}}\n\n\
         event: content_block_stop\n\
         data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
         event: message_stop\n\
         data: {\"type\":\"message_stop\"}\n\n";
    let bytes = sse.as_bytes();
    for chunk_size in [1usize, 2, 3, 5, 8, 13] {
        let msg = stream_from_chunks(bytes, chunk_size)
            .collect_final()
            .await
            .unwrap_or_else(|e| panic!("failed at chunk {chunk_size}: {e:?}"));
        match &msg.content[0] {
            ContentBlock::ToolUse(inner) => {
                assert_eq!(
                    inner.input,
                    serde_json::json!({"city": "São Paulo 世界"}),
                    "input wrong at chunk_size {chunk_size}"
                );
            }
            other => panic!("expected tool_use, got {other:?}"),
        }
    }
}

// Probe 3: CRLF record separators split so that \r and \n straddle chunks.
#[tokio::test]
async fn probe_crlf_split_across_chunks() {
    let sse = "event: message_start\r\n\
         data: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\r\n\r\n\
         event: content_block_start\r\n\
         data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\r\n\r\n\
         event: content_block_delta\r\n\
         data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\r\n\r\n\
         event: message_stop\r\n\
         data: {\"type\":\"message_stop\"}\r\n\r\n";
    let bytes = sse.as_bytes();
    for chunk_size in 1..=6 {
        let msg = stream_from_chunks(bytes, chunk_size)
            .collect_final()
            .await
            .unwrap_or_else(|e| panic!("failed at chunk {chunk_size}: {e:?}"));
        assert_eq!(msg.text(), "Hi", "at chunk_size {chunk_size}");
    }
}

// Probe 4: in-stream error, then MORE bytes after it. The stream must terminate
// at the error and never yield the trailing events.
#[tokio::test]
async fn probe_error_then_trailing_events_terminates() {
    let sse = "event: message_start\n\
         data: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n\
         event: error\n\
         data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n\
         event: content_block_start\n\
         data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"should not appear\"}}\n\n\
         event: message_stop\n\
         data: {\"type\":\"message_stop\"}\n\n";
    let mut s = stream_from_chunks(sse.as_bytes(), 4);
    let first = s.next().await.expect("first").expect("ok");
    assert!(matches!(first, StreamEvent::MessageStart { .. }));
    let second = s.next().await.expect("second");
    assert!(matches!(second, Err(crimson_crab::Error::Overloaded(_))));
    assert!(s.next().await.is_none(), "must terminate after error");
}

// Probe 5: content_block_start at a NON-zero index with earlier indices absent.
// set_block fills the gap with placeholder Unknown blocks.
#[tokio::test]
async fn probe_index_gap_fills_placeholders() {
    let sse = "event: message_start\n\
         data: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n\
         event: content_block_start\n\
         data: {\"type\":\"content_block_start\",\"index\":3,\"content_block\":{\"type\":\"text\",\"text\":\"late\"}}\n\n\
         event: message_stop\n\
         data: {\"type\":\"message_stop\"}\n\n";
    let msg = stream_from_chunks(sse.as_bytes(), 64)
        .collect_final()
        .await
        .expect("collect_final");
    assert_eq!(msg.content.len(), 4, "gap should be filled to index 3");
    assert_eq!(msg.text(), "late");
}

// Probe 6: an absurd content_block_start `index` (near usize::MAX) must NOT drive
// the accumulator to allocate placeholder blocks up to that index (OOM/abort).
#[tokio::test]
async fn probe_absurd_block_index_does_not_allocate() {
    let sse = format!(
        "event: message_start\n\
         data: {{\"type\":\"message_start\",\"message\":{{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}}}\n\n\
         event: content_block_start\n\
         data: {{\"type\":\"content_block_start\",\"index\":{index},\"content_block\":{{\"type\":\"text\",\"text\":\"late\"}}}}\n\n\
         event: message_stop\n\
         data: {{\"type\":\"message_stop\"}}\n\n",
        index = usize::MAX
    );
    let msg = stream_from_chunks(sse.as_bytes(), 64)
        .collect_final()
        .await
        .expect("collect_final");
    // The block at an absurd index is dropped rather than gap-filled.
    assert!(
        msg.content.len() < 1024,
        "content must not be gap-filled for an absurd index, got {}",
        msg.content.len()
    );
}

// Probe 7: tool_use `input_json` fragments that never form valid JSON must be
// preserved as a raw string, not silently coerced to JSON null (data loss).
#[tokio::test]
async fn probe_malformed_tool_input_preserved_as_raw_string() {
    let sse = "event: message_start\n\
         data: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-8\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n\
         event: content_block_start\n\
         data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"f\",\"input\":{}}}\n\n\
         event: content_block_delta\n\
         data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{not valid\"}}\n\n\
         event: content_block_stop\n\
         data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
         event: message_stop\n\
         data: {\"type\":\"message_stop\"}\n\n";
    let msg = stream_from_chunks(sse.as_bytes(), 64)
        .collect_final()
        .await
        .expect("collect_final");
    match &msg.content[0] {
        ContentBlock::ToolUse(inner) => {
            assert_eq!(
                inner.input,
                serde_json::Value::String("{not valid".to_string()),
                "malformed tool input must be preserved, not nulled"
            );
        }
        other => panic!("expected tool_use, got {other:?}"),
    }
}
