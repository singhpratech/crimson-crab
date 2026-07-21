//! Content blocks — the pieces that make up message content in both directions.
//!
//! [`ContentBlock`] models the blocks the API returns (`text`, `tool_use`,
//! `thinking`, server-tool results, …). [`ContentBlockParam`] models the blocks
//! a request sends (`text`, `image`, `document`, `tool_result`, echoed
//! `thinking`, …). Both are `type`-tagged enums with a forward-compatible
//! `Unknown` catch-all (see [`crate::types`] for the mechanism), so unfamiliar
//! block types round-trip untouched rather than erroring.

use serde::{Deserialize, Serialize};

use crate::types::cache::CacheControl;
use crate::types::tagged_enum;
use crate::types::tool::ToolResultContent;

// ---------------------------------------------------------------------------
// Shared inner structs (identical request/response shapes are reused).
// ---------------------------------------------------------------------------

/// A `text` block returned by the API.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    /// The text content.
    pub text: String,
    /// Citations attached to the text, kept as raw JSON for forward
    /// compatibility with citation shapes the SDK does not model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<serde_json::Value>>,
}

impl TextBlock {
    /// Creates a plain text block with no citations.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::TextBlock;
    ///
    /// assert_eq!(TextBlock::new("hi").text, "hi");
    /// ```
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            citations: None,
        }
    }
}

/// A `tool_use` block (also reused for `server_tool_use`, and for echoing an
/// assistant tool call back in a request).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolUseBlock {
    /// Unique identifier for this tool invocation.
    pub id: String,
    /// The name of the tool being invoked.
    pub name: String,
    /// The tool input, an arbitrary JSON object.
    pub input: serde_json::Value,
}

/// A `thinking` block (extended reasoning). Reused for request echoes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThinkingBlock {
    /// The reasoning text (may be empty when `display` is `omitted`).
    pub thinking: String,
    /// Opaque signature that must be echoed back unchanged in follow-up turns.
    pub signature: String,
}

/// A `redacted_thinking` block whose reasoning has been encrypted by the API.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedThinkingBlock {
    /// Opaque redacted payload; echo back unchanged in follow-up turns.
    pub data: String,
}

/// A `web_search_tool_result` block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WebSearchToolResultBlock {
    /// The id of the `server_tool_use` block this result corresponds to.
    pub tool_use_id: String,
    /// The results array, or an error object; kept raw for forward
    /// compatibility.
    pub content: serde_json::Value,
}

/// The `from`/`to` target of a server-side model [`FallbackBlock`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackRef {
    /// The model id involved in the fallback.
    pub model: String,
}

/// A `fallback` block emitted when the server transparently switches models.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackBlock {
    /// The model the request originally targeted.
    pub from: FallbackRef,
    /// The model the server fell back to.
    pub to: FallbackRef,
}

tagged_enum! {
    /// A content block in an API **response** ([`crate::types::Message`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{ContentBlock, TextBlock};
    ///
    /// let json = serde_json::json!({"type": "text", "text": "Hello"});
    /// let block: ContentBlock = serde_json::from_value(json.clone()).unwrap();
    /// assert_eq!(block, ContentBlock::Text(TextBlock::new("Hello")));
    /// assert_eq!(serde_json::to_value(&block).unwrap(), json);
    ///
    /// // An unrecognised block type is preserved rather than rejected.
    /// let novel = serde_json::json!({"type": "brand_new", "foo": 1});
    /// let block: ContentBlock = serde_json::from_value(novel.clone()).unwrap();
    /// assert!(matches!(block, ContentBlock::Unknown(_)));
    /// assert_eq!(serde_json::to_value(&block).unwrap(), novel);
    /// ```
    pub enum ContentBlock {
        /// Model-generated text, optionally with citations.
        Text(TextBlock) = "text",
        /// A request to invoke a client-side tool.
        ToolUse(ToolUseBlock) = "tool_use",
        /// Extended reasoning output.
        Thinking(ThinkingBlock) = "thinking",
        /// Encrypted reasoning output.
        RedactedThinking(RedactedThinkingBlock) = "redacted_thinking",
        /// A request to invoke a server-side tool (e.g. web search).
        ServerToolUse(ToolUseBlock) = "server_tool_use",
        /// The result of a server-side web-search tool call.
        WebSearchToolResult(WebSearchToolResultBlock) = "web_search_tool_result",
        /// A server-side model fallback notification.
        Fallback(FallbackBlock) = "fallback",
    }
}

