//! The tool runner: a driver for the agentic
//! `create → run tools → feed results → repeat` loop.
//!
//! [`ToolRunner`] is the batteries-included counterpart to the manual loop
//! shown in `examples/tool_use.rs`. You register a [`Tool`] together with the
//! async closure that implements it; the runner sends the request, executes
//! every `tool_use` block the model emits, appends the assistant turn and a
//! single user message of `tool_result` blocks, and goes round again until the
//! model stops asking for tools (or a turn cap is hit).
//!
//! The runner deliberately stays *low level about outcomes*: it never inspects
//! the final [`StopReason`] for you. A refusal, a `max_tokens` truncation, or a
//! natural end all return normally, and the caller decides what they mean.
//!
//! Nothing here is feature-gated: [`ToolRunner::tool`] accepts any [`Tool`]
//! value, whether its `input_schema` was hand-written with [`Tool::new`] or
//! derived with `Tool::from_type` on the `schemars` feature.

use std::collections::HashMap;
use std::fmt::Display;
use std::future::Future;

use futures_util::future::{join_all, BoxFuture};

use crate::api::messages::{Messages, MessagesRequest};
use crate::error::{Error, Result};
use crate::types::{
    ContentBlock, ContentBlockParam, Message, MessageParam, StopReason, Tool, ToolResultBlockParam,
    ToolUnion, ToolUseBlock,
};

/// The number of API round-trips a runner makes before giving up, unless
/// [`ToolRunner::max_turns`] says otherwise.
const DEFAULT_MAX_TURNS: usize = 10;

/// The type-erased form every registered handler is stored as: the model's raw
/// `input` in, a boxed future of "JSON result or error text" out.
///
/// The `Err(String)` channel carries everything the model should see as a
/// failed tool call — a handler's own error, an input that did not fit the
/// handler's argument type, or a call to a tool nobody registered — because all
/// three are reported the same way on the wire: a `tool_result` block with
/// `is_error: true`.
type ErasedHandler = Box<
    dyn Fn(serde_json::Value) -> BoxFuture<'static, std::result::Result<serde_json::Value, String>>
        + Send
        + Sync,
>;

/// The boxed per-turn observer installed by [`ToolRunner::on_turn`]. It borrows
/// for `'a` — the same lifetime as the client the runner works through — so it
/// may capture state owned by the caller's stack frame.
type TurnHook<'a> = Box<dyn FnMut(&Message) + Send + 'a>;

/// The outcome of a completed [`ToolRunner::run`].
///
/// `message` is the response that ended the loop — check its
/// [`stop_reason`](Message::stop_reason) to see *why* it ended. `messages` is
/// the whole conversation, ready to be handed to another request (or another
/// runner) to continue where this one left off, and `turns` is how many API
/// round-trips it took.
///
/// # Examples
///
/// ```
/// use crimson_crab::api::{MessagesRequest, ToolRunResult};
///
/// fn continue_conversation(
///     result: ToolRunResult,
/// ) -> crimson_crab::Result<MessagesRequest> {
///     println!("{} after {} turns", result.message.text(), result.turns);
///     // The history already ends with the final assistant turn, so a follow-up
///     // question is one `push` away.
///     let mut messages = result.messages;
///     messages.push(crimson_crab::types::MessageParam::user("And tomorrow?"));
///     MessagesRequest::builder()
///         .model("claude-opus-5")
///         .max_tokens(1024)
///         .messages(messages)
///         .build()
/// }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ToolRunResult {
    /// The final response: the first one whose `stop_reason` was not
    /// [`StopReason::ToolUse`].
    pub message: Message,
    /// The full conversation, including the messages the runner appended: every
    /// assistant turn, every user message of `tool_result` blocks, and the
    /// final assistant turn (so this can be fed straight into a follow-up
    /// request without re-appending [`message`](ToolRunResult::message)).
    pub messages: Vec<MessageParam>,
    /// The number of API round-trips the run took (always at least 1).
    pub turns: usize,
}

