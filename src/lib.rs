//! # crimson-crab
//!
//! The production-grade Rust SDK for Anthropic's Claude API.
//!
//! This crate mirrors the Claude wire API exactly (see the type modules under
//! [`types`]) and is designed to be **forward compatible**: content blocks,
//! stop reasons, and other type-tagged or string-valued enums all carry a
//! catch-all variant, so values the SDK has never seen deserialize instead of
//! erroring.
//!
//! > crimson-crab is an independent open-source project and is not affiliated
//! > with Anthropic.
//!
//! ## Status
//!
//! The wire types, the HTTP [`Client`], the Messages, Models, and Batches
//! endpoints, and SSE [`streaming`] (`client.messages().stream(&req)`) are all
//! implemented.
//!
//! ## Quickstart
//!
//! ```no_run
//! use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
//! use crimson_crab::prelude::*;
//!
//! # #[tokio::main]
//! # async fn main() -> crimson_crab::Result<()> {
//! // Reads the API key from the ANTHROPIC_API_KEY environment variable.
//! let client = Client::from_env()?;
//!
//! let request = MessagesRequest::builder()
//!     .model(CLAUDE_OPUS_4_8)
//!     .max_tokens(1024)
//!     .messages(vec![MessageParam::user("Hello, Claude!")])
//!     .build()?;
//!
//! let message = client.messages().create(&request).await?;
//! println!("{}", message.text());
//! # Ok(())
//! # }
//! ```
//!
//! ## Building request values
//!
//! ```
//! use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
//! use crimson_crab::prelude::*;
//!
//! // A conversation turn.
//! let messages = vec![MessageParam::user("What is the weather in Paris?")];
//!
//! // A custom tool the model may call.
//! let tool = Tool::new(
//!     "get_weather",
//!     "Get the current weather for a location",
//!     serde_json::json!({
//!         "type": "object",
//!         "properties": {"location": {"type": "string"}},
//!         "required": ["location"]
//!     }),
//! );
//!
//! assert_eq!(CLAUDE_OPUS_4_8, "claude-opus-4-8");
//! assert_eq!(messages[0].role, Role::User);
//! assert_eq!(tool.name, "get_weather");
//! ```
//!
//! ## Parsing a response
//!
//! ```
//! use crimson_crab::prelude::*;
//!
//! let body = serde_json::json!({
//!     "id": "msg_01ABC",
//!     "type": "message",
//!     "role": "assistant",
//!     "model": "claude-opus-4-8",
//!     "content": [{"type": "text", "text": "It is sunny."}],
//!     "stop_reason": "end_turn",
//!     "stop_sequence": null,
//!     "usage": {"input_tokens": 12, "output_tokens": 4}
//! });
//! let msg: Message = serde_json::from_value(body).unwrap();
//! assert_eq!(msg.text(), "It is sunny.");
//! assert_eq!(msg.stop_reason, Some(StopReason::EndTurn));
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo)
)]
#![deny(missing_docs)]

// The HTTP client speaks HTTPS to `https://api.anthropic.com`, which requires a
// TLS backend on native targets. Fail the build early (rather than at runtime
// with an opaque connector error) if default features were disabled without
// selecting one. `wasm32` is exempt: there the browser's `fetch` provides TLS.
#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(feature = "rustls-tls", feature = "native-tls"))
))]
compile_error!(
    "crimson-crab requires a TLS backend on native targets: enable the \
     `rustls-tls` (default) or `native-tls` feature."
);

pub mod api;
pub mod client;
pub mod error;
mod http;
pub mod model_ids;
pub mod streaming;
pub mod types;

pub use client::{Client, ClientBuilder};
pub use error::{ApiError, Error, Result};
pub use streaming::{ContentDelta, MessageStream, StreamEvent};

/// Commonly used types, re-exported for `use crimson_crab::prelude::*;`.
///
/// # Examples
///
/// ```
/// use crimson_crab::prelude::*;
///
/// let _ = MessageParam::user("hi");
/// let _ = ToolChoice::auto();
/// let _ = ThinkingConfig::adaptive();
/// let _ = MessagesRequest::builder().model("claude-opus-4-8");
/// ```
pub mod prelude {
    pub use crate::api::{CountTokensRequest, CountTokensResponse, MessagesRequest};
    pub use crate::client::{Client, ClientBuilder};
    // `Error` and the `Result<T>` alias are deliberately NOT re-exported here:
    // a glob import that shadows `std::result::Result` breaks common downstream
    // patterns like `Result<Event, E>`. Reach them as `crimson_crab::Error` /
    // `crimson_crab::Result` instead.
    pub use crate::error::ApiError;
    pub use crate::streaming::{ContentDelta, MessageStream, StreamEvent};
    pub use crate::types::cache::{CacheControl, CacheTtl};
    pub use crate::types::content::{ContentBlock, ContentBlockParam};
    pub use crate::types::message::{
        Message, MessageContent, MessageParam, Metadata, Role, StopDetails, StopReason,
        SystemPrompt, Usage,
    };
    pub use crate::types::output::{Effort, OutputConfig, OutputFormat};
    pub use crate::types::thinking::{ThinkingConfig, ThinkingDisplay};
    pub use crate::types::tool::{Tool, ToolChoice, ToolResultContent, ToolUnion};
    // Tool-loop types (the crate's most common use case) belong in the prelude.
    pub use crate::types::content::{ToolResultBlockParam, ToolUseBlock};
}

/// Compiles the `README.md` code samples as part of `cargo test --doc` so the
/// crate's most-read snippets (quickstart, streaming, tool loop) cannot silently
/// drift from the public API. The item exists only during doctest builds.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