impl ContentBlock {
    /// Returns the text if this is a [`ContentBlock::Text`] block.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{ContentBlock, TextBlock};
    ///
    /// let block = ContentBlock::Text(TextBlock::new("hi"));
    /// assert_eq!(block.as_text(), Some("hi"));
    /// ```
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text(t) => Some(&t.text),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Request-side blocks.
// ---------------------------------------------------------------------------

/// A `text` block in a request, optionally carrying a cache breakpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextBlockParam {
    /// The text content.
    pub text: String,
    /// Optional cache breakpoint placed on this block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

impl TextBlockParam {
    /// Creates a request text block with no cache control.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::TextBlockParam;
    ///
    /// assert_eq!(TextBlockParam::new("hi").text, "hi");
    /// ```
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            cache_control: None,
        }
    }
}

/// An `image` block in a request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageBlockParam {
    /// Where the image bytes come from.
    pub source: ImageSource,
    /// Optional cache breakpoint placed on this block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// A `document` block in a request (e.g. a PDF for analysis).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentBlockParam {
    /// Where the document comes from.
    pub source: DocumentSource,
    /// Optional human-readable title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional context string passed alongside the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Whether citations are enabled for this document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfig>,
    /// Optional cache breakpoint placed on this block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Per-document citation configuration (`{"enabled": true}`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationsConfig {
    /// Whether the model may cite this document.
    pub enabled: bool,
}

/// A `tool_result` block returned to the model after running a tool.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolResultBlockParam {
    /// The id of the `tool_use` block this result answers.
    pub tool_use_id: String,
    /// The result payload: a string or an array of `text`/`image` blocks.
    pub content: ToolResultContent,
    /// Set to `true` when the tool errored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Optional cache breakpoint placed on this block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

impl ToolResultBlockParam {
    /// Creates a successful tool result.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ToolResultBlockParam;
    ///
    /// let r = ToolResultBlockParam::new("toolu_1", "72F and sunny");
    /// assert_eq!(r.tool_use_id, "toolu_1");
    /// assert_eq!(r.is_error, None);
    /// ```
    pub fn new(tool_use_id: impl Into<String>, content: impl Into<ToolResultContent>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: None,
            cache_control: None,
        }
    }

    /// Creates an errored tool result.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ToolResultBlockParam;
    ///
    /// let r = ToolResultBlockParam::error("toolu_1", "network unreachable");
    /// assert_eq!(r.is_error, Some(true));
    /// ```
    pub fn error(tool_use_id: impl Into<String>, content: impl Into<ToolResultContent>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: Some(true),
            cache_control: None,
        }
    }
}

tagged_enum! {
    /// A content block in an API **request** ([`crate::types::MessageParam`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{ContentBlockParam, TextBlockParam};
    ///
    /// let block = ContentBlockParam::text("Hello");
    /// assert_eq!(
    ///     serde_json::to_value(&block).unwrap(),
    ///     serde_json::json!({"type": "text", "text": "Hello"})
    /// );
    ///
    /// // Round-trips through the explicit variant too.
    /// let block = ContentBlockParam::Text(TextBlockParam::new("Hi"));
    /// let json = serde_json::to_value(&block).unwrap();
    /// assert_eq!(serde_json::from_value::<ContentBlockParam>(json).unwrap(), block);
    /// ```
    pub enum ContentBlockParam {
        /// Plain text, optionally cached.
        Text(TextBlockParam) = "text",
        /// An image by base64, URL, or file id.
        Image(ImageBlockParam) = "image",
        /// A document (e.g. PDF) by base64, URL, text, file id, or nested content.
        Document(DocumentBlockParam) = "document",
        /// An assistant tool call echoed back into the conversation.
        ToolUse(ToolUseBlock) = "tool_use",
        /// The result of running a tool, returned to the model.
        ToolResult(ToolResultBlockParam) = "tool_result",
        /// An assistant `thinking` block echoed back unchanged.
        Thinking(ThinkingBlock) = "thinking",
        /// An assistant `redacted_thinking` block echoed back unchanged.
        RedactedThinking(RedactedThinkingBlock) = "redacted_thinking",
    }
}

