//! The Messages endpoint: [`MessagesRequest`] and its builder, message
//! creation, and token counting.

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::{Error, Result};
use crate::streaming::MessageStream;
use crate::types::{
    CacheControl, Message, MessageParam, Metadata, OutputConfig, SystemPrompt, ThinkingConfig,
    ToolChoice, ToolUnion,
};

/// Merges the entries of `extra` into a JSON object `value`, leaving `value`
/// untouched if it is not an object or `extra` is empty.
fn merge_extra(value: &mut serde_json::Value, extra: &serde_json::Map<String, serde_json::Value>) {
    if extra.is_empty() {
        return;
    }
    if let serde_json::Value::Object(map) = value {
        for (key, entry) in extra {
            map.insert(key.clone(), entry.clone());
        }
    }
}

/// A request body for `POST /v1/messages`.
///
/// Build one with [`MessagesRequest::builder`]. The `betas` field is sent as the
/// `anthropic-beta` header (not in the JSON body), and `extra_body` is merged
/// into the serialized body so new top-level fields can be used without an SDK
/// release.
///
/// # Examples
///
/// ```
/// use crimson_crab::api::MessagesRequest;
/// use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
/// use crimson_crab::types::MessageParam;
///
/// let request = MessagesRequest::builder()
///     .model(CLAUDE_OPUS_4_8)
///     .max_tokens(1024)
///     .messages(vec![MessageParam::user("Hello")])
///     .build()
///     .unwrap();
/// assert_eq!(request.model, "claude-opus-4-8");
/// assert_eq!(request.max_tokens, 1024);
/// ```
///
/// This type is `#[non_exhaustive]`: construct it through
/// [`MessagesRequest::builder`] so that fields promoted out of `extra_body` into
/// typed fields in a future minor release do not break your build.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct MessagesRequest {
    /// The model id to use (an open string; see [`crate::model_ids`]).
    pub model: String,
    /// The maximum number of tokens to generate.
    pub max_tokens: u32,
    /// The conversation so far; the first message must be from the user.
    pub messages: Vec<MessageParam>,
    /// An optional system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    /// Optional request metadata (e.g. an end-user id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Custom stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Whether to stream the response (left unset by [`Messages::create`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Extended-thinking configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Output configuration (effort and structured-output format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    /// The tools the model may call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolUnion>>,
    /// How the model should choose among the tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// A top-level cache breakpoint (auto-placed on the last cacheable block).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// Sampling temperature (rejected on some newer models — do not default it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Nucleus-sampling `top_p` (rejected on some newer models).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Top-k sampling (rejected on some newer models).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// A code-execution container id to reuse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// Beta flags sent via the `anthropic-beta` header (never in the body).
    #[serde(skip)]
    pub betas: Vec<String>,
    /// Extra top-level fields merged into the serialized body.
    #[serde(skip)]
    pub extra_body: serde_json::Map<String, serde_json::Value>,
}

impl MessagesRequest {
    /// Starts building a request.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::api::MessagesRequest;
    /// let builder = MessagesRequest::builder();
    /// let _ = builder.model("claude-opus-4-8");
    /// ```
    pub fn builder() -> MessagesRequestBuilder {
        MessagesRequestBuilder::default()
    }

    /// Serializes the request to a JSON body with `extra_body` merged in.
    pub(crate) fn to_json_body(&self) -> Result<serde_json::Value> {
        let mut value = serde_json::to_value(self)?;
        merge_extra(&mut value, &self.extra_body);
        Ok(value)
    }

    /// Derives a [`CountTokensRequest`] from this request.
    ///
    /// Copies `model`, `messages`, `system`, `tools`, `thinking`, `betas`, and
    /// `extra_body` (the fields `/v1/messages/count_tokens` accepts). `max_tokens`
    /// is intentionally omitted.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::api::MessagesRequest;
    /// use crimson_crab::types::MessageParam;
    ///
    /// let request = MessagesRequest::builder()
    ///     .model("claude-opus-4-8")
    ///     .max_tokens(1024)
    ///     .messages(vec![MessageParam::user("Hi")])
    ///     .build()
    ///     .unwrap();
    /// let count = request.as_count_request();
    /// assert_eq!(count.model, "claude-opus-4-8");
    /// ```
    pub fn as_count_request(&self) -> CountTokensRequest {
        CountTokensRequest {
            model: self.model.clone(),
            messages: self.messages.clone(),
            system: self.system.clone(),
            tools: self.tools.clone(),
            thinking: self.thinking.clone(),
            betas: self.betas.clone(),
            extra_body: self.extra_body.clone(),
        }
    }
}

