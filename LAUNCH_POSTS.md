# Ready-to-paste launch posts

(Personal-identity channels — post these yourself. Timing: concentrate all of them within 48 hours of `cargo publish` to spike star velocity onto GitHub Trending. Adjust numbers to match the final release.)

## r/rust — "Show" post (Day 2)

**Title:** crimson-crab: a production-grade Claude API SDK for Rust — streaming, tool use, prompt caching, batches, zero-surprise types

**Body:**

Hi r/rust! I just published crimson-crab, a dedicated Rust SDK for Anthropic's Claude API.

Why another AI crate? The multi-provider frameworks (rig, genai — both great) treat Claude as one adapter among many, so Claude-specific features lag: thinking blocks, prompt caching, message batches, new betas. Meanwhile every previously dedicated Anthropic crate has been abandoned for 1–2 years. If you build on Claude, there was no living, wire-faithful client. Now there is.

What you get:

- Full Messages API: streaming (hand-rolled SSE, accumulates to a final `Message`), tool use, adaptive thinking, prompt caching, structured outputs, token counting, models, batches
- Forward compatible by design: every type-tagged enum has an `Unknown` catch-all, `model` is an open string — new models (Fable/Mythos class included) work day one without an SDK update
- Tokio-free public API (`futures::Stream`), works toward WASM targets
- Typed errors per status with retry-after capture; automatic retries with exponential backoff matching the official SDKs' policy
- 160+ tests including wiremock integration tests and serde round-trips against real API fixtures; zero clippy warnings; no unwrap() in library code

`cargo add crimson-crab` — docs at docs.rs/crimson-crab, repo at github.com/singhpratech/crimson-crab

The promise of the project: same-week support for every new Anthropic API feature. Feedback and brutal reviews very welcome — that's why I'm here.

## Hacker News — Show HN (Day 4)

**Title:** Show HN: Crimson Crab – a Rust SDK for the Claude API

**Text:** I built a dedicated, production-grade Rust client for Anthropic's Claude API: streaming SSE, tool use, thinking, prompt caching, batches — with forward-compatible types so new models work the day they ship. Every previous Rust Anthropic client was abandoned; the multi-provider frameworks lag on Claude-specific features. Tokio-free public API, 160+ tests, MIT/Apache-2.0. Would love feedback on the API design.

## This Week in Rust (Day 3)

PR to https://github.com/rust-lang/this-week-in-rust adding under "Updates from Rust Community / Project Updates":

[crimson-crab](https://github.com/singhpratech/crimson-crab) — a production-grade Rust SDK for Anthropic's Claude API (streaming, tool use, prompt caching, batches)

## X / Twitter thread

1/ Shipped: crimson-crab 🦀 — the production-grade Rust SDK for Anthropic's Claude API.
Streaming, tools, thinking, prompt caching, batches. `cargo add crimson-crab`
2/ Built it because every dedicated Anthropic crate in Rust was abandoned, and multi-provider clients lag on Claude-specific features. Dedicated wins: that's the async-openai playbook.
3/ Forward-compatible by design: unknown-tolerant types + open model strings = new Claude models work day one, no SDK update needed.
4/ 160+ tests, wiremock integration suites, zero clippy warnings, tokio-free public API. And a promise: same-week support for every new Anthropic API feature. ⭐ github.com/singhpratech/crimson-crab

## Discord blurbs (Rust #showcase, Anthropic dev server)

Just released crimson-crab — a dedicated production-grade Rust SDK for the Claude API (streaming, tools, thinking, caching, batches; forward-compatible types; 160+ tests). Would love feedback from anyone building Claude apps in Rust: https://github.com/singhpratech/crimson-crab

## Awesome-list PR one-liner

`crimson-crab` — Production-grade Rust SDK for Anthropic's Claude API: streaming, tool use, thinking, prompt caching, batches. [crates.io](https://crates.io/crates/crimson-crab)