/// A driver for the agentic tool loop, created by [`Messages::runner`].
///
/// Register tools with [`tool`](ToolRunner::tool) (typed) or
/// [`tool_raw`](ToolRunner::tool_raw) (untyped), optionally set a turn cap and
/// a per-turn observer, then call [`run`](ToolRunner::run).
///
/// Registering a tool also **appends it to the request's `tools` list**, so a
/// tool is described to the model exactly once, in the same place its handler
/// is defined. Registering the same name twice replaces both the handler and
/// the tool definition rather than sending a duplicate.
///
/// # Examples
///
/// ```no_run
/// use crimson_crab::api::MessagesRequest;
/// use crimson_crab::types::{MessageParam, Tool};
///
/// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
/// let request = MessagesRequest::builder()
///     .model("claude-opus-5")
///     .max_tokens(1024)
///     .messages(vec![MessageParam::user("What's the weather in Paris?")])
///     .build()?;
///
/// let weather = Tool::new(
///     "get_weather",
///     "Get the current weather for a location",
///     serde_json::json!({
///         "type": "object",
///         "properties": {"location": {"type": "string"}},
///         "required": ["location"]
///     }),
/// );
///
/// let result = client
///     .messages()
///     .runner(request)
///     .tool_raw(weather, |input: serde_json::Value| async move {
///         let location = input["location"].as_str().unwrap_or("?").to_string();
///         Ok::<_, String>(format!("18C and raining in {location}"))
///     })
///     .max_turns(6)
///     .run()
///     .await?;
///
/// println!("{} ({} turns)", result.message.text(), result.turns);
/// # Ok(())
/// # }
/// ```
pub struct ToolRunner<'a> {
    messages: Messages<'a>,
    request: MessagesRequest,
    handlers: HashMap<String, ErasedHandler>,
    /// Registered names in registration order, so the "unknown tool" message
    /// the model sees is stable rather than hash-ordered.
    names: Vec<String>,
    max_turns: usize,
    on_turn: Option<TurnHook<'a>>,
}

impl std::fmt::Debug for ToolRunner<'_> {
    /// Prints the runner's configuration. Handlers are opaque closures, so they
    /// are represented by their registered names.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRunner")
            .field("request", &self.request)
            .field("tools", &self.names)
            .field("max_turns", &self.max_turns)
            .field("on_turn", &self.on_turn.is_some())
            .finish()
    }
}

impl<'a> ToolRunner<'a> {
    /// Creates a runner over `request`, which the runner owns and grows as the
    /// conversation proceeds.
    pub(crate) fn new(messages: Messages<'a>, request: MessagesRequest) -> Self {
        Self {
            messages,
            request,
            handlers: HashMap::new(),
            names: Vec::new(),
            max_turns: DEFAULT_MAX_TURNS,
            on_turn: None,
        }
    }