impl ContentBlockParam {
    /// Creates a `text` content block.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ContentBlockParam;
    ///
    /// let block = ContentBlockParam::text("Hello");
    /// assert!(matches!(block, ContentBlockParam::Text(_)));
    /// ```
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlockParam::Text(TextBlockParam::new(text))
    }

    /// Creates a `tool_result` content block.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ContentBlockParam;
    ///
    /// let block = ContentBlockParam::tool_result("toolu_1", "done");
    /// assert!(matches!(block, ContentBlockParam::ToolResult(_)));
    /// ```
    pub fn tool_result(
        tool_use_id: impl Into<String>,
        content: impl Into<ToolResultContent>,
    ) -> Self {
        ContentBlockParam::ToolResult(ToolResultBlockParam::new(tool_use_id, content))
    }
}

impl From<ContentBlock> for ContentBlockParam {
    /// Converts a response [`ContentBlock`] into the request
    /// [`ContentBlockParam`] used to echo an assistant turn back into a
    /// follow-up request (the wire-api tool-loop contract).
    ///
    /// Blocks with a direct request counterpart (`text`, `tool_use`,
    /// `thinking`, `redacted_thinking`) map to their typed variant; a
    /// [`TextBlock`]'s response-only `citations` are dropped because
    /// [`TextBlockParam`] has no such field. Response-only block types
    /// (`server_tool_use`, `web_search_tool_result`, `fallback`, and any
    /// [`ContentBlock::Unknown`]) are preserved verbatim as raw JSON in
    /// [`ContentBlockParam::Unknown`] so replayed history round-trips.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{ContentBlock, ContentBlockParam, ToolUseBlock};
    ///
    /// let call = ToolUseBlock {
    ///     id: "toolu_1".into(),
    ///     name: "get_weather".into(),
    ///     input: serde_json::json!({"location": "Paris"}),
    /// };
    /// let param = ContentBlockParam::from(ContentBlock::ToolUse(call.clone()));
    /// assert_eq!(param, ContentBlockParam::ToolUse(call));
    /// ```
    fn from(block: ContentBlock) -> Self {
        match block {
            ContentBlock::Text(text) => ContentBlockParam::Text(TextBlockParam {
                text: text.text,
                cache_control: None,
            }),
            ContentBlock::ToolUse(inner) => ContentBlockParam::ToolUse(inner),
            ContentBlock::Thinking(inner) => ContentBlockParam::Thinking(inner),
            ContentBlock::RedactedThinking(inner) => ContentBlockParam::RedactedThinking(inner),
            // Response-only block types have no typed request equivalent; preserve
            // them verbatim as raw JSON so replayed history is faithful.
            other => {
                let value = serde_json::to_value(&other).unwrap_or(serde_json::Value::Null);
                ContentBlockParam::Unknown(value)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Image / document sources.
// ---------------------------------------------------------------------------

/// A base64-encoded source (image or document bytes).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Base64Source {
    /// The MIME type, e.g. `image/png` or `application/pdf`.
    pub media_type: String,
    /// The base64-encoded bytes.
    pub data: String,
}

/// A source that references content by URL.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlSource {
    /// The URL to fetch.
    pub url: String,
}

/// A source that references a previously uploaded file by id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSource {
    /// The uploaded file's id.
    pub file_id: String,
}

/// A plain-text document source (`media_type: "text/plain"`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlainTextSource {
    /// The MIME type, typically `text/plain`.
    pub media_type: String,
    /// The raw text content.
    pub data: String,
}

/// A document source composed of nested content blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContentSource {
    /// The nested content blocks that make up the document.
    pub content: Vec<ContentBlockParam>,
}

