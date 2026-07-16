//! Server-sent-events (SSE) streaming for `POST /v1/messages` with
//! `"stream": true`.
//!
//! [`MessageStream`] wraps the raw byte stream returned by the API, decodes the
//! SSE records with a hand-rolled parser (no external eventsource dependency),
//! and yields typed [`StreamEvent`]s. As events flow past it, the stream also
//! *accumulates* them into a [`Message`] identical in shape to the one the
//! non-streaming endpoint would have returned — retrievable with
//! [`MessageStream::final_message`] while draining, or in one shot with
//! [`MessageStream::collect_final`].
//!
//! Forward compatibility is preserved throughout: unknown event types and
//! unknown delta types deserialize into `Unknown` catch-alls (retaining the raw
//! JSON) rather than erroring, and an in-stream `error` event is surfaced to the
//! caller as an `Err` item that terminates the stream.
//!
//! # Examples
//!
//! ```no_run
//! use crimson_crab::api::MessagesRequest;
//! use crimson_crab::streaming::{ContentDelta, StreamEvent};
//! use crimson_crab::types::MessageParam;
//! use futures_util::StreamExt;
//!
//! # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
//! let request = MessagesRequest::builder()
//!     .model("claude-opus-4-8")
//!     .max_tokens(1024)
//!     .messages(vec![MessageParam::user("Hello")])
//!     .build()?;
//!
//! let mut stream = client.messages().stream(&request).await?;
//! while let Some(event) = stream.next().await {
//!     if let StreamEvent::ContentBlockDelta {
//!         delta: ContentDelta::TextDelta { text },
//!         ..
//!     } = event?
//!     {
//!         print!("{text}");
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::de::{self, Deserializer};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use crate::error::{self, Error, Result};
use crate::types::{ContentBlock, Message, StopDetails, StopReason, Usage};

// ---------------------------------------------------------------------------
// Stream event types.
// ---------------------------------------------------------------------------

/// A single decoded SSE event from a streaming `POST /v1/messages` response.
///
/// The variants mirror the Anthropic stream event sequence
/// (`message_start` → `content_block_start`/`content_block_delta`/
/// `content_block_stop` (repeated) → `message_delta` → `message_stop`), plus
/// `ping` heartbeats and an in-stream `error`. Any event type the SDK does not
/// model deserializes to [`StreamEvent::Unknown`], preserving the raw JSON.
///
/// [`MessageStream`] intercepts [`StreamEvent::Error`] and surfaces it as an
/// `Err` item instead of yielding it, so consumers of the stream never observe
/// this variant directly; it exists so the event model is complete and
/// round-trips.
///
/// # Examples
///
/// ```
/// use crimson_crab::streaming::StreamEvent;
///
/// let event: StreamEvent = serde_json::from_str(
///     r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#,
/// )
/// .unwrap();
/// assert!(matches!(event, StreamEvent::ContentBlockDelta { index: 0, .. }));
///
/// // An unknown event type is preserved rather than rejected.
/// let novel: StreamEvent = serde_json::from_str(r#"{"type":"brand_new","x":1}"#).unwrap();
/// assert!(matches!(novel, StreamEvent::Unknown(_)));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StreamEvent {
    /// The opening event; carries the base [`Message`] (empty `content`).
    MessageStart {
        /// The base message that subsequent events are accumulated into.
        message: Message,
    },
    /// A content block begins at `index`.
    ContentBlockStart {
        /// The position of the block within the message `content` array.
        index: usize,
        /// The initial (usually empty) block.
        content_block: ContentBlock,
    },
    /// An incremental update to the content block at `index`.
    ContentBlockDelta {
        /// The position of the block being updated.
        index: usize,
        /// The delta to apply.
        delta: ContentDelta,
    },
    /// The content block at `index` is complete.
    ContentBlockStop {
        /// The position of the block that finished.
        index: usize,
    },
    /// Top-level message updates (final `stop_reason`, cumulative `usage`).
    MessageDelta {
        /// The stop metadata for the message.
        delta: MessageDeltaBody,
        /// Cumulative usage totals, if present.
        usage: Option<UsageDelta>,
    },
    /// The final event; the message is complete.
    MessageStop,
    /// A keep-alive heartbeat with no payload.
    Ping,
    /// An in-stream error; terminates the message.
    Error {
        /// The error body reported by the server.
        error: StreamErrorBody,
    },
    /// Forward-compatible catch-all preserving the raw JSON of an unknown event.
    Unknown(serde_json::Value),
}

