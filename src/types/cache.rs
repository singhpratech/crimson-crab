//! Prompt-caching control (`cache_control`).
//!
//! A [`CacheControl`] marks a cache breakpoint on a system text block, a tool
//! definition, or a message content block; it may also be supplied as a
//! top-level request field to auto-place the breakpoint on the last cacheable
//! block. The API allows at most four breakpoints per request.

use serde::{Deserialize, Serialize};

use crate::types::string_enum;

/// A prompt-caching breakpoint.
///
/// Wire shape: `{"type": "ephemeral"}` or `{"type": "ephemeral", "ttl": "1h"}`.
/// The `type` is always `"ephemeral"` today; it is kept as a free-form string so
/// a future cache type deserializes without an SDK change.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::{CacheControl, CacheTtl};
///
/// let c = CacheControl::ephemeral();
/// assert_eq!(serde_json::to_value(&c).unwrap(), serde_json::json!({"type": "ephemeral"}));
///
/// let c = CacheControl::ephemeral_with_ttl(CacheTtl::OneHour);
/// assert_eq!(
///     serde_json::to_value(&c).unwrap(),
///     serde_json::json!({"type": "ephemeral", "ttl": "1h"})
/// );
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheControl {
    /// The cache type. Always `"ephemeral"` in the current API.
    #[serde(rename = "type")]
    pub cache_type: String,
    /// Optional time-to-live for the cache entry (`"5m"` or `"1h"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<CacheTtl>,
}

impl CacheControl {
    /// Creates an ephemeral cache breakpoint with the default TTL.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::CacheControl;
    ///
    /// assert_eq!(CacheControl::ephemeral().cache_type, "ephemeral");
    /// ```
    pub fn ephemeral() -> Self {
        Self {
            cache_type: "ephemeral".to_string(),
            ttl: None,
        }
    }

    /// Creates an ephemeral cache breakpoint with an explicit TTL.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{CacheControl, CacheTtl};
    ///
    /// let c = CacheControl::ephemeral_with_ttl(CacheTtl::FiveMinutes);
    /// assert_eq!(c.ttl, Some(CacheTtl::FiveMinutes));
    /// ```
    pub fn ephemeral_with_ttl(ttl: CacheTtl) -> Self {
        Self {
            cache_type: "ephemeral".to_string(),
            ttl: Some(ttl),
        }
    }
}

impl Default for CacheControl {
    fn default() -> Self {
        Self::ephemeral()
    }
}

string_enum! {
    /// Time-to-live for a cache breakpoint.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::CacheTtl;
    ///
    /// assert_eq!(CacheTtl::OneHour.as_str(), "1h");
    /// assert_eq!(serde_json::from_str::<CacheTtl>("\"5m\"").unwrap(), CacheTtl::FiveMinutes);
    /// ```
    pub enum CacheTtl {
        /// Five minutes (`"5m"`).
        FiveMinutes = "5m",
        /// One hour (`"1h"`).
        OneHour = "1h",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt<T>(json: serde_json::Value) -> serde_json::Value
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let parsed: T = serde_json::from_value(json).expect("deserialize");
        let out = serde_json::to_value(&parsed).expect("serialize");
        let reparsed: T = serde_json::from_value(out.clone()).expect("re-deserialize");
        assert_eq!(parsed, reparsed, "struct round-trip mismatch");
        out
    }

    // Fixture: cache_control forms from docs/wire-api.md ("cache_control").
    #[test]
    fn ephemeral_forms_round_trip() {
        let plain = serde_json::json!({"type": "ephemeral"});
        assert_eq!(rt::<CacheControl>(plain.clone()), plain);

        let one_hour = serde_json::json!({"type": "ephemeral", "ttl": "1h"});
        assert_eq!(rt::<CacheControl>(one_hour.clone()), one_hour);

        let five_min = serde_json::json!({"type": "ephemeral", "ttl": "5m"});
        assert_eq!(rt::<CacheControl>(five_min.clone()), five_min);
    }

    #[test]
    fn unknown_ttl_is_preserved() {
        let parsed: CacheTtl = serde_json::from_value(serde_json::json!("42m")).unwrap();
        assert_eq!(parsed, CacheTtl::Unknown("42m".to_string()));
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            serde_json::json!("42m")
        );
    }
}