/// A hand-rolled builder for [`MessagesRequest`].
///
/// `model`, `max_tokens`, and at least one message are required; [`build`]
/// returns [`Error::Config`] if any are missing.
///
/// [`build`]: MessagesRequestBuilder::build
///
/// # Examples
///
/// ```
/// use crimson_crab::api::MessagesRequest;
/// use crimson_crab::types::{MessageParam, ThinkingConfig};
///
/// let request = MessagesRequest::builder()
///     .model("claude-opus-4-8")
///     .max_tokens(2048)
///     .system("You are terse.")
///     .messages(vec![MessageParam::user("Hello")])
///     .thinking(ThinkingConfig::adaptive())
///     .beta("fast-mode-2026-02-01")
///     .extra_field("speed", serde_json::json!("fast"))
///     .build()
///     .unwrap();
/// assert_eq!(request.betas, vec!["fast-mode-2026-02-01".to_string()]);
/// assert_eq!(request.extra_body["speed"], serde_json::json!("fast"));
/// ```
#[derive(Clone, Debug, Default)]
pub struct MessagesRequestBuilder {
    model: Option<String>,
    max_tokens: Option<u32>,
    messages: Option<Vec<MessageParam>>,
    system: Option<SystemPrompt>,
    metadata: Option<Metadata>,
    stop_sequences: Option<Vec<String>>,
    stream: Option<bool>,
    thinking: Option<ThinkingConfig>,
    output_config: Option<OutputConfig>,
    tools: Option<Vec<ToolUnion>>,
    tool_choice: Option<ToolChoice>,
    cache_control: Option<CacheControl>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    top_k: Option<u32>,
    container: Option<String>,
    betas: Vec<String>,
    extra_body: serde_json::Map<String, serde_json::Value>,
}

impl MessagesRequestBuilder {
    /// Sets the model id. Required.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Sets `max_tokens`. Required.
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Replaces the whole message list. Required (at least one message).
    pub fn messages(mut self, messages: Vec<MessageParam>) -> Self {
        self.messages = Some(messages);
        self
    }

    /// Appends a single message.
    pub fn message(mut self, message: MessageParam) -> Self {
        self.messages.get_or_insert_with(Vec::new).push(message);
        self
    }

    /// Sets the system prompt.
    pub fn system(mut self, system: impl Into<SystemPrompt>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Sets request metadata.
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Sets the stop sequences.
    pub fn stop_sequences(mut self, stop_sequences: Vec<String>) -> Self {
        self.stop_sequences = Some(stop_sequences);
        self
    }

    /// Sets the `stream` flag (normally left unset for [`Messages::create`]).
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = Some(stream);
        self
    }