/// An incremental update to a content block, carried by
/// [`StreamEvent::ContentBlockDelta`].
///
/// # Examples
///
/// ```
/// use crimson_crab::streaming::ContentDelta;
///
/// let delta: ContentDelta =
///     serde_json::from_str(r#"{"type":"text_delta","text":"Hello"}"#).unwrap();
/// assert_eq!(delta, ContentDelta::TextDelta { text: "Hello".to_string() });
///
/// let unknown: ContentDelta =
///     serde_json::from_str(r#"{"type":"future_delta","x":1}"#).unwrap();
/// assert!(matches!(unknown, ContentDelta::Unknown(_)));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ContentDelta {
    /// Text to append to a `text` block.
    TextDelta {
        /// The text fragment.
        text: String,
    },
    /// A fragment of a `tool_use` block's JSON `input`; concatenate all
    /// fragments and parse the result (an empty string means `{}`).
    InputJsonDelta {
        /// The partial JSON fragment.
        partial_json: String,
    },
    /// Reasoning text to append to a `thinking` block.
    ThinkingDelta {
        /// The thinking fragment.
        thinking: String,
    },
    /// The signature for a `thinking` block (arrives before its stop event).
    SignatureDelta {
        /// The opaque signature fragment.
        signature: String,
    },
    /// A citation to append to a `text` block.
    CitationsDelta {
        /// The raw citation object.
        citation: serde_json::Value,
    },
    /// Forward-compatible catch-all preserving the raw JSON of an unknown delta.
    Unknown(serde_json::Value),
}

/// The `delta` object carried by a [`StreamEvent::MessageDelta`] event.
///
/// It conveys the final `stop_reason`/`stop_sequence` (and, on a refusal, the
/// `stop_details`) once the model has finished; these are merged into the
/// accumulated [`Message`].
///
/// # Examples
///
/// ```
/// use crimson_crab::streaming::MessageDeltaBody;
/// use crimson_crab::types::StopReason;
///
/// let body: MessageDeltaBody = serde_json::from_value(serde_json::json!({
///     "stop_reason": "end_turn",
///     "stop_sequence": null
/// }))
/// .unwrap();
/// assert_eq!(body.stop_reason, Some(StopReason::EndTurn));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageDeltaBody {
    /// Why generation stopped, if known at this point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    /// The matched stop sequence, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    /// Refusal detail, present only on a refusal stop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_details: Option<StopDetails>,
    /// Forward-compatible catch-all for any other fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// The `usage` object carried by a [`StreamEvent::MessageDelta`] event.
///
/// Each present field is a **cumulative total** that *replaces* (not adds to)
/// the corresponding field on the accumulated message's [`Usage`].
///
/// # Examples
///
/// ```
/// use crimson_crab::streaming::UsageDelta;
///
/// let usage: UsageDelta =
///     serde_json::from_value(serde_json::json!({"output_tokens": 12})).unwrap();
/// assert_eq!(usage.output_tokens, Some(12));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageDelta {
    /// Cumulative input tokens, if updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Cumulative output tokens, if updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Cumulative cache-creation input tokens, if updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    /// Cumulative cache-read input tokens, if updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
    /// Forward-compatible catch-all for any other usage fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// The `error` object carried by an in-stream [`StreamEvent::Error`] event.
