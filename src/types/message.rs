//! Messages: the response [`Message`], the request [`MessageParam`], and the
//! supporting value types (roles, system prompt, usage, stop reason/details,
//! metadata).

use serde::{Deserialize, Serialize};

use crate::types::content::{ContentBlock, ContentBlockParam};
use crate::types::string_enum;

string_enum! {
    /// The author of a message.
    ///
    /// Like every other discriminated enum in the crate, `Role` carries a
    /// forward-compatible [`Role::Unknown`] catch-all, so a role value the SDK
    /// has never seen (e.g. a future `"system"`-style author) deserializes
    /// instead of failing the entire [`Message`] parse.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::Role;
    /// assert_eq!(serde_json::to_value(Role::User).unwrap(), serde_json::json!("user"));
    /// // An unrecognised role is preserved rather than erroring.
    /// let r: Role = serde_json::from_str("\"future_role\"").unwrap();
    /// assert_eq!(r, Role::Unknown("future_role".to_string()));
    /// ```
    pub enum Role {
        /// A message authored by the user.
        User = "user",
        /// A message authored by the assistant (the model).
        Assistant = "assistant",
    }
}

string_enum! {
    /// Why the model stopped generating.
    ///
    /// `null` on the wire (mid-stream) maps to `Option::None` on
    /// [`Message::stop_reason`]. Unknown values are preserved in
    /// [`StopReason::Unknown`] rather than erroring.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::StopReason;
    /// assert_eq!(StopReason::EndTurn.as_str(), "end_turn");
    /// // A stop reason the SDK has never seen still deserializes.
    /// let r: StopReason = serde_json::from_str("\"some_new_reason\"").unwrap();
    /// assert_eq!(r, StopReason::Unknown("some_new_reason".to_string()));
    /// ```
    pub enum StopReason {
        /// The model reached a natural stopping point.
        EndTurn = "end_turn",
        /// The `max_tokens` limit was hit.
        MaxTokens = "max_tokens",
        /// One of the request's `stop_sequences` was produced.
        StopSequence = "stop_sequence",
        /// The model emitted tool calls and is waiting for their results.
        ToolUse = "tool_use",
        /// A long-running turn was paused and should be resumed.
        PauseTurn = "pause_turn",
        /// The model declined to answer (see [`StopDetails`]).
        Refusal = "refusal",
        /// The conversation exceeded the model's context window.
        ModelContextWindowExceeded = "model_context_window_exceeded",
    }
}

/// Extra detail populated only when `stop_reason == "refusal"`.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::StopDetails;
///
/// let json = serde_json::json!({"type": "refusal", "category": "cyber", "explanation": "..."});
/// let d: StopDetails = serde_json::from_value(json).unwrap();
/// assert_eq!(d.category.as_deref(), Some("cyber"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopDetails {
    /// The detail type, e.g. `"refusal"`.
    #[serde(rename = "type")]
    pub detail_type: String,
    /// The refusal category (`"cyber"`, `"bio"`, …), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// A human-readable explanation, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

/// Token accounting for a request/response.
///
/// `input_tokens` and `output_tokens` are always present; the cache counters
/// are present when prompt caching is in play. Any additional fields the API
/// adds (service tier, iterations, …) are captured in [`Usage::extra`].
///
/// # Examples
///
/// ```
/// use crimson_crab::types::Usage;
///
/// let json = serde_json::json!({
///     "input_tokens": 10, "output_tokens": 25,
///     "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0,
///     "service_tier": "standard"
/// });
/// let usage: Usage = serde_json::from_value(json).unwrap();
/// assert_eq!(usage.input_tokens, 10);
/// assert_eq!(usage.extra["service_tier"], serde_json::json!("standard"));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// Number of input tokens billed.
    pub input_tokens: u64,
    /// Number of output tokens billed.
    pub output_tokens: u64,
    /// Tokens written to the prompt cache, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    /// Tokens read from the prompt cache, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
    /// Forward-compatible catch-all for any other usage fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// The container populated when the code-execution tool is used.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Container {
    /// The container id.
    pub id: String,
    /// Forward-compatible catch-all for any other container fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A message returned by the API (`POST /v1/messages`).
