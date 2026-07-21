# crimson-crab — Architecture

**Crate:** `crimson-crab` (crates.io name verified available 2026-07-16)

**Pitch:** The production-grade Rust SDK for Anthropic's Claude API — the `async-openai` of the Claude ecosystem.
**License:** MIT OR Apache-2.0. **Edition:** 2021. **MSRV:** 1.75.

## Why this exists (from market research, 2026-07)

Every dedicated Anthropic crate is dead (`anthropic-sdk` stale since 2024-07, `misanthropy`, `clust`, `claude-rs` all abandoned), while the comparable OpenAI slot (`async-openai`) does 2.09M downloads/90 days. Multi-provider clients (rig, genai) treat Claude as one adapter and lag on Claude-specific features: thinking blocks, prompt caching, batches, fine-grained streaming. The moat is **release cadence + spec fidelity**, not features. Every dethroned incumbent lost by going quiet.

## Product principles

1. **Wire fidelity over cleverness.** Types mirror the API exactly (`docs/wire-api.md` is the source of truth). No renamed concepts, no leaky abstractions.
2. **Forward compatible by default.** Unknown content-block types, stream events, stop reasons, and enum values must deserialize into an `Unknown`/`Other` catch-all (preserving the raw JSON) — never a hard error. Anthropic ships new block types often; the SDK must not break on them.
3. **Runtime-light.** Public API exposes `futures_core::Stream`, not tokio types. `tokio` appears only in dev-dependencies. HTTP via `reqwest` (works on native and wasm32-unknown-unknown).
4. **Ergonomic but honest.** Builder pattern for requests; helpers for the common paths (accumulate a stream into a final `Message`, extract first text block); nothing hides `stop_reason` handling.
5. **Ship v0.1 small and correct**, iterate fast. Derive-macro tools, Files API, Vertex/Bedrock clients are v0.2+.

## Crate layout

```
crimson-crab/
├── Cargo.toml
├── src/
│   ├── lib.rs            # crate docs + quickstart doctest, re-exports, prelude
│   ├── client.rs         # Client, ClientBuilder
│   ├── error.rs          # Error, ApiErrorBody, per-status variants
│   ├── http.rs           # internal: request exec, retry w/ backoff+jitter, headers
│   ├── types/
│   │   ├── mod.rs
│   │   ├── message.rs    # Message, MessageParam, Role, SystemPrompt, Usage, StopReason, StopDetails, Metadata
│   │   ├── content.rs    # ContentBlock (response) / ContentBlockParam (request) + sources (base64/url/file image & document)
│   │   ├── tool.rs       # Tool, ToolUnion, ToolChoice, ToolResultContent
│   │   ├── thinking.rs   # ThinkingConfig (Adaptive{display} | Enabled{budget_tokens} | Disabled), ThinkingDisplay
│   │   ├── output.rs     # OutputConfig { effort, format }, Effort, OutputFormat::JsonSchema
│   │   └── cache.rs      # CacheControl (ephemeral + optional ttl "5m"/"1h")
│   ├── streaming.rs      # SSE parser, StreamEvent, deltas, MessageStream, accumulation
│   ├── api/
│   │   ├── mod.rs
│   │   ├── messages.rs   # create(), stream(), count_tokens()
│   │   ├── models.rs     # list(), get() (+ ModelInfo w/ capabilities as serde_json::Value)
│   │   └── batches.rs    # create/get/list/cancel/results (JSONL streaming decode)
│   └── models_ids.rs     # const model ID strings (CLAUDE_OPUS_4_8, CLAUDE_SONNET_5, ...)
├── examples/
│   ├── basic.rs          # simple message
│   ├── streaming.rs      # stream text deltas + final message
│   ├── tool_use.rs       # manual agentic loop (weather tool)
│   ├── thinking.rs       # adaptive thinking + effort + display
│   ├── prompt_caching.rs # system cache_control + usage verification
│   └── structured_output.rs # output_config.format json_schema
├── tests/
│   ├── messages.rs       # wiremock: create, headers, error mapping, retry behavior
│   ├── streaming.rs      # SSE fixtures → events → accumulated Message
│   └── serde_roundtrip.rs# every content block + event fixture from docs/wire-api.md
├── docs/wire-api.md      # authoritative wire reference (DO NOT GUESS; read this)
├── README.md
├── CHANGELOG.md
└── .github/workflows/ci.yml
```