    /// Sets the extended-thinking configuration.
    pub fn thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.thinking = Some(thinking);
        self
    }

    /// Sets the output configuration.
    pub fn output_config(mut self, output_config: OutputConfig) -> Self {
        self.output_config = Some(output_config);
        self
    }

    /// Replaces the whole tool list.
    pub fn tools(mut self, tools: Vec<ToolUnion>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Appends a single tool.
    pub fn tool(mut self, tool: impl Into<ToolUnion>) -> Self {
        self.tools.get_or_insert_with(Vec::new).push(tool.into());
        self
    }

    /// Sets the tool-choice strategy.
    pub fn tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    /// Sets a top-level cache breakpoint.
    pub fn cache_control(mut self, cache_control: CacheControl) -> Self {
        self.cache_control = Some(cache_control);
        self
    }

    /// Sets the sampling temperature.
    pub fn temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Sets nucleus-sampling `top_p`.
    pub fn top_p(mut self, top_p: f64) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// Sets top-k sampling.
    pub fn top_k(mut self, top_k: u32) -> Self {
        self.top_k = Some(top_k);
        self
    }

    /// Sets the code-execution container id to reuse.
    pub fn container(mut self, container: impl Into<String>) -> Self {
        self.container = Some(container.into());
        self
    }

    /// Replaces the whole beta-flag list (sent via `anthropic-beta`).
    pub fn betas<I, S>(mut self, betas: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.betas = betas.into_iter().map(Into::into).collect();
        self
    }

    /// Appends a single beta flag.
    pub fn beta(mut self, beta: impl Into<String>) -> Self {
        self.betas.push(beta.into());
        self
    }

    /// Replaces the whole `extra_body` map.
    pub fn extra_body(mut self, extra_body: serde_json::Map<String, serde_json::Value>) -> Self {
        self.extra_body = extra_body;
        self
    }

    /// Sets a single extra top-level body field.
    pub fn extra_field(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.extra_body.insert(key.into(), value);
        self
    }

    /// Validates the required fields and builds a [`MessagesRequest`].
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::api::MessagesRequest;
    /// assert!(MessagesRequest::builder().build().is_err()); // no model
    /// ```
    pub fn build(self) -> Result<MessagesRequest> {
        let model = self
            .model
            .filter(|model| !model.is_empty())
            .ok_or_else(|| Error::Config("`model` is required".to_string()))?;
        let max_tokens = self
            .max_tokens
            .ok_or_else(|| Error::Config("`max_tokens` is required".to_string()))?;
        let messages = self
            .messages
            .filter(|messages| !messages.is_empty())
            .ok_or_else(|| Error::Config("at least one message is required".to_string()))?;
        Ok(MessagesRequest {
            model,
            max_tokens,
            messages,
            system: self.system,
            metadata: self.metadata,
            stop_sequences: self.stop_sequences,
            stream: self.stream,
            thinking: self.thinking,
            output_config: self.output_config,
            tools: self.tools,
            tool_choice: self.tool_choice,
            cache_control: self.cache_control,
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            container: self.container,
            betas: self.betas,
            extra_body: self.extra_body,
        })
    }
}

/// A request body for `POST /v1/messages/count_tokens`.
///
/// Same prompt-shaping fields as [`MessagesRequest`] but without `max_tokens`.
///
/// # Examples
///
/// ```
/// use crimson_crab::api::CountTokensRequest;
/// use crimson_crab::types::MessageParam;
///
/// let request = CountTokensRequest::new("claude-opus-4-8", vec![MessageParam::user("Hi")]);
/// assert_eq!(request.model, "claude-opus-4-8");
/// ```
#[derive(Clone, Debug, Serialize)]
pub struct CountTokensRequest {
    /// The model id to count against (counts are model-specific).
    pub model: String,
    /// The conversation to count.
    pub messages: Vec<MessageParam>,
    /// An optional system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    /// The tools whose definitions should be counted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolUnion>>,
    /// Extended-thinking configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Beta flags sent via the `anthropic-beta` header (never in the body).
    #[serde(skip)]
    pub betas: Vec<String>,
    /// Extra top-level fields merged into the serialized body.
    #[serde(skip)]
    pub extra_body: serde_json::Map<String, serde_json::Value>,
}

impl CountTokensRequest {
    /// Creates a request from a model id and message list.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::api::CountTokensRequest;
    /// use crimson_crab::types::MessageParam;
    ///
    /// let request = CountTokensRequest::new("claude-opus-4-8", vec![MessageParam::user("Hi")]);
    /// assert_eq!(request.messages.len(), 1);
    /// ```
    pub fn new(model: impl Into<String>, messages: Vec<MessageParam>) -> Self {
        Self {
            model: model.into(),
            messages,
            system: None,
            tools: None,
            thinking: None,
            betas: Vec::new(),
            extra_body: serde_json::Map::new(),
        }
    }

    /// Serializes the request to a JSON body with `extra_body` merged in.
    pub(crate) fn to_json_body(&self) -> Result<serde_json::Value> {
        let mut value = serde_json::to_value(self)?;
        merge_extra(&mut value, &self.extra_body);
        Ok(value)
    }
}