    /// Registers `tool` along with a **typed** handler for it.
    ///
    /// The tool is appended to the request's `tools` list — there is no need to
    /// also set it on the request you passed to [`Messages::runner`], and a
    /// same-named definition already there is replaced rather than duplicated.
    /// The handler receives the model's `input` deserialized into `T` and
    /// returns anything serializable; a `Result::Err` is shown to the model
    /// rather than aborting the run (see [`run`](ToolRunner::run)).
    ///
    /// Two failures are reported to the model as an errored `tool_result` and
    /// leave the loop running, because the model can recover from both by
    /// calling again with better arguments:
    ///
    /// * the `input` does not deserialize into `T` (the message names `T` and
    ///   quotes the `serde_json` error);
    /// * the handler returns `Err(e)` (the message is `e`'s [`Display`] text).
    ///
    /// This method is **not** gated on the `schemars` feature: it takes any
    /// [`Tool`], however its schema was built.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crimson_crab::api::MessagesRequest;
    /// use crimson_crab::types::{MessageParam, Tool};
    /// use serde::Deserialize;
    ///
    /// /// The arguments of the weather tool.
    /// #[derive(Deserialize)]
    /// struct GetWeather {
    ///     location: String,
    /// }
    ///
    /// # async fn demo(client: &crimson_crab::Client, request: MessagesRequest, tool: Tool)
    /// # -> crimson_crab::Result<()> {
    /// let result = client
    ///     .messages()
    ///     .runner(request)
    ///     .tool(tool, |args: GetWeather| async move {
    ///         Ok::<_, String>(format!("22C in {}", args.location))
    ///     })
    ///     .run()
    ///     .await?;
    /// # let _ = result;
    /// # Ok(())
    /// # }
    /// ```
    pub fn tool<T, R, E, F, Fut>(self, tool: Tool, handler: F) -> Self
    where
        T: serde::de::DeserializeOwned,
        R: serde::Serialize,
        E: Display,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = std::result::Result<R, E>> + Send + 'static,
    {
        let erased: ErasedHandler = Box::new(move |input| {
            // Deserialize eagerly, outside the future: a bad input is the
            // model's mistake to fix, not a reason to fail the run.
            match serde_json::from_value::<T>(input) {
                Ok(args) => {
                    let call = handler(args);
                    Box::pin(
                        async move { call.await.map_err(|err| err.to_string()).and_then(to_json) },
                    )
                }
                Err(err) => {
                    let message = format!(
                        "the tool input did not deserialize into `{}`: {err}",
                        std::any::type_name::<T>()
                    );
                    Box::pin(async move { Err(message) })
                }
            }
        });
        self.register(tool, erased)
    }

    /// Registers `tool` along with an **untyped** handler that receives the
    /// model's `input` as a raw [`serde_json::Value`].
    ///
    /// The escape hatch for inputs that are not worth a struct, or whose shape
    /// is only known at runtime. Everything else matches
    /// [`tool`](ToolRunner::tool): the tool is appended to the request, and a
    /// handler `Err` becomes an errored `tool_result` instead of ending the run.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use crimson_crab::api::MessagesRequest;
    /// # use crimson_crab::types::Tool;
    /// # async fn demo(client: &crimson_crab::Client, request: MessagesRequest, tool: Tool)
    /// # -> crimson_crab::Result<()> {
    /// let result = client
    ///     .messages()
    ///     .runner(request)
    ///     .tool_raw(tool, |input: serde_json::Value| async move {
    ///         Ok::<_, String>(serde_json::json!({"echo": input}))
    ///     })
    ///     .run()
    ///     .await?;
    /// # let _ = result;
    /// # Ok(())
    /// # }
    /// ```
    pub fn tool_raw<R, E, F, Fut>(self, tool: Tool, handler: F) -> Self
    where
        R: serde::Serialize,
        E: Display,
        F: Fn(serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = std::result::Result<R, E>> + Send + 'static,
    {
        let erased: ErasedHandler = Box::new(move |input| {
            let call = handler(input);
            Box::pin(async move { call.await.map_err(|err| err.to_string()).and_then(to_json) })
        });
        self.register(tool, erased)
    }

    /// Stores a handler under the tool's name and mirrors the tool into the
    /// request, replacing any earlier registration of the same name so the wire
    /// never carries a duplicate tool definition.
    fn register(mut self, tool: Tool, handler: ErasedHandler) -> Self {
        let name = tool.name.clone();
        if self.handlers.insert(name.clone(), handler).is_none() {
            self.names.push(name.clone());
        }
        let tools = self.request.tools.get_or_insert_with(Vec::new);
        // Replace a same-named custom tool — an earlier registration, or one the
        // caller had already put on the request — instead of sending the API two
        // definitions for one name.
        let existing = tools
            .iter()
            .position(|entry| matches!(entry, ToolUnion::Custom(custom) if custom.name == name));
        match existing {
            Some(index) => tools[index] = ToolUnion::Custom(tool),
            None => tools.push(ToolUnion::Custom(tool)),
        }
        self
    }

    /// Caps how many API round-trips the run may make. Defaults to `10`.
    ///
    /// One turn is one `POST /v1/messages`, so a run that answers after a
    /// single round of tool calls uses two turns. Hitting the cap while the
    /// model still wants tools is an [`Error::ToolRunner`]; see
    /// [`run`](ToolRunner::run).
    ///
    /// A cap of `0` is rejected by [`run`](ToolRunner::run) with
    /// [`Error::Config`] — a run that is not allowed to call the API at all has
    /// no meaningful result.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use crimson_crab::api::MessagesRequest;
    /// # fn demo(client: &crimson_crab::Client, request: MessagesRequest) {
    /// let runner = client.messages().runner(request).max_turns(3);
    /// # let _ = runner;
    /// # }
    /// ```
    pub fn max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = max_turns;
        self
    }