tagged_enum! {
    /// The `source` of an [`ImageBlockParam`].
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{Base64Source, ImageSource};
    ///
    /// let src = ImageSource::Base64(Base64Source {
    ///     media_type: "image/png".into(),
    ///     data: "aGVsbG8=".into(),
    /// });
    /// let json = serde_json::to_value(&src).unwrap();
    /// assert_eq!(json["type"], "base64");
    /// assert_eq!(serde_json::from_value::<ImageSource>(json).unwrap(), src);
    /// ```
    pub enum ImageSource {
        /// Inline base64-encoded image bytes.
        Base64(Base64Source) = "base64",
        /// An image referenced by URL.
        Url(UrlSource) = "url",
        /// An image referenced by uploaded file id.
        File(FileSource) = "file",
    }
}

tagged_enum! {
    /// The `source` of a [`DocumentBlockParam`].
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{DocumentSource, UrlSource};
    ///
    /// let src = DocumentSource::Url(UrlSource { url: "https://example.com/a.pdf".into() });
    /// let json = serde_json::to_value(&src).unwrap();
    /// assert_eq!(json["type"], "url");
    /// assert_eq!(serde_json::from_value::<DocumentSource>(json).unwrap(), src);
    /// ```
    pub enum DocumentSource {
        /// Inline base64-encoded document bytes (e.g. a PDF).
        Base64(Base64Source) = "base64",
        /// Inline plain text.
        Text(PlainTextSource) = "text",
        /// A document assembled from nested content blocks.
        Content(ContentSource) = "content",
        /// A document referenced by URL.
        Url(UrlSource) = "url",
        /// A document referenced by uploaded file id.
        File(FileSource) = "file",
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

    // Fixtures: response content blocks (docs/wire-api.md "Content blocks — response").
    #[test]
    fn response_text_block() {
        let j = serde_json::json!({"type": "text", "text": "Hello"});
        assert_eq!(rt::<ContentBlock>(j.clone()), j);
    }

    #[test]
    fn response_tool_use_block() {
        let j = serde_json::json!({
            "type": "tool_use",
            "id": "toolu_01",
            "name": "get_weather",
            "input": {"location": "San Francisco"}
        });
        assert_eq!(rt::<ContentBlock>(j.clone()), j);
    }

    #[test]
    fn response_thinking_block() {
        let j = serde_json::json!({
            "type": "thinking",
            "thinking": "Let me reason about this.",
            "signature": "c2ln"
        });
        assert_eq!(rt::<ContentBlock>(j.clone()), j);
    }

    #[test]
    fn response_redacted_thinking_block() {
        let j = serde_json::json!({"type": "redacted_thinking", "data": "encrypted"});
        assert_eq!(rt::<ContentBlock>(j.clone()), j);
    }

    #[test]
    fn response_server_tool_use_block() {
        let j = serde_json::json!({
            "type": "server_tool_use",
            "id": "srvtoolu_1",
            "name": "web_search",
            "input": {"query": "rust sdk"}
        });
        assert_eq!(rt::<ContentBlock>(j.clone()), j);
    }

    #[test]
    fn response_web_search_tool_result_block() {
        let j = serde_json::json!({
            "type": "web_search_tool_result",
            "tool_use_id": "srvtoolu_1",
            "content": [{"type": "web_search_result", "title": "x", "url": "https://x"}]
        });
        assert_eq!(rt::<ContentBlock>(j.clone()), j);
    }

    #[test]
    fn response_fallback_block() {
        let j = serde_json::json!({
            "type": "fallback",
            "from": {"model": "claude-fable-5"},
            "to": {"model": "claude-opus-4-8"}
        });
        assert_eq!(rt::<ContentBlock>(j.clone()), j);
    }

    // Fixture: unknown block type must round-trip verbatim, never error.
    #[test]
    fn unknown_response_block_is_preserved() {
        let j = serde_json::json!({"type": "brand_new_block", "foo": [1, 2, 3], "bar": "x"});
        let parsed: ContentBlock = serde_json::from_value(j.clone()).unwrap();
        assert!(matches!(parsed, ContentBlock::Unknown(_)));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), j);
    }

    #[test]
    fn object_without_type_falls_through_to_unknown() {
        let j = serde_json::json!({"no_type_here": true});
        let parsed: ContentBlock = serde_json::from_value(j.clone()).unwrap();
        assert!(matches!(parsed, ContentBlock::Unknown(_)));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), j);
    }

    // Fixtures: request content blocks (docs/wire-api.md "Content blocks — request").
    #[test]
    fn request_text_block_with_cache_control() {
        let j = serde_json::json!({
            "type": "text",
            "text": "cache me",
            "cache_control": {"type": "ephemeral"}
        });
        assert_eq!(rt::<ContentBlockParam>(j.clone()), j);
    }

    #[test]
    fn request_image_base64_block() {
        let j = serde_json::json!({
            "type": "image",
            "source": {"type": "base64", "media_type": "image/png", "data": "aGVsbG8="}
        });
        assert_eq!(rt::<ContentBlockParam>(j.clone()), j);
    }

    #[test]
    fn request_document_url_block() {
        let j = serde_json::json!({
            "type": "document",
            "source": {"type": "url", "url": "https://example.com/a.pdf"},
            "title": "A PDF",
            "citations": {"enabled": true}
        });
        assert_eq!(rt::<ContentBlockParam>(j.clone()), j);
    }

    #[test]
    fn request_document_content_block() {
        let j = serde_json::json!({
            "type": "document",
            "source": {"type": "content", "content": [{"type": "text", "text": "nested"}]}
        });
        assert_eq!(rt::<ContentBlockParam>(j.clone()), j);
    }

    #[test]
    fn request_tool_result_string_block() {
        let j = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "toolu_01",
            "content": "72F and sunny"
        });
        assert_eq!(rt::<ContentBlockParam>(j.clone()), j);
    }

    #[test]
    fn request_tool_result_blocks_with_error() {
        let j = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "toolu_01",
            "content": [{"type": "text", "text": "boom"}],
            "is_error": true
        });
        assert_eq!(rt::<ContentBlockParam>(j.clone()), j);
    }

    // Regression: converting a response block to a request block maps typed
    // variants, drops response-only citations, and preserves response-only block
    // types verbatim as raw JSON.
    #[test]
    fn content_block_into_param_maps_and_preserves() {
        // text: citations (response-only) are dropped.
        let text = ContentBlock::Text(TextBlock {
            text: "hi".into(),
            citations: Some(vec![serde_json::json!({"cited_text": "x"})]),
        });
        assert_eq!(
            ContentBlockParam::from(text),
            ContentBlockParam::Text(TextBlockParam::new("hi"))
        );

        // tool_use: maps to the identical request variant.
        let call = ToolUseBlock {
            id: "toolu_1".into(),
            name: "get_weather".into(),
            input: serde_json::json!({"location": "Paris"}),
        };
        assert_eq!(
            ContentBlockParam::from(ContentBlock::ToolUse(call.clone())),
            ContentBlockParam::ToolUse(call)
        );

        // server_tool_use has no request variant: preserved verbatim as Unknown.
        let stu = ContentBlock::ServerToolUse(ToolUseBlock {
            id: "srvtoolu_1".into(),
            name: "web_search".into(),
            input: serde_json::json!({"query": "rust"}),
        });
        match ContentBlockParam::from(stu) {
            ContentBlockParam::Unknown(value) => {
                assert_eq!(value["type"], serde_json::json!("server_tool_use"));
                assert_eq!(value["name"], serde_json::json!("web_search"));
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn unknown_request_block_is_preserved() {
        // Server-tool blocks replayed into history pass through untouched.
        let j = serde_json::json!({
            "type": "server_tool_use",
            "id": "srvtoolu_1",
            "name": "web_search",
            "input": {"query": "x"}
        });
        let parsed: ContentBlockParam = serde_json::from_value(j.clone()).unwrap();
        assert!(matches!(parsed, ContentBlockParam::Unknown(_)));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), j);
    }
}
