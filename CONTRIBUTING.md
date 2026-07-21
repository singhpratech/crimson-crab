# Contributing to crimson-crab

Thanks for helping make the best Claude SDK in Rust! 🦀

## Quick start
1. Fork and clone, then `cargo test` — everything should be green.
2. Make your change. House rules: no `unwrap()`/`expect()` in library code, rustdoc on all public items, wire shapes must match `docs/wire-api.md` exactly.
3. Gates before pushing: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`.
4. Open a PR with a clear description. Small, focused PRs merge fastest.

## What we love
- New Anthropic API feature support (with fixtures + tests) — same-week API parity is this project's promise.
- Serde round-trip tests from real API payloads.
- Docs and example improvements.

## Wire changes
Any change to request/response types must cite the Anthropic docs and include a round-trip test fixture. Forward compatibility is sacred: type-tagged enums keep their `Unknown` catch-alls.

## Releases
We ship small and often (weekly cadence). Maintainers handle publishing.