///
/// # Examples
///
/// ```
/// use crimson_crab::types::{Message, Role, StopReason};
///
/// let json = serde_json::json!({
///     "id": "msg_01ABC",
///     "type": "message",
///     "role": "assistant",
///     "model": "claude-opus-4-8",
///     "content": [{"type": "text", "text": "Hello!"}],
///     "stop_reason": "end_turn",
///     "stop_sequence": null,
///     "usage": {"input_tokens": 10, "output_tokens": 3}
/// });
/// let msg: Message = serde_json::from_value(json).unwrap();
/// assert_eq!(msg.role, Role::Assistant);
/// assert_eq!(msg.stop_reason, Some(StopReason::EndTurn));
/// assert_eq!(msg.text(), "Hello!");
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Unique message id (`msg_...`).
    pub id: String,
    /// The object type; always `"message"`.
    #[serde(rename = "type")]
    pub message_type: String,
    /// The author; always [`Role::Assistant`] for responses.
    pub role: Role,
    /// The model that produced the message.
    pub model: String,
    /// The generated content blocks.
    pub content: Vec<ContentBlock>,
    /// Why generation stopped (`None` mid-stream).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    /// The matched stop sequence, if `stop_reason == StopSequence`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    /// Refusal detail, populated only when `stop_reason == Refusal`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_details: Option<StopDetails>,
    /// Token usage for this message.
    pub usage: Usage,
    /// The code-execution container, if one was created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<Container>,
}

impl Message {
    /// Concatenates the text of all [`ContentBlock::Text`] blocks.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{ContentBlock, Message, Role, TextBlock, Usage};
    ///
    /// let msg = Message {
    ///     id: "msg_1".into(),
    ///     message_type: "message".into(),
    ///     role: Role::Assistant,
    ///     model: "claude-opus-4-8".into(),
    ///     content: vec![
    ///         ContentBlock::Text(TextBlock::new("Hello, ")),
    ///         ContentBlock::Text(TextBlock::new("world")),
    ///     ],
    ///     stop_reason: None,
    ///     stop_sequence: None,
    ///     stop_details: None,
    ///     usage: Usage::default(),
    ///     container: None,
    /// };
    /// assert_eq!(msg.text(), "Hello, world");
    /// ```
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(ContentBlock::as_text)
            .collect()
    }

    /// Converts this response message into a [`MessageParam`] suitable for
    /// echoing the assistant turn back into a follow-up request.
    ///
    /// Each response [`ContentBlock`] is mapped to the equivalent request
    /// [`ContentBlockParam`] (see the [`From<ContentBlock>`] impl), so the
    /// wire-api tool-loop contract ("append the assistant `content` verbatim")
    /// is a single call instead of a lossy `serde_json` re-encode/decode
    /// round-trip. Response-only fields that have no request counterpart (such
    /// as [`TextBlock`](crate::types::TextBlock) citations) are dropped, and
    /// response-only block types are preserved verbatim as raw JSON.
    ///
    /// [`From<ContentBlock>`]: crate::types::ContentBlockParam
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{ContentBlock, Message, MessageContent, MessageParam, Role, TextBlock, Usage};
    ///
    /// let msg = Message {
    ///     id: "msg_1".into(),
    ///     message_type: "message".into(),
    ///     role: Role::Assistant,
    ///     model: "claude-opus-4-8".into(),
    ///     content: vec![ContentBlock::Text(TextBlock::new("Hi"))],
    ///     stop_reason: None,
    ///     stop_sequence: None,
    ///     stop_details: None,
    ///     usage: Usage::default(),
    ///     container: None,
    /// };
    /// let param: MessageParam = msg.into_param();
    /// assert_eq!(param.role, Role::Assistant);
    /// assert!(matches!(param.content, MessageContent::Blocks(_)));
    /// ```
    pub fn into_param(self) -> MessageParam {
        MessageParam {
            role: self.role,
            content: MessageContent::Blocks(
                self.content
                    .into_iter()
                    .map(ContentBlockParam::from)
                    .collect(),
            ),
        }
    }
}

/// Request metadata (`metadata` field).
///
/// # Examples
///
/// ```
/// use crimson_crab::types::Metadata;
///
/// let m = Metadata::with_user_id("user_123");
/// assert_eq!(serde_json::to_value(&m).unwrap(), serde_json::json!({"user_id": "user_123"}));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    /// An opaque, non-PII identifier for the end user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Forward-compatible catch-all for any other metadata fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Metadata {
    /// Builds metadata carrying just a `user_id`.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::Metadata;
    /// assert_eq!(Metadata::with_user_id("u").user_id.as_deref(), Some("u"));
    /// ```
    pub fn with_user_id(user_id: impl Into<String>) -> Self {
        Self {
            user_id: Some(user_id.into()),
            extra: serde_json::Map::new(),
        }
    }
}

