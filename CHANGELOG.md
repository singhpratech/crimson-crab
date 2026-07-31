# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
