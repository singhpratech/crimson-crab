//! String constants for the current Claude model ids.
//!
//! These are plain `&str` constants so they can be passed anywhere a model id
//! is expected without allocation. Model ids are validated server-side, so a
//! model not listed here can still be used by passing its id string directly.
//!
//! # Examples
//!
//! ```
//! use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
//! assert_eq!(CLAUDE_OPUS_4_8, "claude-opus-4-8");
//! ```

/// Claude Fable 5 (`claude-fable-5`).
pub const CLAUDE_FABLE_5: &str = "claude-fable-5";

/// Claude Opus 4.8 (`claude-opus-4-8`).
pub const CLAUDE_OPUS_4_8: &str = "claude-opus-4-8";

/// Claude Opus 4.7 (`claude-opus-4-7`).
pub const CLAUDE_OPUS_4_7: &str = "claude-opus-4-7";

/// Claude Opus 4.6 (`claude-opus-4-6`).
pub const CLAUDE_OPUS_4_6: &str = "claude-opus-4-6";

/// Claude Sonnet 5 (`claude-sonnet-5`).
pub const CLAUDE_SONNET_5: &str = "claude-sonnet-5";

/// Claude Sonnet 4.6 (`claude-sonnet-4-6`).
pub const CLAUDE_SONNET_4_6: &str = "claude-sonnet-4-6";

/// Claude Haiku 4.5 alias (`claude-haiku-4-5`).
pub const CLAUDE_HAIKU_4_5: &str = "claude-haiku-4-5";

/// Claude Haiku 4.5, pinned snapshot (`claude-haiku-4-5-20251001`).
pub const CLAUDE_HAIKU_4_5_20251001: &str = "claude-haiku-4-5-20251001";

/// Legacy: Claude Opus 4.5 (`claude-opus-4-5`).
pub const CLAUDE_OPUS_4_5: &str = "claude-opus-4-5";

/// Legacy: Claude Sonnet 4.5 (`claude-sonnet-4-5`).
pub const CLAUDE_SONNET_4_5: &str = "claude-sonnet-4-5";