## Public API sketch

```rust
// Construction
let client = Client::from_env()?;                    // ANTHROPIC_API_KEY
let client = Client::builder()
    .api_key("sk-ant-...")
    .base_url("https://api.anthropic.com")           // default
    .timeout(Duration::from_secs(600))               // default 10 min
    .max_retries(2)                                  // default 2
    .build()?;

// Messages
let req = MessagesRequest::builder()
    .model(models_ids::CLAUDE_OPUS_4_8)
    .max_tokens(1024)
    .system("You are terse.")                        // Into<SystemPrompt>
    .messages(vec![MessageParam::user("Hello")])
    .thinking(ThinkingConfig::adaptive())
    .build()?;                                       // validates required fields

let msg: Message = client.messages().create(&req).await?;
if msg.stop_reason == Some(StopReason::Refusal) { /* handle */ }
println!("{}", msg.text());                          // concat of text blocks

// Streaming
let mut stream = client.messages().stream(&req).await?;   // impl Stream<Item = Result<StreamEvent>>
while let Some(ev) = stream.next().await { ... }
let final_msg = stream.final_message();                   // accumulated, after drain
// or the one-liner:
let msg = client.messages().stream(&req).await?.collect_final().await?;

// Token counting
let n = client.messages().count_tokens(&req.as_count_request()).await?.input_tokens;

// Betas (per-request)
let req = MessagesRequest::builder().betas(["files-api-2025-04-14"]).…

// Models & Batches
let m = client.models().get("claude-opus-4-8").await?;
let batch = client.batches().create(&requests).await?;
let mut results = client.batches().results(&batch.id).await?;  // Stream of BatchResult
```

### Key type decisions

- **All request/response enums** use `#[serde(tag = "type", rename_all = "snake_case")]` where the wire uses a `type` tag. Every such enum gets a final catch-all variant capturing unknown types (implement via a custom deserializer or `#[serde(untagged)]` fallback wrapper — must round-trip unmodified JSON where feasible; at minimum must not error).
- **`MessageParam::user(impl Into<UserContent>)` / `::assistant(...)`** convenience constructors; content accepts `String` (→ shorthand string form) or `Vec<ContentBlockParam>`.
- **`Tool::new(name, description, input_schema: serde_json::Value)`** — schemas are raw JSON values in v0.1 (schemars integration is v0.2). `ToolUnion` also has a `Raw(serde_json::Value)` variant so users can pass any server-tool definition (web_search_20260209 etc.) without the SDK needing to model each one.
- **`Usage`** includes `input_tokens`, `output_tokens`, `cache_creation_input_tokens: Option<u64>`, `cache_read_input_tokens: Option<u64>`, plus `#[serde(flatten)] extra: serde_json::Map` for forward-compat.
- **`StopReason`**: `EndTurn, MaxTokens, StopSequence, ToolUse, PauseTurn, Refusal, ModelContextWindowExceeded, #[serde(other)] Other`. `StopDetails { r#type, category: Option<String>, explanation: Option<String> }`.
- **Errors** (`thiserror`):
  ```rust
  pub enum Error {
      BadRequest(ApiError), Authentication(ApiError), PermissionDenied(ApiError),
      NotFound(ApiError), RequestTooLarge(ApiError), RateLimit { err: ApiError, retry_after: Option<Duration> },
      Overloaded(ApiError), Api { status: u16, err: ApiError },   // other non-2xx
      Connection(reqwest::Error), Timeout, Serde{..}, Stream{..}, Config{..},
  }
  pub struct ApiError { pub error_type: String, pub message: String, pub request_id: Option<String> }
  ```
  `request_id` read from the `request-id` response header. `Error::is_retryable()` helper.

## HTTP & retry policy