///
/// # Examples
///
/// ```
/// use crimson_crab::streaming::StreamErrorBody;
///
/// let body: StreamErrorBody = serde_json::from_value(serde_json::json!({
///     "type": "overloaded_error",
///     "message": "Overloaded"
/// }))
/// .unwrap();
/// assert_eq!(body.error_type, "overloaded_error");
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StreamErrorBody {
    /// The machine-readable error type, e.g. `"overloaded_error"`.
    #[serde(rename = "type")]
    pub error_type: String,
    /// A human-readable description of the error.
    pub message: String,
    /// Forward-compatible catch-all for any other fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Serde for the two hand-written enums.
// ---------------------------------------------------------------------------

impl Serialize for ContentDelta {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ContentDelta::TextDelta { text } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "text_delta")?;
                map.serialize_entry("text", text)?;
                map.end()
            }
            ContentDelta::InputJsonDelta { partial_json } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "input_json_delta")?;
                map.serialize_entry("partial_json", partial_json)?;
                map.end()
            }
            ContentDelta::ThinkingDelta { thinking } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "thinking_delta")?;
                map.serialize_entry("thinking", thinking)?;
                map.end()
            }
            ContentDelta::SignatureDelta { signature } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "signature_delta")?;
                map.serialize_entry("signature", signature)?;
                map.end()
            }
            ContentDelta::CitationsDelta { citation } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "citations_delta")?;
                map.serialize_entry("citation", citation)?;
                map.end()
            }
            ContentDelta::Unknown(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ContentDelta {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let tag = value
            .get("type")
            .and_then(|t| t.as_str())
            .map(str::to_owned);
        let delta = match tag.as_deref() {
            Some("text_delta") => {
                #[derive(Deserialize)]
                struct R {
                    #[serde(default)]
                    text: String,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                ContentDelta::TextDelta { text: r.text }
            }
            Some("input_json_delta") => {
                #[derive(Deserialize)]
                struct R {
                    #[serde(default)]
                    partial_json: String,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                ContentDelta::InputJsonDelta {
                    partial_json: r.partial_json,
                }
            }
            Some("thinking_delta") => {
                #[derive(Deserialize)]
                struct R {
                    #[serde(default)]
                    thinking: String,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                ContentDelta::ThinkingDelta {
                    thinking: r.thinking,
                }
            }
            Some("signature_delta") => {
                #[derive(Deserialize)]
                struct R {
                    #[serde(default)]
                    signature: String,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                ContentDelta::SignatureDelta {
                    signature: r.signature,
                }
            }
            Some("citations_delta") => {
                #[derive(Deserialize)]
                struct R {
                    #[serde(default)]
                    citation: serde_json::Value,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                ContentDelta::CitationsDelta {
                    citation: r.citation,
                }
            }
            _ => ContentDelta::Unknown(value),
        };
        Ok(delta)
    }
}

impl Serialize for StreamEvent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            StreamEvent::MessageStart { message } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "message_start")?;
                map.serialize_entry("message", message)?;
                map.end()
            }
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "content_block_start")?;
                map.serialize_entry("index", index)?;
                map.serialize_entry("content_block", content_block)?;
                map.end()
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "content_block_delta")?;
                map.serialize_entry("index", index)?;
                map.serialize_entry("delta", delta)?;
                map.end()
            }
            StreamEvent::ContentBlockStop { index } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "content_block_stop")?;
                map.serialize_entry("index", index)?;
                map.end()
            }
            StreamEvent::MessageDelta { delta, usage } => {
                let len = if usage.is_some() { 3 } else { 2 };
                let mut map = serializer.serialize_map(Some(len))?;
                map.serialize_entry("type", "message_delta")?;
                map.serialize_entry("delta", delta)?;
                if let Some(usage) = usage {
                    map.serialize_entry("usage", usage)?;
                }
                map.end()
            }
            StreamEvent::MessageStop => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "message_stop")?;
                map.end()
            }
            StreamEvent::Ping => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "ping")?;
                map.end()
            }
            StreamEvent::Error { error } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "error")?;
                map.serialize_entry("error", error)?;
                map.end()
            }
            StreamEvent::Unknown(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for StreamEvent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let tag = value
            .get("type")
            .and_then(|t| t.as_str())
            .map(str::to_owned);
        let event = match tag.as_deref() {
            Some("message_start") => {
                #[derive(Deserialize)]
                struct R {
                    message: Message,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                StreamEvent::MessageStart { message: r.message }
            }
            Some("content_block_start") => {
                #[derive(Deserialize)]
                struct R {
                    index: usize,
                    content_block: ContentBlock,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                StreamEvent::ContentBlockStart {
                    index: r.index,
                    content_block: r.content_block,
                }
            }
            Some("content_block_delta") => {
                #[derive(Deserialize)]
                struct R {
                    index: usize,
                    delta: ContentDelta,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                StreamEvent::ContentBlockDelta {
                    index: r.index,
                    delta: r.delta,
                }
            }
            Some("content_block_stop") => {
                #[derive(Deserialize)]
                struct R {
                    index: usize,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                StreamEvent::ContentBlockStop { index: r.index }
            }
            Some("message_delta") => {
                #[derive(Deserialize)]
                struct R {
                    delta: MessageDeltaBody,
                    #[serde(default)]
                    usage: Option<UsageDelta>,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                StreamEvent::MessageDelta {
                    delta: r.delta,
                    usage: r.usage,
                }
            }
            Some("message_stop") => StreamEvent::MessageStop,
            Some("ping") => StreamEvent::Ping,
            Some("error") => {
                #[derive(Deserialize)]
                struct R {
                    error: StreamErrorBody,
                }
                let r: R = serde_json::from_value(value).map_err(de::Error::custom)?;
                StreamEvent::Error { error: r.error }
            }
            _ => StreamEvent::Unknown(value),
        };
        Ok(event)
    }
}

// ---------------------------------------------------------------------------
// SSE record decoder.
// ---------------------------------------------------------------------------

/// Splits an `SSE` line into its field name and value (`"data: x"` →
/// `("data", "x")`), stripping a single optional space after the colon. A line
/// with no colon is a field name with an empty value.
fn parse_field(line: &str) -> (&str, &str) {
    match line.find(':') {
        Some(idx) => {
            let field = &line[..idx];
            let value = &line[idx + 1..];
            let value = value.strip_prefix(' ').unwrap_or(value);
            (field, value)
        }
        None => (line, ""),
    }
}

/// An incremental SSE record decoder over an arbitrarily-chunked byte stream.
///
/// Bytes are buffered; complete newline-terminated lines are consumed one at a
/// time (tolerating both `\n` and `\r\n`). `data:` lines accumulate (joined with
/// `\n` for multi-line data), comment lines (`:` prefix) and other fields are
/// ignored, and a blank line dispatches the accumulated event's `data` payload.
#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
    data: String,
    saw_data: bool,
}

impl SseDecoder {
    fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// Returns the next complete event's `data` payload, or `None` if more bytes
    /// are required. Records without a `data` field are skipped.
    fn next_event_data(&mut self) -> Option<String> {
        loop {
            let newline = self.buffer.iter().position(|&byte| byte == b'\n')?;
            let line_bytes: Vec<u8> = self.buffer.drain(..=newline).collect();
            let mut end = line_bytes.len() - 1; // exclude the trailing '\n'
            if end > 0 && line_bytes[end - 1] == b'\r' {
                end -= 1; // tolerate CRLF line endings
            }
            let line = String::from_utf8_lossy(&line_bytes[..end]);

            if line.is_empty() {
                if self.saw_data {
                    self.saw_data = false;
                    return Some(std::mem::take(&mut self.data));
                }
                self.data.clear();
                continue;
            }
            if line.starts_with(':') {
                continue; // comment / heartbeat line
            }
            let (field, value) = parse_field(line.as_ref());
            if field == "data" {
                if self.saw_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.saw_data = true;
            }
            // `event:`, `id:`, `retry:`, and unknown fields are ignored; the
            // event's `type` is read from the JSON `data` payload instead.
        }
    }

    /// Flushes a trailing event that the stream ended without terminating with a
    /// blank line.
    fn flush(&mut self) -> Option<String> {
        if !self.buffer.is_empty() {
            let line_bytes = std::mem::take(&mut self.buffer);
            let mut end = line_bytes.len();
            if end > 0 && line_bytes[end - 1] == b'\n' {
                end -= 1;
            }
            if end > 0 && line_bytes[end - 1] == b'\r' {
                end -= 1;
            }
            let line = String::from_utf8_lossy(&line_bytes[..end]);
            if !line.is_empty() && !line.starts_with(':') {
                let (field, value) = parse_field(line.as_ref());
                if field == "data" {
                    if self.saw_data {
                        self.data.push('\n');
                    }
                    self.data.push_str(value);
                    self.saw_data = true;
                }
            }
        }
        if self.saw_data {
            self.saw_data = false;
            Some(std::mem::take(&mut self.data))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Accumulation.
// ---------------------------------------------------------------------------

/// An upper bound on the number of accumulated content blocks.
///
/// Blocks normally arrive contiguously from index 0, so any index at or beyond
/// this bound indicates a malformed or hostile stream. The cap prevents a single
/// `content_block_start` event with an enormous `index` (a few dozen bytes on the
/// wire) from driving [`set_block`] to allocate placeholder blocks up to that
/// index and exhausting memory.
const MAX_STREAM_CONTENT_BLOCKS: usize = 1 << 16;

/// Places `block` at `index` in `content`, extending with placeholder blocks if
/// a gap appears (blocks normally arrive contiguously from index 0).
///
/// An `index` at or beyond [`MAX_STREAM_CONTENT_BLOCKS`] is ignored rather than
/// gap-filled, so an untrusted byte source cannot force unbounded allocation.
fn set_block(content: &mut Vec<ContentBlock>, index: usize, block: ContentBlock) {
    if index < content.len() {
        content[index] = block;
    } else if index < MAX_STREAM_CONTENT_BLOCKS {
        while content.len() < index {
            content.push(ContentBlock::Unknown(serde_json::Value::Null));
        }
        content.push(block);
    }
    // else: an absurd index — drop the block to avoid unbounded allocation.
}

/// Merges a [`UsageDelta`] into a [`Usage`], replacing each present field with
/// the delta's cumulative total.
fn merge_usage(target: &mut Usage, delta: &UsageDelta) {
    if let Some(value) = delta.input_tokens {
        target.input_tokens = value;
    }
    if let Some(value) = delta.output_tokens {
        target.output_tokens = value;
    }
    if let Some(value) = delta.cache_creation_input_tokens {
        target.cache_creation_input_tokens = Some(value);
    }
    if let Some(value) = delta.cache_read_input_tokens {
        target.cache_read_input_tokens = Some(value);
    }
    for (key, value) in &delta.extra {
        target.extra.insert(key.clone(), value.clone());
    }
}

/// Assembles a [`Message`] from the stream events as they pass by.
#[derive(Default)]
struct Accumulator {
    message: Option<Message>,
    /// Per-index buffers for `tool_use` blocks' `input_json_delta` fragments.
    json_buffers: HashMap<usize, String>,
}

impl Accumulator {
    fn apply(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::MessageStart { message } => {
                self.message = Some(message.clone());
                self.json_buffers.clear();
            }
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                if let Some(message) = self.message.as_mut() {
                    set_block(&mut message.content, *index, content_block.clone());
                }
                if matches!(
                    content_block,
                    ContentBlock::ToolUse(_) | ContentBlock::ServerToolUse(_)
                ) {
                    self.json_buffers.insert(*index, String::new());
                }
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                self.apply_delta(*index, delta);
            }
            StreamEvent::ContentBlockStop { index } => {
                self.finish_block(*index);
            }
            StreamEvent::MessageDelta { delta, usage } => {
                if let Some(message) = self.message.as_mut() {
                    if delta.stop_reason.is_some() {
                        message.stop_reason = delta.stop_reason.clone();
                    }
                    if delta.stop_sequence.is_some() {
                        message.stop_sequence = delta.stop_sequence.clone();
                    }
                    if delta.stop_details.is_some() {
                        message.stop_details = delta.stop_details.clone();
                    }
                    if let Some(usage) = usage {
                        merge_usage(&mut message.usage, usage);
                    }
                }
            }
            StreamEvent::MessageStop
            | StreamEvent::Ping
            | StreamEvent::Error { .. }
            | StreamEvent::Unknown(_) => {}
        }
    }

    fn apply_delta(&mut self, index: usize, delta: &ContentDelta) {
        if let ContentDelta::InputJsonDelta { partial_json } = delta {
            self.json_buffers
                .entry(index)
                .or_default()
                .push_str(partial_json);
            return;
        }
        let Some(message) = self.message.as_mut() else {
            return;
        };
        let Some(block) = message.content.get_mut(index) else {
            return;
        };
        match delta {
            ContentDelta::TextDelta { text } => {
                if let ContentBlock::Text(inner) = block {
                    inner.text.push_str(text);
                }
            }
            ContentDelta::ThinkingDelta { thinking } => {
                if let ContentBlock::Thinking(inner) = block {
                    inner.thinking.push_str(thinking);
                }
            }
            ContentDelta::SignatureDelta { signature } => {
                if let ContentBlock::Thinking(inner) = block {
                    inner.signature.push_str(signature);
                }
            }
            ContentDelta::CitationsDelta { citation } => {
                if let ContentBlock::Text(inner) = block {
                    inner
                        .citations
                        .get_or_insert_with(Vec::new)
                        .push(citation.clone());
                }
            }
            ContentDelta::InputJsonDelta { .. } | ContentDelta::Unknown(_) => {}
        }
    }

    fn finish_block(&mut self, index: usize) {
        let Some(json) = self.json_buffers.remove(&index) else {
            return;
        };
        let parsed = if json.trim().is_empty() {
            // An empty input buffer means an empty tool input object.
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            // The concatenated fragments should parse as the tool_use `input`
            // object. If they do not (a truncated or corrupted stream), preserve
            // the raw accumulated bytes as a JSON string rather than silently
            // discarding them as `null`, so the loss is detectable by the caller.
            match serde_json::from_str(&json) {
                Ok(value) => value,
                Err(_) => serde_json::Value::String(json),
            }
        };
        if let Some(message) = self.message.as_mut() {
            if let Some(ContentBlock::ToolUse(inner) | ContentBlock::ServerToolUse(inner)) =
                message.content.get_mut(index)
            {
                inner.input = parsed;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MessageStream.
// ---------------------------------------------------------------------------

/// The boxed byte stream a [`MessageStream`] reads from.
type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>;

/// A [`Stream`] of [`StreamEvent`]s decoded from a streaming `POST /v1/messages`
/// response, which simultaneously accumulates a final [`Message`].
///
/// Drive it with your async runtime via [`futures_util::StreamExt`]. Each item
/// is a `Result<StreamEvent>`: a decode failure or an in-stream `error` event
/// yields `Err` and ends the stream. After the stream is exhausted (or at any
/// point), [`final_message`](MessageStream::final_message) returns the message
/// assembled so far; [`collect_final`](MessageStream::collect_final) drains the
/// stream and returns the complete [`Message`].
///
/// # Examples
///
/// ```no_run
/// use crimson_crab::api::MessagesRequest;
/// use crimson_crab::types::MessageParam;
///
/// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
/// let request = MessagesRequest::builder()
///     .model("claude-opus-4-8")
///     .max_tokens(1024)
///     .messages(vec![MessageParam::user("Hi")])
///     .build()?;
/// let message = client.messages().stream(&request).await?.collect_final().await?;
/// println!("{}", message.text());
/// # Ok(())
/// # }
/// ```
pub struct MessageStream {
    source: ByteStream,
    decoder: SseDecoder,
    accumulator: Accumulator,
    source_done: bool,
    done: bool,
}

impl MessageStream {
    /// Wraps a raw streaming HTTP response.
    pub(crate) fn new(response: reqwest::Response) -> Self {
        let source = response
            .bytes_stream()
            .map(|chunk| chunk.map_err(Error::from));
        Self::from_pinned(Box::pin(source))
    }

    /// Builds a [`MessageStream`] from an arbitrary byte stream.
    ///
    /// This low-level constructor exists for custom transports and testing; most
    /// callers obtain a stream via
    /// [`Messages::stream`](crate::api::messages::Messages::stream).
    ///
    /// # Examples
    ///
    /// ```
    /// use bytes::Bytes;
    /// use crimson_crab::streaming::MessageStream;
    /// use futures_util::stream;
    ///
    /// let chunks: Vec<crimson_crab::Result<Bytes>> =
    ///     vec![Ok(Bytes::from_static(b"event: ping\ndata: {\"type\":\"ping\"}\n\n"))];
    /// let stream = MessageStream::from_byte_stream(stream::iter(chunks));
    /// assert!(stream.final_message().is_none());
    /// ```
    #[doc(hidden)]
    pub fn from_byte_stream<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes>> + Send + 'static,
    {
        Self::from_pinned(Box::pin(stream))
    }

    fn from_pinned(source: ByteStream) -> Self {
        Self {
            source,
            decoder: SseDecoder::default(),
            accumulator: Accumulator::default(),
            source_done: false,
            done: false,
        }
    }

    /// Returns the message accumulated so far, or `None` before the
    /// `message_start` event has been seen.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytes::Bytes;
    /// use crimson_crab::streaming::MessageStream;
    /// use futures_util::stream;
    ///
    /// let stream =
    ///     MessageStream::from_byte_stream(stream::iter(Vec::<crimson_crab::Result<Bytes>>::new()));
    /// assert!(stream.final_message().is_none());
    /// ```
    pub fn final_message(&self) -> Option<&Message> {
        self.accumulator.message.as_ref()
    }

    /// Drains the stream to completion and returns the accumulated [`Message`].
    ///
    /// Returns the first error encountered (a decode failure, an in-stream
    /// `error` event, or a transport error). Returns [`Error::Stream`] if the
    /// stream ended without a `message_start` event.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crimson_crab::api::MessagesRequest;
    /// use crimson_crab::types::MessageParam;
    ///
    /// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
    /// let request = MessagesRequest::builder()
    ///     .model("claude-opus-4-8")
    ///     .max_tokens(1024)
    ///     .messages(vec![MessageParam::user("Hi")])
    ///     .build()?;
    /// let message = client.messages().stream(&request).await?.collect_final().await?;
    /// let _ = message.stop_reason;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn collect_final(mut self) -> Result<Message> {
        while let Some(item) = self.next().await {
            item?;
        }
        self.accumulator
            .message
            .ok_or_else(|| Error::Stream("stream ended without a message_start event".to_string()))
    }

    /// Decodes one event `data` payload, accumulating it and returning the item
    /// to yield (or `None` to skip an empty payload).
    fn decode(&mut self, data: &str) -> Option<Result<StreamEvent>> {
        if data.trim().is_empty() {
            return None;
        }
        match serde_json::from_str::<StreamEvent>(data) {
            Ok(StreamEvent::Error { error }) => {
                self.done = true;
                Some(Err(error::from_error_body(error.error_type, error.message)))
            }
            Ok(event) => {
                self.accumulator.apply(&event);
                Some(Ok(event))
            }
            Err(source) => {
                self.done = true;
                Some(Err(Error::from(source)))
            }
        }
    }
}

impl Stream for MessageStream {
    type Item = Result<StreamEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        loop {
            if let Some(data) = this.decoder.next_event_data() {
                if let Some(item) = this.decode(&data) {
                    return Poll::Ready(Some(item));
                }
                continue;
            }

            if this.source_done {
                if let Some(data) = this.decoder.flush() {
                    if let Some(item) = this.decode(&data) {
                        return Poll::Ready(Some(item));
                    }
                }
                this.done = true;
                return Poll::Ready(None);
            }

            match this.source.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    this.decoder.push(&bytes);
                    continue;
                }
                Poll::Ready(Some(Err(err))) => {
                    this.done = true;
                    return Poll::Ready(Some(Err(err)));
                }
                Poll::Ready(None) => {
                    this.source_done = true;
                    continue;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_decoder_tolerates_crlf_and_comments() {
        let mut decoder = SseDecoder::default();
        decoder.push(b": heartbeat comment\r\n");
        decoder.push(b"event: ping\r\ndata: {\"type\":\"ping\"}\r\n\r\n");
        let data = decoder.next_event_data().expect("one record");
        assert_eq!(data, "{\"type\":\"ping\"}");
        assert!(decoder.next_event_data().is_none());
    }

    #[test]
    fn sse_decoder_joins_multiline_data() {
        let mut decoder = SseDecoder::default();
        decoder.push(b"data: line one\ndata: line two\n\n");
        let data = decoder.next_event_data().expect("one record");
        assert_eq!(data, "line one\nline two");
    }

    #[test]
    fn content_delta_round_trips() {
        for raw in [
            r#"{"type":"text_delta","text":"Hi"}"#,
            r#"{"type":"input_json_delta","partial_json":"{\"a\":1}"}"#,
            r#"{"type":"thinking_delta","thinking":"hmm"}"#,
            r#"{"type":"signature_delta","signature":"sig"}"#,
        ] {
            let delta: ContentDelta = serde_json::from_str(raw).expect("parse");
            let out = serde_json::to_string(&delta).expect("serialize");
            let reparsed: ContentDelta = serde_json::from_str(&out).expect("reparse");
            assert_eq!(delta, reparsed);
        }
    }

    #[test]
    fn empty_input_json_becomes_empty_object() {
        let mut acc = Accumulator::default();
        let message_start: StreamEvent = serde_json::from_str(
            r#"{"type":"message_start","message":{"id":"m1","type":"message","role":"assistant","model":"claude-opus-4-8","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}}}"#,
        )
        .expect("parse message_start");
        acc.apply(&message_start);
        let start: StreamEvent = serde_json::from_str(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"noop","input":{}}}"#,
        )
        .expect("parse start");
        acc.apply(&start);
        // No input_json_delta events at all -> empty buffer parses to `{}`.
        let stop: StreamEvent =
            serde_json::from_str(r#"{"type":"content_block_stop","index":0}"#).expect("parse stop");
        acc.apply(&stop);
        let message = acc.message.expect("no message");
        match &message.content[0] {
            ContentBlock::ToolUse(inner) => {
                assert_eq!(inner.input, serde_json::json!({}));
            }
            other => panic!("expected tool_use, got {other:?}"),
        }
    }
}