    /// Installs an observer called with every response, immediately after it
    /// arrives and **before** its tool calls are executed.
    ///
    /// This is the logging/progress hook: it sees the message but cannot change
    /// it or the run. Richer per-turn hooks — approving a tool call before it
    /// runs, rewriting a result on the way back — are planned; they need a
    /// shape that can say "no", which this one deliberately does not have.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use crimson_crab::api::MessagesRequest;
    /// # async fn demo(client: &crimson_crab::Client, request: MessagesRequest)
    /// # -> crimson_crab::Result<()> {
    /// let mut seen = 0;
    /// let result = client
    ///     .messages()
    ///     .runner(request)
    ///     .on_turn(|message| {
    ///         seen += 1;
    ///         eprintln!("turn: {} output tokens", message.usage.output_tokens);
    ///     })
    ///     .run()
    ///     .await?;
    /// assert_eq!(seen, result.turns);
    /// # Ok(())
    /// # }
    /// ```
    pub fn on_turn<F>(mut self, on_turn: F) -> Self
    where
        F: FnMut(&Message) + Send + 'a,
    {
        self.on_turn = Some(Box::new(on_turn));
        self
    }

    /// Runs the loop to completion and returns the [`ToolRunResult`].
    ///
    /// Each turn sends the conversation, hands the response to the
    /// [`on_turn`](ToolRunner::on_turn) observer, and then either stops or
    /// answers tool calls:
    ///
    /// * **Stop.** Any `stop_reason` other than [`StopReason::ToolUse`] ends the
    ///   run and is returned as-is — including [`StopReason::Refusal`] and
    ///   [`StopReason::MaxTokens`]. The runner does not judge outcomes; read
    ///   `result.message.stop_reason` and decide for yourself.
    /// * **Answer.** Every `tool_use` block is executed (concurrently), and all
    ///   of the results go back in **one** user message of `tool_result` blocks,
    ///   in the order the calls appeared — the shape the API requires.
    ///
    /// Tool failures are data, not errors: a handler `Err`, an input that does
    /// not fit the handler's argument type, and a call to a tool that was never
    /// registered all become a `tool_result` with `is_error: true` carrying an
    /// explanation, and the loop continues so the model can correct itself.
    ///
    /// # Errors
    ///
    /// * [`Error::ToolRunner`] when the turn cap is reached while the model is
    ///   still requesting tools. Exactly [`max_turns`](ToolRunner::max_turns)
    ///   requests will have been sent, and the error's `turns` says how many.
    /// * [`Error::Config`] when the turn cap is `0`.
    /// * Anything [`Messages::create`] can return — a transport failure or an
    ///   API error ends the run immediately, on whichever turn it happened.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use crimson_crab::api::MessagesRequest;
    /// # use crimson_crab::types::{StopReason, Tool};
    /// # async fn demo(client: &crimson_crab::Client, request: MessagesRequest, tool: Tool)
    /// # -> crimson_crab::Result<()> {
    /// let result = client
    ///     .messages()
    ///     .runner(request)
    ///     .tool_raw(tool, |_input: serde_json::Value| async move {
    ///         Ok::<_, String>("done")
    ///     })
    ///     .run()
    ///     .await?;
    ///
    /// if result.message.stop_reason == Some(StopReason::Refusal) {
    ///     eprintln!("the model declined");
    /// }
    /// println!("{}", result.message.text());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run(mut self) -> Result<ToolRunResult> {
        if self.max_turns == 0 {
            return Err(Error::Config(
                "`max_turns` must be at least 1; a run that cannot call the API has no result"
                    .to_string(),
            ));
        }