/// The `system` prompt: a plain string or an array of text blocks (which may
/// carry cache breakpoints).
///
/// # Examples
///
/// ```
/// use crimson_crab::types::{ContentBlockParam, SystemPrompt};
///
/// let s: SystemPrompt = "You are terse.".into();
/// assert_eq!(serde_json::to_value(&s).unwrap(), serde_json::json!("You are terse."));
///
/// let blocks: SystemPrompt = vec![ContentBlockParam::text("You are terse.")].into();
/// assert!(serde_json::to_value(&blocks).unwrap().is_array());
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    /// A single system string.
    Text(String),
    /// An array of content blocks (typically `text` with optional caching).
    Blocks(Vec<ContentBlockParam>),
}

impl From<String> for SystemPrompt {
    fn from(s: String) -> Self {
        SystemPrompt::Text(s)
    }
}

impl From<&str> for SystemPrompt {
    fn from(s: &str) -> Self {
        SystemPrompt::Text(s.to_string())
    }
}

impl From<Vec<ContentBlockParam>> for SystemPrompt {
    fn from(blocks: Vec<ContentBlockParam>) -> Self {
        SystemPrompt::Blocks(blocks)
    }
}

/// The content of a [`MessageParam`]: a plain string or an array of content
/// blocks.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::{ContentBlockParam, MessageContent};
///
/// let c: MessageContent = "hi".into();
/// assert_eq!(serde_json::to_value(&c).unwrap(), serde_json::json!("hi"));
///
/// let c: MessageContent = vec![ContentBlockParam::text("hi")].into();
/// assert!(serde_json::to_value(&c).unwrap().is_array());
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// The string shorthand form.
    Text(String),
    /// An array of content blocks.
    Blocks(Vec<ContentBlockParam>),
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        MessageContent::Text(s)
    }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        MessageContent::Text(s.to_string())
    }
}

impl From<Vec<ContentBlockParam>> for MessageContent {
    fn from(blocks: Vec<ContentBlockParam>) -> Self {
        MessageContent::Blocks(blocks)
    }
}

impl From<ContentBlockParam> for MessageContent {
    fn from(block: ContentBlockParam) -> Self {
        MessageContent::Blocks(vec![block])
    }
}

/// A message supplied in a request's `messages` array.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::{ContentBlockParam, MessageParam, Role};
///
/// let user = MessageParam::user("Hello");
/// assert_eq!(user.role, Role::User);
/// assert_eq!(
///     serde_json::to_value(&user).unwrap(),
///     serde_json::json!({"role": "user", "content": "Hello"})
/// );
///
/// let assistant = MessageParam::assistant(vec![ContentBlockParam::text("Hi")]);
/// assert_eq!(assistant.role, Role::Assistant);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageParam {
    /// The author of the message.
    pub role: Role,
    /// The message content (string shorthand or content blocks).
    pub content: MessageContent,
}