- Headers on every request: `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`; `anthropic-beta: a,b,c` when betas set (comma-joined).
- Retry (like official SDKs): connection errors, 408, 409, 429, and ≥500. Exponential backoff with jitter (0.5s base, cap 8s), honor `retry-after` header (seconds) when present. Default `max_retries = 2`, configurable, 0 disables. **Never retry other 4xx.** Streaming requests: retry only before the first byte is received.
- No retry logic exposed publicly beyond the config knob.

## SSE / streaming design

Hand-rolled SSE parser (no eventsource dep): buffer bytes → split records on `\n\n` (tolerate `\r\n`), parse `event:` and `data:` lines, ignore comments/heartbeats. Deserialize `data` by the `type` field into:

```rust
pub enum StreamEvent {
    MessageStart { message: Message },
    ContentBlockStart { index: usize, content_block: ContentBlock },
    ContentBlockDelta { index: usize, delta: ContentDelta },
    ContentBlockStop { index: usize },
    MessageDelta { delta: MessageDeltaBody, usage: Option<UsageDelta> },
    MessageStop,
    Ping,
    Error { error: ApiErrorBody },   // in-stream error event
    Unknown(serde_json::Value),
}
pub enum ContentDelta { TextDelta{text}, InputJsonDelta{partial_json}, ThinkingDelta{thinking}, SignatureDelta{signature}, Unknown(Value) }
```

`MessageStream` wraps the raw stream, yields `StreamEvent`s, and internally accumulates (text concatenation, tool-input JSON assembly from `partial_json` fragments, usage merge from `message_delta`) so `final_message()`/`collect_final()` returns a complete `Message` identical in shape to the non-streaming response.

## Dependencies (keep minimal)

`reqwest` (default-features=false; features: `json`, `stream`; TLS via crate features `rustls-tls` [default] / `native-tls`), `serde`, `serde_json`, `thiserror`, `futures-core`, `futures-util`, `bytes`, `pin-project-lite`, `fastrand` (jitter). Dev: `tokio` (macros, rt-multi-thread), `wiremock`, `anyhow`.

Feature flags: `default = ["rustls-tls"]`, `native-tls`. (No `wasm` flag needed — reqwest handles the target; CI adds a `cargo check --target wasm32-unknown-unknown` job; streaming on wasm is best-effort, documented.)

## Quality gates (CI, every PR)

`cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` · `cargo test` · `cargo doc --no-deps` (warnings denied) · `cargo check --target wasm32-unknown-unknown` (if target installed) · examples compile (`cargo build --examples`).

## README requirements — this is a PRODUCT LAUNCH page, not a docs page

Positioning: **the best way to build Claude-powered applications in Rust.** Lead from strength everywhere; never use the word "unofficial" in the title, tagline, description, or hero. A single small footer line "crimson-crab is an independent open-source project, not affiliated with Anthropic" is the ONLY affiliation mention (required — it protects the project).

Structure: hero (crab mark + tagline "The production-grade Rust SDK for Anthropic's Claude API" + one-line value prop like "Everything the Claude API can do, in idiomatic Rust — streaming, tools, thinking, caching, batches — with zero-surprise types and bulletproof retries"); badges (crates.io, docs.rs, CI, license, MSRV); `cargo add crimson-crab` + 30-second quickstart that WOWs; "Why crimson-crab" section selling the differentiators as benefits (wire-faithful types = your code never breaks on new models; forward-compatible enums = future models work day one incl. Fable/Mythos class; tokio-free public API = runs anywhere incl. WASM; official-SDK-parity retries = production-grade out of the box; every feature covered by tests against real API fixtures); feature matrix vs "generic multi-provider clients" (confident, factual, never trash-talking by name); streaming + tool-loop examples; roadmap teaser; MSRV; semver policy (minor bumps may add enum variants — safe because wire enums are `#[non_exhaustive]`); footer with the single affiliation line + Crimson Crab branding.

## v0.2+ roadmap (do NOT build now)

Files API (beta), schemars-derived tool schemas + `#[tool]` macro crate, tool-runner loop helper, Admin/Usage API, Vertex/Bedrock transports, structured-output `parse::<T>()` helper, compaction/context-management betas, fine-grained tool streaming.