/// The response from `POST /v1/messages/count_tokens`.
///
/// # Examples
///
/// ```
/// use crimson_crab::api::CountTokensResponse;
///
/// let response: CountTokensResponse =
///     serde_json::from_value(serde_json::json!({"input_tokens": 2095})).unwrap();
/// assert_eq!(response.input_tokens, 2095);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CountTokensResponse {
    /// The number of input tokens the request would consume.
    pub input_tokens: u64,
    /// Forward-compatible catch-all for any other response fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A handle to the Messages endpoint, obtained via [`Client::messages`].
///
/// [`Client::messages`]: crate::Client::messages
///
/// # Examples
///
/// ```no_run
/// use crimson_crab::api::MessagesRequest;
/// use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
/// use crimson_crab::types::MessageParam;
///
/// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
/// let request = MessagesRequest::builder()
///     .model(CLAUDE_OPUS_4_8)
///     .max_tokens(1024)
///     .messages(vec![MessageParam::user("Hello")])
///     .build()?;
/// let message = client.messages().create(&request).await?;
/// println!("{}", message.text());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Messages<'a> {
    client: &'a Client,
}

impl<'a> Messages<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Creates a message (`POST /v1/messages`) and returns the [`Message`].
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
    /// let message = client.messages().create(&request).await?;
    /// let _ = message.stop_reason;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(&self, request: &MessagesRequest) -> Result<Message> {
        // A `stream: true` request returns SSE, which `create` cannot parse as a
        // single JSON message. Reject it with a clear error instead of failing
        // later with a confusing deserialization error.
        if request.stream == Some(true) {
            return Err(Error::Config(
                "`stream` is set to `true`; call `messages().stream(&request)` instead of \
                 `create()`"
                    .to_string(),
            ));
        }
        let body = request.to_json_body()?;
        self.client
            .http()
            .post_json("/v1/messages", &body, &request.betas)
            .await
    }

    /// Streams a message (`POST /v1/messages` with `"stream": true`).
    ///
    /// The returned [`MessageStream`] is a
    /// [`Stream`](futures_core::Stream) of [`StreamEvent`](crate::streaming::StreamEvent)s
    /// that also accumulates a final [`Message`] (see
    /// [`MessageStream::collect_final`]). The `stream` field is set
    /// automatically, so it need not be set on the request. Retries happen only
    /// before the first byte of the response body is received.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crimson_crab::api::MessagesRequest;
    /// use crimson_crab::types::MessageParam;
    /// use futures_util::StreamExt;
    ///
    /// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
    /// let request = MessagesRequest::builder()
    ///     .model("claude-opus-4-8")
    ///     .max_tokens(1024)
    ///     .messages(vec![MessageParam::user("Hi")])
    ///     .build()?;
    /// let mut stream = client.messages().stream(&request).await?;
    /// while let Some(event) = stream.next().await {
    ///     let _event = event?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn stream(&self, request: &MessagesRequest) -> Result<MessageStream> {
        let mut body = request.to_json_body()?;
        if let serde_json::Value::Object(map) = &mut body {
            map.insert("stream".to_string(), serde_json::Value::Bool(true));
        }
        let response = self
            .client
            .http()
            .post_raw("/v1/messages", &body, &request.betas)
            .await?;
        Ok(MessageStream::new(response))
    }

    /// Counts the tokens a request would consume
    /// (`POST /v1/messages/count_tokens`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crimson_crab::api::CountTokensRequest;
    /// use crimson_crab::types::MessageParam;
    ///
    /// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
    /// let request = CountTokensRequest::new("claude-opus-4-8", vec![MessageParam::user("Hi")]);
    /// let count = client.messages().count_tokens(&request).await?;
    /// println!("{}", count.input_tokens);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn count_tokens(&self, request: &CountTokensRequest) -> Result<CountTokensResponse> {
        let body = request.to_json_body()?;
        self.client
            .http()
            .post_json("/v1/messages/count_tokens", &body, &request.betas)
            .await
    }
}
