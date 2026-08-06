# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-08-06

### Added

- **Tool runner** — `client.messages().runner(request)` drives the agentic
  `create → run tools → feed results → repeat` loop, so the common case is no
  longer a hand-written loop. It takes the request by value and owns it: each
  registered tool is appended to `tools`, and each turn appends the assistant
  turn plus a single user message of `tool_result` blocks to `messages`. The
  manual loop stays fully supported and unchanged.
- **`ToolRunner::tool(tool, handler)`** registers a tool *and* its typed async
  handler in one place, so a tool definition and the code that implements it
  cannot drift apart. The handler takes any `DeserializeOwned` argument type and
  returns any `Serialize` value; a `String` result is sent as the `tool_result`
  text, anything else as its JSON. Not gated on the `schemars` feature — it
  accepts any `Tool`, whether the schema was hand-written or derived with
  `Tool::from_type`. **`ToolRunner::tool_raw`** is the same with a
  `serde_json::Value` handler, for inputs that aren't worth a struct.
- **Tool failures are data, not errors.** A handler `Err`, an input that does
  not deserialize into the handler's argument type, and a call to an
  unregistered tool all come back as a `tool_result` with `is_error: true`
  carrying an explanation, and the loop continues so the model can correct
  itself. Nothing is ever silently dropped.
- **Parallel tool calls need no special handling**: every `tool_use` block in a
  response is executed concurrently, and all of the results go back in one user
  message, in the order the calls appeared.
- **`ToolRunner::max_turns(n)`** caps the number of API round-trips (default
  `10`), and **`ToolRunner::on_turn(f)`** observes every response as it arrives,
  before its tools run — the logging/progress hook. Richer per-turn hooks
  (approving a call before it runs, rewriting a result on the way back) are
  planned; they need a shape that can say "no", which this one deliberately
  does not have.
- **`ToolRunResult`** carries the final `Message`, the whole conversation as
  `Vec<MessageParam>` (ending with the final assistant turn, so a follow-up
  question is one `push` away), and the number of turns used.
- **`Error::ToolRunner { message, turns }`** — returned when the turn cap is
  reached while the model is still requesting tools; `turns` is how many
  round-trips were made. Adding a variant to `Error` is not breaking: it is
  `#[non_exhaustive]`.
- Example: `tool_runner` (a two-tool weather agent, `--features schemars`).

### Notes

- Any stop reason other than `tool_use` ends a run and is returned as-is,
  including `refusal` and `max_tokens`. The runner is lower level than
  `parse::<T>()` and does not turn an outcome into an error — read
  `result.message.stop_reason` and decide for yourself.

## [0.2.1] - 2026-07-31

### Fixed

- **`parse::<T>()` rejects recursive types up front.** A recursive `T` has no
  finite inlined schema — `schemars` falls back to `$defs`/`$ref`, which
  structured output does not accept — so `parse` previously sent a request the
  API would refuse with an opaque `400`. It now returns `Error::Config` naming
  the type and the cause before anything is sent. (`Tool::from_type` is
  unaffected: tool schemas are ordinary JSON Schema, where references are
  valid.)
- **A response with no text block** (for example `tool_use`-only content) is now
  reported by `parse` as exactly that, instead of as a schema mismatch wrapping
  an opaque serde EOF error.
- **README setup for the `schemars` feature** now lists the `schemars@1` and
  `serde` dependencies your own crate needs for the derives, and warns about
  the confusing error produced by mixing in `schemars 0.8`.

## [0.2.0] - 2026-07-31

### Added

- **`schemars` feature** (optional, not on by default) — JSON Schemas derived
  from Rust types, so a schema and the type that parses against it cannot drift
  apart. Enable with `cargo add crimson-crab --features schemars`.
- **`messages().parse::<T>()`** derives `T`'s schema, tightens it into the shape
  `output_config.format` requires (subschemas inlined, `additionalProperties:
  false`, every property `required`), sets it on a **clone** of the request, and
  deserializes the response text into `T`. Returns **`ParsedMessage<T>`**
  (`data` plus the whole `Message`), so `usage` and `stop_reason` stay reachable.
  An `output_config.effort` already on the request is preserved; an
  `output_config.format` already on it is overridden. `Option<T>` fields become
  nullable properties rather than absent ones, which is what makes requiring
  every property safe.
