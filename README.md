# 🦀 crimson-crab

**The production-grade Rust SDK for Anthropic's Claude API.**

Everything the Claude API can do, in idiomatic Rust — streaming, tool use, extended thinking, prompt caching, and batches — with wire-faithful, zero-surprise types and bulletproof retries. The definitive Rust client for building on Claude: depth beats breadth.

[![crates.io](https://img.shields.io/crates/v/crimson-crab.svg)](https://crates.io/crates/crimson-crab)
[![docs.rs](https://img.shields.io/docsrs/crimson-crab)](https://docs.rs/crimson-crab)
[![CI](https://github.com/singhpratech/crimson-crab/actions/workflows/ci.yml/badge.svg)](https://github.com/singhpratech/crimson-crab/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/crimson-crab.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.75-blue.svg)](#minimum-supported-rust-version)

**187 tests · zero clippy warnings · a panic-free library (`unwrap`/`expect`/`panic` denied at compile time) · MSRV 1.75 · MIT OR Apache-2.0**

- **Docs:** [docs.rs/crimson-crab](https://docs.rs/crimson-crab)
- **Site:** [singhpratech.github.io/crimson-crab](https://singhpratech.github.io/crimson-crab/)
- **Source:** [github.com/singhpratech/crimson-crab](https://github.com/singhpratech/crimson-crab)

---

## Install

```sh
cargo add crimson-crab
```

Streaming and batch results are plain [`futures_core::Stream`]s. To drive them with `.next()`, add the `StreamExt` extension trait:

```sh
cargo add futures-util
```

[`futures_core::Stream`]: https://docs.rs/futures-core/latest/futures_core/stream/trait.Stream.html

## 30-second quickstart

```rust,no_run
use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::prelude::*;

#[tokio::main]
async fn main() -> crimson_crab::Result<()> {
    // Reads ANTHROPIC_API_KEY from the environment.
    let client = Client::from_env()?;

    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(1024)
        .messages(vec![MessageParam::user("Explain Rust's borrow checker in one line.")])
        .build()?;

    let message = client.messages().create(&request).await?;
    println!("{}", message.text());
    Ok(())
}
```

`Client` is `Clone + Send + Sync` and shares one connection pool, so build it once and store it in your `axum` state, your MCP server struct, or a plain field — no `Arc`, no `Mutex`, no manual bounds.

## Why crimson-crab

- **Wire-faithful types — your code never breaks on new models.** Types mirror the API field-for-field; there are no renamed concepts or leaky abstractions to relearn, and no adapter layer to lag behind a release.
- **Forward-compatible enums — future models work day one.** Every wire enum (content blocks, stream events, deltas, stop reasons, tool definitions, cache TTLs, thinking configs) carries an `Unknown` catch-all that preserves the raw JSON and re-serializes it unchanged instead of erroring. A response from a model the SDK has never heard of — including future **Fable-** and **Mythos-class** models — deserializes cleanly and round-trips verbatim.
- **A tokio-free public API — runs anywhere.** The public surface exposes `futures_core::Stream`, not runtime-specific types; `tokio` is a dev-dependency only. The same builder code compiles for native **and** `wasm32-unknown-unknown` on default features.
- **Official-SDK-parity retries — production-grade out of the box.** Connection errors, timeouts, `408`/`409`/`429`, and `5xx` are retried with full-jitter exponential backoff (0.5s base, 8s cap) and honor `retry-after` — capped at 60s so a hostile or broken server can't park your retry loop for hours. Streaming requests retry only before the first byte.
- **Streaming that never truncates mid-generation.** The client uses an *idle* read timeout rather than a total-request deadline, so a long-but-actively-flowing SSE response is never cut off just because total elapsed time crossed a limit.
- **Tested against real API fixtures.** Every content block and stream event from the wire reference has a serde round-trip test, and every endpoint has `wiremock` coverage — 187 tests, zero clippy warnings, and a library that denies `unwrap`/`expect`/`panic` so it cannot panic on you in production.

## Feature coverage

| Capability | crimson-crab | Generic multi-provider clients |
|---|:---:|:---:|
| Messages (create / count tokens) | ✅ | ✅ |
| Fine-grained SSE streaming + accumulated final `Message` | ✅ | usually text-only |
| Tool use (custom tools + server-tool passthrough) | ✅ | partial |
| Extended thinking (adaptive / budgeted / display) | ✅ | rare |
| Prompt caching (`cache_control`, 5m/1h TTLs) | ✅ | rare |
| Structured output (`output_config` JSON Schema) | ✅ | rare |
| Message Batches (create / poll / cancel / stream results) | ✅ | rare |
| Models endpoint (get / list with pagination) | ✅ | varies |
| Forward-compatible unknown-variant handling | ✅ | varies |
| New beta flags without an SDK release (`betas` + `extra_body`) | ✅ | rare |

Claude-specific features are first-class here because Claude is the only API this crate targets. Building against several model vendors? A multi-provider framework will serve you better — [`rig`](https://crates.io/crates/rig-core) and [`genai`](https://crates.io/crates/genai) are genuinely good. **crimson-crab is for teams who have chosen Claude** and want the whole surface, exactly as Anthropic ships it.

## Streaming

Iterate typed events as they arrive; the stream accumulates a complete `Message` for you in the background.

```rust,no_run
use crimson_crab::prelude::*;
use futures_util::StreamExt;

# async fn run(client: &Client, request: &MessagesRequest) -> crimson_crab::Result<()> {
let mut stream = client.messages().stream(request).await?;
while let Some(event) = stream.next().await {
    if let StreamEvent::ContentBlockDelta {
        delta: ContentDelta::TextDelta { text },
        ..
    } = event?
    {
        print!("{text}");
    }
}
// After draining, the accumulated `Message` is identical in shape to a
// non-streaming response.
if let Some(message) = stream.final_message() {
    println!("\n[stop_reason: {:?}]", message.stop_reason);
}
# Ok(())
# }
```

### Relaying text deltas (SSE bodies, channels, web handlers)

`MessageStream` is `Send + Unpin` and `crimson_crab::Error` is `Send + Sync + std::error::Error`, so a streaming response drops straight into an `axum` `Sse` body (or any channel) with no `Box::pin` and no wrapper error type. Map the event stream down to plain `String` deltas with `filter_map`:

```rust,no_run
use crimson_crab::prelude::*;
use futures_util::StreamExt;

# async fn run(client: &Client, request: &MessagesRequest) -> crimson_crab::Result<()> {
// A `Stream<Item = crimson_crab::Result<String>>` of plain text deltas, ready to
// hand to an `axum` `Sse` body, a channel, or any consumer.
let text_deltas = client
    .messages()
    .stream(request)
    .await?
    .filter_map(|event| async move {
        match event {
            Ok(StreamEvent::ContentBlockDelta {
                delta: ContentDelta::TextDelta { text },
                ..
            }) => Some(Ok(text)),
            // A late/in-stream error surfaces as an `Err` item — forward it as a
            // final `event: error` frame instead of dropping the connection.
            Err(e) => Some(Err(e)),
            Ok(_) => None,
        }
    });

forward(text_deltas).await;
# Ok(())
# }
# async fn forward<S>(_deltas: S)
# where
#     S: futures_util::Stream<Item = crimson_crab::Result<String>>,
# {
# }
```

## Tool use (manual agentic loop)

`message.into_param()` converts a response `Message` straight into a request `MessageParam` — the two `tool_use` blocks echoed verbatim — so the "append the assistant turn, then a user message of tool results" contract is two lines with no lossy serde round-trip. Parallel tool calls need no special handling: just iterate `message.content`.

```rust,no_run
use crimson_crab::prelude::*;
use crimson_crab::types::ToolResultBlockParam;

# async fn run(client: &Client, mut messages: Vec<MessageParam>, tool: Tool) -> crimson_crab::Result<()> {
loop {
    let request = MessagesRequest::builder()
        .model("claude-opus-4-8")
        .max_tokens(1024)
        .messages(messages.clone())
        .tool(tool.clone()) // `.tool(_)` appends any `Into<ToolUnion>`; `.tools(vec)` replaces
        .build()?;
    let message = client.messages().create(&request).await?;

    if message.stop_reason != Some(StopReason::ToolUse) {
        println!("{}", message.text());
        break;
    }

    // Answer every tool call. `ContentBlock` is in the prelude, so matching
    // `ToolUse` needs no extra import.
    let mut results = Vec::new();
    for block in &message.content {
        if let ContentBlock::ToolUse(call) = block {
            match run_tool(&call.name, &call.input) {
                // Success: the discoverable `ContentBlockParam::tool_result` helper.
                Ok(output) => results.push(ContentBlockParam::tool_result(&call.id, output)),
                // Failure: surface it to the model with `is_error: true`.
                Err(why) => results.push(ContentBlockParam::ToolResult(
                    ToolResultBlockParam::error(&call.id, why),
                )),
            }
        }
    }

    // Echo the assistant turn back verbatim, then one user message of results.
    messages.push(message.into_param());
    messages.push(MessageParam::user(results));
}
# Ok(())
# }
// Your tool dispatch. Note `std::result::Result<_, _>`: see "Imports & the prelude".
# fn run_tool(_name: &str, _input: &serde_json::Value) -> std::result::Result<String, String> {
#     Ok("tool output".to_string())
# }
```

## Prompt caching & token budgeting

The simplest caching path: a plain-string system prompt plus a top-level `cache_control`, which auto-places one breakpoint on the last cacheable block — no per-block wiring.

```rust,no_run
use crimson_crab::prelude::*;

# async fn run(client: &Client) -> crimson_crab::Result<()> {
let request = MessagesRequest::builder()
    .model("claude-opus-4-8")
    .max_tokens(256)
    .system("A long, reusable system prompt worth caching…")
    .cache_control(CacheControl::ephemeral()) // or `CacheControl::ephemeral_with_ttl(CacheTtl::OneHour)`
    .messages(vec![MessageParam::user("Restate rule one.")])
    .build()?;

let message = client.messages().create(&request).await?;
let usage = &message.usage;
println!(
    "fresh input: {}  written to cache: {:?}  read from cache: {:?}",
    usage.input_tokens,
    usage.cache_creation_input_tokens,
    usage.cache_read_input_tokens,
);
# Ok(())
# }
```

**Token accounting for cost reports.** The three input buckets are **disjoint**: `input_tokens` counts only the uncached input, while `cache_creation_input_tokens` and `cache_read_input_tokens` are separate. Total input tokens = `input_tokens` + `cache_creation_input_tokens` + `cache_read_input_tokens`. Never add the cache buckets *into* `input_tokens` — that double-bills the cached prefix.

Need the number before you spend on generation? Derive a count request from the same messages request — no rebuilding the prompt twice:

```rust,no_run
# use crimson_crab::prelude::*;
# async fn run(client: &Client, request: &MessagesRequest) -> crimson_crab::Result<()> {
let count = client.messages().count_tokens(&request.as_count_request()).await?;
println!("this request will cost {} input tokens", count.input_tokens);
# Ok(())
# }
```

For fine-grained control you can attach a breakpoint to an individual block instead: build a `TextBlockParam` (in `crimson_crab::types`), set `cache_control`, and pass it as a system block or message content — see [`examples/prompt_caching.rs`](examples/prompt_caching.rs).

## Message Batches

The whole submit → poll → stream → tally pipeline. `BatchRequestItem::from_request` turns a `MessagesRequest` into a batch entry with no hand-rolled JSON; `BatchStatus` and `BatchRequestCounts` are typed for progress display; and `results()` decodes the JSONL stream line-by-line, tolerating blank lines and a missing trailing newline.

```rust,no_run
use crimson_crab::api::{BatchRequestItem, BatchResultOutcome, BatchStatus};
use crimson_crab::prelude::*;
use futures_util::StreamExt;

# async fn run(client: &Client, request: &MessagesRequest) -> crimson_crab::Result<()> {
// Submit, each entry tagged with your own custom id.
let items = vec![BatchRequestItem::from_request("row-1", request)?];
let batch = client.batches().create(&items).await?;

// Poll until the batch reaches a terminal state. (A built-in `poll_until_ended`
// helper is on the v0.2 roadmap; until then, loop with your runtime's timer and
// your own deadline guard.)
let batch = loop {
    let current = client.batches().get(&batch.id).await?;
    if current.processing_status == BatchStatus::Ended {
        break current;
    }
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
};

// Results arrive in any order — key them by `custom_id`, never by position.
let mut results = client.batches().results(&batch.id).await?;
while let Some(result) = results.next().await {
    let result = result?;
    match result.result {
        BatchResultOutcome::Succeeded(ok) => {
            println!("{}: {}", result.custom_id, ok.message.text());
        }
        BatchResultOutcome::Errored(err) => {
            // `err.error` is the raw error envelope (a `serde_json::Value`).
            println!("{}: errored: {}", result.custom_id, err.error);
        }
        BatchResultOutcome::Canceled(_) | BatchResultOutcome::Expired(_) => {}
        // `BatchResultOutcome` is `#[non_exhaustive]`; the wildcard keeps you
        // forward-compatible with outcome types added in a future release.
        _ => {}
    }
}
# Ok(())
# }
```

## Imports & the prelude

`use crimson_crab::prelude::*;` is the fastest way to get the common types — `Client`, `MessagesRequest`, `MessageParam`, `StreamEvent`, `ContentDelta`, `ContentBlock`, `ContentBlockParam`, `Tool`, `ToolChoice`, `StopReason`, `CacheControl`, and more.

One thing worth knowing: the prelude also re-exports the crate's `Result<T>` and `Error` type aliases, which **shadow** `std::result::Result` / `std::error::Error` inside a glob import. If you write a two-type-argument `Result<T, E>` in the same scope — common with `axum` handlers, `thiserror`, or macro-heavy crates like `rmcp` — it resolves to the one-argument alias and fails with a confusing `E0107` ("type alias takes 1 generic argument but 2 were supplied"). Two easy fixes:

- Fully-qualify it: `std::result::Result<String, String>` (as in the tool-loop example above); or
- Skip the glob and import exactly what you need. Curated paths: `crimson_crab::Client`, request/response types under `crimson_crab::api::*` (e.g. `MessagesRequest`), and the wire types under `crimson_crab::types::*` (e.g. `MessageParam`, `TextBlockParam`, `ToolResultBlockParam`).

## Platform support

The crate compiles for native targets and `wasm32-unknown-unknown` on **default features** — no feature juggling. On wasm, `reqwest` resolves to the browser `fetch` backend and TLS features are ignored, so you do not need to disable `rustls-tls`; `cargo tree -i tokio --target wasm32-unknown-unknown` prints nothing.

Two honest caveats for edge/browser deployments: the retry loop cannot sleep on wasm (there are no threads), so on that target retries fire without backoff and do not observe `retry-after` — the browser applies its own backpressure. Streaming type-checks on wasm but is best-effort and not exercised in a headless browser in CI. For a 429-sensitive edge worker, cap `max_retries` accordingly.

## More examples

Runnable programs live in [`examples/`](examples): `basic`, `streaming`, `tool_use`, `thinking`, `prompt_caching`, and `structured_output`. Run one with:

```sh
ANTHROPIC_API_KEY=sk-ant-... cargo run --example streaming
```

## Roadmap

Shipping fast and iterating; spec fidelity and release cadence are the whole point. On deck for v0.2+: the Files API (beta), `schemars`-derived tool schemas with a `#[tool]` macro, typed tool-input deserialization, a tool-runner loop helper, a `batches().poll_until_ended()` convenience, a `parse::<T>()` structured-output helper, the Admin/Usage API, and Vertex/Bedrock transports. The `model` field is an open string everywhere, so any model Anthropic ships works today with zero changes.

## Minimum supported Rust version

MSRV is **1.75**. Raising it is a minor-version change.

## Semver policy

The wire enums are `#[non_exhaustive]` and carry `Unknown` catch-all variants, so a minor release may add a new known variant (for a feature Anthropic ships) without breaking your build. Match with a wildcard `_` arm on SDK enums to stay forward-compatible across minor versions.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.

---

<sub>🦀 Crimson Crab · crimson-crab is an independent open-source project and is not affiliated with Anthropic.</sub>
