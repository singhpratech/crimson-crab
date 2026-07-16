# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/example/crimson-crab/releases/tag/v0.1.0