        let mut turns = 0usize;
        loop {
            let message = self.messages.create(&self.request).await?;
            turns += 1;
            if let Some(on_turn) = self.on_turn.as_mut() {
                on_turn(&message);
            }

            let calls = tool_calls(&message);
            // `tool_use` with no `tool_use` block would leave nothing to answer
            // and would resend an identical request forever, so treat it as a
            // stop rather than looping.
            if message.stop_reason != Some(StopReason::ToolUse) || calls.is_empty() {
                self.request.messages.push(message.clone().into_param());
                return Ok(ToolRunResult {
                    message,
                    messages: self.request.messages,
                    turns,
                });
            }

            if turns >= self.max_turns {
                return Err(Error::ToolRunner {
                    message: format!(
                        "the model was still requesting tools ({}) after the {turns}-turn limit",
                        joined(calls.iter().map(|call| call.name.as_str()))
                    ),
                    turns,
                });
            }

            let outcomes = join_all(
                calls
                    .iter()
                    .map(|call| match self.handlers.get(&call.name) {
                        Some(handler) => handler(call.input.clone()),
                        None => {
                            let message = unknown_tool(&call.name, &self.names);
                            Box::pin(async move { Err(message) }) as BoxFuture<'static, _>
                        }
                    }),
            )
            .await;

            let results: Vec<ContentBlockParam> = calls
                .iter()
                .zip(outcomes)
                .map(|(call, outcome)| match outcome {
                    Ok(value) => ContentBlockParam::tool_result(&call.id, as_text(value)),
                    Err(text) => {
                        ContentBlockParam::ToolResult(ToolResultBlockParam::error(&call.id, text))
                    }
                })
                .collect();

            self.request.messages.push(message.into_param());
            self.request.messages.push(MessageParam::user(results));
        }
    }
}

/// The `tool_use` blocks of a response, in wire order.
fn tool_calls(message: &Message) -> Vec<ToolUseBlock> {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse(call) => Some(call.clone()),
            _ => None,
        })
        .collect()
}

/// Serializes a handler's success value, reporting a serialization failure on
/// the same channel as a handler error so it reaches the model instead of
/// killing the run.
fn to_json<R: serde::Serialize>(value: R) -> std::result::Result<serde_json::Value, String> {
    serde_json::to_value(value).map_err(|err| format!("the tool result did not serialize: {err}"))
}

/// Renders a handler's JSON result as `tool_result` text: a JSON string is sent
/// as its own contents (not as a quoted JSON literal), anything else as its
/// compact JSON form.
fn as_text(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text,
        other => other.to_string(),
    }
}

/// The `is_error` text sent back when the model calls a tool nobody registered.
fn unknown_tool(name: &str, registered: &[String]) -> String {
    if registered.is_empty() {
        return format!("unknown tool `{name}`: this runner has no registered tools");
    }
    format!(
        "unknown tool `{name}`: the registered tools are {}",
        joined(registered.iter().map(String::as_str))
    )
}

/// Formats names as a backtick-quoted, comma-separated list.
fn joined<'n>(names: impl Iterator<Item = &'n str>) -> String {
    names
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_results_are_sent_unquoted() {
        assert_eq!(as_text(serde_json::json!("22C in Paris")), "22C in Paris");
        assert_eq!(as_text(serde_json::json!({"temp": 22})), r#"{"temp":22}"#);
        assert_eq!(as_text(serde_json::json!(null)), "null");
    }

    #[test]
    fn unknown_tool_message_lists_registered_names() {
        let names = vec!["get_weather".to_string(), "get_time".to_string()];
        let message = unknown_tool("get_stock", &names);
        assert!(message.contains("`get_stock`"), "{message}");
        assert!(message.contains("`get_weather`, `get_time`"), "{message}");

        let empty = unknown_tool("anything", &[]);
        assert!(empty.contains("no registered tools"), "{empty}");
    }
}
