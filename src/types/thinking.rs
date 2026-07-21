//! Extended-thinking configuration (`thinking` request field).

use serde::de::{self, Deserializer};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use crate::types::string_enum;

/// Configures extended thinking for a request.
///
/// The `adaptive` mode lets the model choose its own reasoning budget and is
/// preferred on current models; `enabled` with an explicit `budget_tokens` is
/// for older models; `disabled` turns thinking off. An unknown `type` value
/// deserializes to [`ThinkingConfig::Unknown`], which preserves the raw JSON and
/// re-serializes it unchanged so a newer thinking shape round-trips.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::{ThinkingConfig, ThinkingDisplay};
///
/// assert_eq!(
///     serde_json::to_value(ThinkingConfig::adaptive()).unwrap(),
///     serde_json::json!({"type": "adaptive"})
/// );
/// assert_eq!(
///     serde_json::to_value(ThinkingConfig::adaptive_with_display(ThinkingDisplay::Summarized))
///         .unwrap(),
///     serde_json::json!({"type": "adaptive", "display": "summarized"})
/// );
/// assert_eq!(
///     serde_json::to_value(ThinkingConfig::enabled(8192)).unwrap(),
///     serde_json::json!({"type": "enabled", "budget_tokens": 8192})
/// );
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ThinkingConfig {
    /// The model chooses its own reasoning budget.
    Adaptive {
        /// How much of the reasoning to surface in the response.
        display: Option<ThinkingDisplay>,
    },
    /// Extended thinking with a fixed token budget (older models only).
    Enabled {
        /// The maximum number of tokens the model may spend thinking.
        budget_tokens: u32,
    },
    /// Extended thinking is turned off.
    Disabled,
    /// Forward-compatible catch-all preserving the raw JSON of an unknown
    /// `type` value.
    Unknown(serde_json::Value),
}

impl Serialize for ThinkingConfig {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ThinkingConfig::Adaptive { display } => {
                let len = 1 + usize::from(display.is_some());
                let mut map = serializer.serialize_map(Some(len))?;
                map.serialize_entry("type", "adaptive")?;
                if let Some(display) = display {
                    map.serialize_entry("display", display)?;
                }
                map.end()
            }
            ThinkingConfig::Enabled { budget_tokens } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "enabled")?;
                map.serialize_entry("budget_tokens", budget_tokens)?;
                map.end()
            }
            ThinkingConfig::Disabled => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "disabled")?;
                map.end()
            }
            ThinkingConfig::Unknown(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ThinkingConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let tag = value
            .get("type")
            .and_then(|t| t.as_str())
            .map(str::to_owned);

        #[derive(Deserialize)]
        struct AdaptiveFields {
            #[serde(default)]
            display: Option<ThinkingDisplay>,
        }
        #[derive(Deserialize)]
        struct EnabledFields {
            budget_tokens: u32,
        }

        let config = match tag.as_deref() {
            Some("adaptive") => {
                let r: AdaptiveFields = serde_json::from_value(value).map_err(de::Error::custom)?;
                ThinkingConfig::Adaptive { display: r.display }
            }
            Some("enabled") => {
                let r: EnabledFields = serde_json::from_value(value).map_err(de::Error::custom)?;
                ThinkingConfig::Enabled {
                    budget_tokens: r.budget_tokens,
                }
            }
            Some("disabled") => ThinkingConfig::Disabled,
            _ => ThinkingConfig::Unknown(value),
        };
        Ok(config)
    }
}

impl ThinkingConfig {
    /// `{"type": "adaptive"}` with no `display` override.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ThinkingConfig;
    /// assert!(matches!(ThinkingConfig::adaptive(), ThinkingConfig::Adaptive { .. }));
    /// ```
    pub fn adaptive() -> Self {
        ThinkingConfig::Adaptive { display: None }
    }

    /// `{"type": "adaptive", "display": ...}`.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{ThinkingConfig, ThinkingDisplay};
    /// let t = ThinkingConfig::adaptive_with_display(ThinkingDisplay::Omitted);
    /// assert!(matches!(t, ThinkingConfig::Adaptive { display: Some(ThinkingDisplay::Omitted) }));
    /// ```
    pub fn adaptive_with_display(display: ThinkingDisplay) -> Self {
        ThinkingConfig::Adaptive {
            display: Some(display),
        }
    }

    /// `{"type": "enabled", "budget_tokens": ...}`.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ThinkingConfig;
    /// assert!(matches!(ThinkingConfig::enabled(1024), ThinkingConfig::Enabled { .. }));
    /// ```
    pub fn enabled(budget_tokens: u32) -> Self {
        ThinkingConfig::Enabled { budget_tokens }
    }

    /// `{"type": "disabled"}`.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ThinkingConfig;
    /// assert!(matches!(ThinkingConfig::disabled(), ThinkingConfig::Disabled));
    /// ```
    pub fn disabled() -> Self {
        ThinkingConfig::Disabled
    }
}

string_enum! {
    /// How much of the model's reasoning to surface in the response.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ThinkingDisplay;
    /// assert_eq!(ThinkingDisplay::Summarized.as_str(), "summarized");
    /// ```
    pub enum ThinkingDisplay {
        /// A summarized view of the reasoning.
        Summarized = "summarized",
        /// The reasoning is omitted from the response.
        Omitted = "omitted",
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

    // Fixtures: thinking config variants (docs/wire-api.md `thinking`).
    #[test]
    fn thinking_config_variants_round_trip() {
        for j in [
            serde_json::json!({"type": "adaptive"}),
            serde_json::json!({"type": "adaptive", "display": "summarized"}),
            serde_json::json!({"type": "adaptive", "display": "omitted"}),
            serde_json::json!({"type": "enabled", "budget_tokens": 8192}),
            serde_json::json!({"type": "disabled"}),
        ] {
            assert_eq!(rt::<ThinkingConfig>(j.clone()), j);
        }
    }

    #[test]
    fn unknown_thinking_type_is_preserved() {
        let j = serde_json::json!({"type": "future_mode", "budget": 5});
        let parsed: ThinkingConfig = serde_json::from_value(j.clone()).unwrap();
        assert!(matches!(parsed, ThinkingConfig::Unknown(_)));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), j);
    }
}
