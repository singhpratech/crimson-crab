# Roadmap

This is a living document describing where crimson-crab is headed. It is a
statement of intent, not a promise of dates — priorities shift as the Anthropic
API evolves. Feedback and contributions on any item are welcome; open an issue
to discuss before starting something large.

## Guiding principles

- **Track the wire API faithfully.** New Anthropic block types, events, and
  fields should be usable the week they ship — via typed support where it's
  stable, and the `extra_body` / `betas` escape hatches until then.
- **Panic-free and forward-compatible.** Unknown JSON is preserved, never a hard
  error. `unwrap`/`expect`/`panic!`/`todo!` stay denied in the library.
- **Small, frequent releases.** One feature or fix per release beats large,
  infrequent ones. Every release gets a changelog entry.

## Shipped (0.1.x)

- Client & transport: retries with backoff + jitter, `retry-after`, timeouts.
- Messages: `create`, `count_tokens`, `betas`, `extra_body`.
- Streaming: SSE parser, `StreamEvent` / `ContentDelta`, accumulated final message.
- Models, Message Batches (with JSONL result streaming).
- Full wire types for messages, tools, thinking, prompt caching, usage.
- Model id constants; examples for the core flows.

## 0.2 — ergonomics for tools and structured output

- **`messages().parse::<T>()`** — a typed helper that constrains the response
  with `output_config.format` and deserializes it straight into your struct,
  returning a validation error rather than loose JSON.
- **Tool definitions from Rust types** — derive a tool's JSON schema from a
  `schemars`-annotated struct so a tool is defined once, in Rust, with no
  hand-written schema to drift.
- More examples: axum SSE passthrough, and a minimal typed-tools agent.

## 0.3 — the agentic loop

- **Tool runner** — an opt-in helper that drives the
  `create → run tools → feed results → repeat` loop for user-defined tools, with
  per-turn hooks (approval, logging, result rewriting) for callers who want them.
  The manual loop stays fully supported for those who'd rather own it.

## 0.4 — Files API

- **Files endpoint** (`/v1/files`): upload, list, retrieve, delete, and
  reference uploaded files from message content.

## Toward 1.0 — stability

- A semver stability pledge and a documented MSRV policy.
- API review pass: settle naming and builder ergonomics before committing to
  1.0 compatibility.
- "One year of tracking the Claude API" as the bar for 1.0 — the point is a
  client you can depend on staying current, not just a version number.

## How to help

- Picking up a roadmap item: open an issue first so we can agree on the shape.
- Smaller entry points (examples, docs, a missing wire field) are a great first
  contribution — look for issues labelled `good-first-issue`.
- Hit a rough edge or a missing Claude feature? Filing an issue is genuinely
  useful signal for what to prioritize.