impl MessageParam {
    /// Builds a user message from anything convertible into [`MessageContent`].
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::MessageParam;
    /// let m = MessageParam::user("Hello");
    /// assert!(matches!(m.role, crimson_crab::types::Role::User));
    /// ```
    pub fn user(content: impl Into<MessageContent>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    /// Builds an assistant message from anything convertible into
    /// [`MessageContent`].
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::MessageParam;
    /// let m = MessageParam::assistant("Sure!");
    /// assert!(matches!(m.role, crimson_crab::types::Role::Assistant));
    /// ```
    pub fn assistant(content: impl Into<MessageContent>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt<T>(json: serde_json::Value) -> serde_json::Value
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let parsed: T = serde_json::from_value(json).expect("deserialize");
        let out = serde_json::to_value(&parsed).expect("serialize");
        let reparsed: T = serde_json::from_value(out.clone()).expect("re-deserialize");
        assert_eq!(parsed, reparsed, "struct round-trip mismatch");
        out
    }

    // Fixture: Message response (docs/wire-api.md "Response (Message)").
    #[test]
    fn message_response_round_trips() {
        let j = serde_json::json!({
            "id": "msg_01ABC",
            "type": "message",
            "role": "assistant",
            "model": "claude-opus-4-8",
            "content": [{"type": "text", "text": "Hi there."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "stop_details": null,
            "usage": {
                "input_tokens": 10,
                "output_tokens": 25,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0
            },
            "container": null
        });
        rt::<Message>(j);
    }

    // Fixture: a refusal carries stop_details (docs/wire-api.md).
    #[test]
    fn refusal_message_with_stop_details() {
        let j = serde_json::json!({
            "id": "msg_02",
            "type": "message",
            "role": "assistant",
            "model": "claude-opus-4-8",
            "content": [],
            "stop_reason": "refusal",
            "stop_sequence": null,
            "stop_details": {"type": "refusal", "category": "cyber", "explanation": "no"},
            "usage": {"input_tokens": 3, "output_tokens": 0}
        });
        let msg: Message = serde_json::from_value(j).unwrap();
        assert_eq!(msg.stop_reason, Some(StopReason::Refusal));
        assert_eq!(
            msg.stop_details.as_ref().unwrap().category.as_deref(),
            Some("cyber")
        );
    }

    // Fixture: streaming message_start's message has minimal usage and no
    // stop_details / container (docs/wire-api.md "Streaming").
    #[test]
    fn streaming_message_start_shape() {
        let j = serde_json::json!({
            "id": "msg_stream",
            "type": "message",
            "role": "assistant",
            "model": "claude-opus-4-8",
            "content": [],
            "stop_reason": null,
            "stop_sequence": null,
            "usage": {"input_tokens": 25, "output_tokens": 1}
        });
        let msg: Message = serde_json::from_value(j).unwrap();
        assert_eq!(msg.stop_reason, None);
        assert_eq!(msg.usage.cache_read_input_tokens, None);
    }

    // Fixture: forward-compat usage fields flatten into `extra`.
    #[test]
    fn usage_extra_fields_are_captured() {
        let j = serde_json::json!({
            "input_tokens": 10,
            "output_tokens": 25,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0,
            "service_tier": "standard",
            "iterations": 2
        });
        assert_eq!(rt::<Usage>(j.clone()), j);
        let usage: Usage = serde_json::from_value(j).unwrap();
        assert_eq!(usage.extra["service_tier"], serde_json::json!("standard"));
        assert_eq!(usage.extra["iterations"], serde_json::json!(2));
    }

    // Regression: an unrecognised role must not fail the whole Message parse.
    #[test]
    fn unknown_role_does_not_fail_message_deserialize() {
        let j = serde_json::json!({
            "id": "msg_role",
            "type": "message",
            "role": "assistant_v2",
            "model": "claude-opus-4-8",
            "content": [{"type": "text", "text": "hi"}],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });
        let msg: Message = serde_json::from_value(j).expect("unknown role should still parse");
        assert_eq!(msg.role, Role::Unknown("assistant_v2".to_string()));
        // And the role round-trips back to its original wire string.
        assert_eq!(
            serde_json::to_value(&msg.role).unwrap(),
            serde_json::json!("assistant_v2")
        );
    }

    // Fixture: unknown stop reason is preserved rather than erroring.
    #[test]
    fn unknown_stop_reason_is_preserved() {
        let parsed: StopReason =
            serde_json::from_value(serde_json::json!("some_future_reason")).unwrap();
        assert_eq!(
            parsed,
            StopReason::Unknown("some_future_reason".to_string())
        );
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            serde_json::json!("some_future_reason")
        );
    }

    // Fixture: system prompt string and block forms.
    #[test]
    fn system_prompt_forms() {
        let s = serde_json::json!("You are terse.");
        assert_eq!(rt::<SystemPrompt>(s.clone()), s);

        let blocks = serde_json::json!([
            {"type": "text", "text": "You are terse.", "cache_control": {"type": "ephemeral"}}
        ]);
        assert_eq!(rt::<SystemPrompt>(blocks.clone()), blocks);
    }

    // Fixture: request messages, both content forms.
    #[test]
    fn message_param_forms() {
        let string_form = serde_json::json!({"role": "user", "content": "hi"});
        assert_eq!(rt::<MessageParam>(string_form.clone()), string_form);

        let block_form = serde_json::json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "..."}]
        });
        assert_eq!(rt::<MessageParam>(block_form.clone()), block_form);
    }

    // Fixture: error envelope (docs/wire-api.md "Errors (non-2xx)"). The typed
    // error module is a later stage; this local mirror pins the wire shape.
    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct ErrorEnvelope {
        #[serde(rename = "type")]
        envelope_type: String,
        error: ApiErrorBody,
        request_id: Option<String>,
    }

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct ApiErrorBody {
        #[serde(rename = "type")]
        error_type: String,
        message: String,
    }

    #[test]
    fn error_envelope_round_trips() {
        let j = serde_json::json!({
            "type": "error",
            "error": {"type": "invalid_request_error", "message": "bad thing"},
            "request_id": "req_123"
        });
        let parsed: ErrorEnvelope = serde_json::from_value(j.clone()).unwrap();
        assert_eq!(parsed.error.error_type, "invalid_request_error");
        assert_eq!(serde_json::to_value(&parsed).unwrap(), j);
    }
}