- **`Tool::from_type::<T>(name, description)`** builds a custom tool's
  `input_schema` from its argument type, with subschemas inlined and the schema
  otherwise exactly as `schemars` emits it (no strictify pass — the strict-tool
  contract stays opt-in via `.strict(true)`). Doc comments on `T` and its fields
  become schema `description`s.
- **`Error::StructuredOutput { message, source }`** (gated on `schemars`) —
  returned by `parse` when the model refused, when the response was truncated by
  `max_tokens`, or when the text did not deserialize into `T`. `message`
  summarizes the offending response (id, model, stop reason); `source` carries
  the `serde_json` failure when there was one. Adding a variant to `Error` is
  minor-safe: it is `#[non_exhaustive]`.
- **Examples.** `typed_parse` (structured output end-to-end) and `typed_tools`
  (an agentic loop whose tool schema is derived from its argument type), both
  gated behind `required-features = ["schemars"]`.

### Changed

- **Nothing breaks.** The default feature set, the wire types, and every
  existing signature are unchanged; 0.2.0 is a minor bump only because it adds
  public API.

## [0.1.2] - 2026-07-26

### Added

- **`CLAUDE_OPUS_5`** (`claude-opus-5`) and **`CLAUDE_MYTHOS_5`** (`claude-mythos-5`)
  in `model_ids`. Opus 5 is the current Opus generation; Mythos 5 was already
  named in `docs/wire-api.md` but had no constant.

### Changed

- **Docs and examples** now open with `CLAUDE_OPUS_5` rather than the
  previous-generation `CLAUDE_OPUS_4_8`.

Both models already worked before this release — `model` is an open string
everywhere in the SDK and always has been, so `.model("claude-opus-5")` was
valid on 0.1.0. These constants are conveniences catching up to the model
lineup, not new capability.

## [0.1.1] - 2026-07-16

### Fixed

- **License.** The MIT license now names `singhpratech` as the copyright holder.

### Changed

- **Docs.** The README links to the companion MCP server template
  ([crimson-crab-mcp-template](https://github.com/singhpratech/crimson-crab-mcp-template)).

## [0.1.0] - 2026-07-16

Initial release.

### Added

- **Client & transport.** `Client` / `ClientBuilder` with `Client::from_env`
  (`ANTHROPIC_API_KEY`), configurable base URL, request timeout, and retry
  budget. Automatic retries for connection errors, 408, 409, 429, and 5xx with
  exponential backoff, full jitter, and `retry-after` support; streaming
  requests retry only before the first byte.
- **Messages endpoint.** `MessagesRequest` builder, `messages().create()`,
  `messages().count_tokens()`, per-request `betas` (sent as `anthropic-beta`),
  and `extra_body` for using new top-level fields without an SDK release.
- **Streaming.** Hand-rolled SSE parser, `StreamEvent` / `ContentDelta`, and
  `MessageStream` that accumulates a final `Message` (`final_message()` /
  `collect_final()`) identical in shape to a non-streaming response.
- **Models endpoint.** `models().get()` and `models().list()` with pagination;
  `ModelInfo` keeps the `capabilities` tree as raw JSON.
- **Message Batches endpoint.** `batches().create()`, `get()`, `list()`,
  `cancel()`, and `results()` streaming decoded `BatchResult`s from the JSONL
  results stream (line-buffered across chunk boundaries).
- **Wire types.** Full request/response type coverage for messages, content
  blocks, tools, thinking, output config, prompt caching, and usage — mirroring
  `docs/wire-api.md`.
- **Forward compatibility.** Every `type`-tagged and string-valued enum carries a
  catch-all variant that preserves unknown JSON verbatim instead of erroring.
- **Errors.** `thiserror`-based `Error` with per-status variants, `ApiError`
  (with `request_id` from the `request-id` header), `is_retryable()`, and
  `retry_after()`.
- **Model id constants** in `model_ids` for the current Claude lineup.
- **Examples.** `basic`, `streaming`, `tool_use`, `thinking`, `prompt_caching`,
  and `structured_output`.

[0.1.0]: https://github.com/singhpratech/crimson-crab/releases/tag/v0.1.0
