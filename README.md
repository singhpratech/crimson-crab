# crimson-crab

**The production-grade Rust SDK for Anthropic's Claude API.**

Everything the Claude API can do, in idiomatic Rust — streaming, tools, thinking, prompt caching, and batches — with zero-surprise types and bulletproof retries.

[![crates.io](https://img.shields.io/crates/v/crimson-crab.svg)](https://crates.io/crates/crimson-crab)
[![docs.rs](https://img.shields.io/docsrs/crimson-crab)](https://docs.rs/crimson-crab)
[![CI](https://github.com/example/crimson-crab/actions/workflows/ci.yml/badge.svg)](https://github.com/example/crimson-crab/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/crimson-crab.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.75-blue.svg)](#minimum-supported-rust-version)

> crimson-crab is an independent open-source project and is not affiliated with Anthropic.

---

## Install

```sh
cargo add crimson-crab
```

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

## Why crimson-crab

- **Wire-faithful types — your code never breaks on new models.** Types mirror the API exactly; there are no renamed concepts or leaky abstractions to relearn.
- **Forward-compatible enums — future models work day one.** Unknown content blocks, stream events, stop reasons, and tool definitions deserialize into a catch-all that preserves the raw JSON instead of erroring, so new capabilities (including future Fable- and Mythos-class models) work without waiting for an SDK release.
- **A tokio-free public API — runs anywhere.** The public surface exposes `futures_core::Stream`, not runtime-specific types; `tokio` is a dev-dependency only. Native and `wasm32-unknown-unknown` are both supported targets.
- **Official-SDK-parity retries — production-grade out of the box.** Connection errors, 408/409/429, and 5xx are retried with exponential backoff, full jitter, and `retry-after` support. Streaming requests retry only before the first byte.
- **Tested against real API fixtures.** Every content block and stream event from the wire reference has a serde round-trip test, and every endpoint has wiremock coverage.

## Feature coverage

| Capability | crimson-crab | Generic multi-provider clients |
|---|:---:|:---:|
| Messages (create / count tokens) | ✅ | ✅ |
| Fine-grained SSE streaming + accumulation | ✅ | partial |
| Tool use (custom + server tools passthrough) | ✅ | partial |
| Extended thinking (adaptive / budget / display) | ✅ | rare |
| Prompt caching (`cache_control`, TTLs) | ✅ | rare |
| Structured output (`output_config` JSON Schema) | ✅ | rare |
| Message Batches (create / poll / cancel / stream results) | ✅ | rare |
| Forward-compatible unknown-variant handling | ✅ | rare |
| New beta flags without an SDK release (`betas` + `extra_body`) | ✅ | rare |

Claude-specific features are first-class here because Claude is the only API this crate targets.

## Streaming

```rust,no_run
use crimson_crab::prelude::*;
use crimson_crab::streaming::{ContentDelta, StreamEvent};
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
// The stream accumulates a complete `Message` identical to a non-streaming response.
let _final = stream.final_message();
# Ok(())
# }
```

## Tool use (manual loop)

```rust,no_run
use crimson_crab::prelude::*;
use crimson_crab::types::ContentBlock;

# async fn run(client: &Client, mut messages: Vec<MessageParam>, tool: Tool) -> crimson_crab::Result<()> {
loop {
    let request = MessagesRequest::builder()
        .model("claude-opus-4-8")
        .max_tokens(1024)
        .messages(messages.clone())
        .tool(tool.clone())
        .build()?;
    let message = client.messages().create(&request).await?;

    if message.stop_reason != Some(StopReason::ToolUse) {
        println!("{}", message.text());
        break;
    }

    // Answer every tool call, then append the assistant turn verbatim via
    // `into_param()` (no lossy serde_json round-trip) followed by one user
    // message carrying all the tool results.
    let mut results = Vec::new();
    for block in &message.content {
        if let ContentBlock::ToolUse(call) = block {
            // ...run the tool for `call.name` with `call.input`...
            results.push(ContentBlockParam::tool_result(&call.id, "tool output"));
        }
    }
    messages.push(message.into_param());
    messages.push(MessageParam::user(results));
}
# Ok(())
# }
```

More runnable programs live in [`examples/`](examples): `basic`, `streaming`, `tool_use`, `thinking`, `prompt_caching`, and `structured_output`. Run one with:

```sh
ANTHROPIC_API_KEY=sk-ant-... cargo run --example streaming
```

## Roadmap

Shipping fast and iterating. On deck for v0.2+: the Files API (beta), `schemars`-derived tool schemas with a `#[tool]` macro, a tool-runner loop helper, a `parse::<T>()` structured-output helper, the Admin/Usage API, and Vertex/Bedrock transports. The `model` field is an open string everywhere, so any model Anthropic ships works today with zero changes.

## Minimum supported Rust version

MSRV is **1.75**. Raising it is a minor-version change.

## Semver policy

The wire enums are `#[non_exhaustive]` and carry catch-all variants, so a minor release may add new known variants without breaking your build. Match with a wildcard arm on SDK enums to stay forward-compatible.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.

---

*crimson-crab is an independent open-source project, not affiliated with Anthropic.*
