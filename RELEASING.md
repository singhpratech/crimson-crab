# Release protocol — a version NEVER ships unless every gate passes

Non-negotiable: no broken, panicking, or half-tested version reaches crates.io. If any gate fails, the release stops. There is no "ship it anyway".

## Gate 0 — Panic-free library guarantee
- `[lints.clippy]` in Cargo.toml enforces `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`, `indexing_slicing = "warn"` for library code (tests/examples exempt via `#[allow]`).
- Every fallible path returns `Result<_, Error>`; every error variant is constructible and documented. Malformed server responses, unknown JSON, dropped connections, and mid-stream errors must surface as typed errors — never a panic, never a hang.

## Gate 1 — Static quality (must all be exit 0)
```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps            # with RUSTDOCFLAGS="-D warnings"
```

## Gate 2 — Tests, from a CLEAN state
```
git status --porcelain         # must be empty (release from committed code only)
cargo test --all-features      # unit + wiremock integration + doctests
cargo build --examples
cargo check --no-default-features --features native-tls
```

## Gate 3 — Adversarial robustness evidence (first release + any release touching streaming/http)
- SSE parser fuzz-ish tests: chunk splits mid-line/mid-UTF-8, CRLF, in-stream error events.
- Retry tests: 429 with retry-after honored, 500→200 recovery, no retry on 400.
- Unknown-tolerance tests: fabricated future content-block/event types must deserialize, not error.

## Gate 4 — Consumer reality check
- Fresh `cargo new` consumer with the crate as a path dependency compiles and runs the quickstart against wiremock.
- (When ANTHROPIC_API_KEY is available) one live smoke test: basic message + a short stream against the real API.

## Gate 5 — Packaging
```
cargo publish --dry-run        # package verification
```
- Version bumped per semver; CHANGELOG.md entry written; grep audit: zero forbidden legacy names anywhere.

## Only then
```
git tag vX.Y.Z && git push --tags
cargo publish
```
Create the GitHub Release with notes. Verify docs.rs builds within the hour; if docs.rs fails, fix-forward immediately with a patch release.

## Yanking policy
If a shipped version is discovered broken: `cargo yank --vers X.Y.Z` immediately, patch release same day, post-mortem note in CHANGELOG.
