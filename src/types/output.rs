//! Output configuration (`output_config` request field): reasoning effort and
//! structured output format.

use serde::de::{self, Deserializer};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use crate::types::string_enum;

/// The `output_config` request field.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::{Effort, OutputConfig, OutputFormat};
///
/// let cfg = OutputConfig {
///     effort: Some(Effort::High),
///     format: Some(OutputFormat::json_schema(serde_json::json!({
///         "type": "object",
///         "properties": {"answer": {"type": "string"}},
///         "additionalProperties": false
///     }))),
/// };
/// let json = serde_json::to_value(&cfg).unwrap();
/// assert_eq!(json["effort"], "high");
/// assert_eq!(json["format"]["type"], "json_schema");
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    /// How much effort the model should spend before responding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// The structured output format, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputFormat>,
}

string_enum! {
    /// The reasoning-effort level for a request.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::Effort;
    /// assert_eq!(Effort::Xhigh.as_str(), "xhigh");
    /// assert_eq!(serde_json::from_str::<Effort>("\"max\"").unwrap(), Effort::Max);
    /// ```
    pub enum Effort {
        /// Minimal effort.
        Low = "low",
        /// Moderate effort.
        Medium = "medium",
        /// High effort.
        High = "high",
        /// Extra-high effort.
        Xhigh = "xhigh",
        /// Maximum effort.
        Max = "max",
    }
}

/// The structured output `format`.
///
/// Currently only `json_schema` is defined. An unknown `type` value deserializes
/// to [`OutputFormat::Unknown`], which preserves the raw JSON and re-serializes
/// it unchanged so a newer format shape round-trips.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::OutputFormat;
///
/// let fmt = OutputFormat::json_schema(serde_json::json!({"type": "object"}));
/// let json = serde_json::to_value(&fmt).unwrap();
/// assert_eq!(json["type"], "json_schema");
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum OutputFormat {
    /// Constrain output to a JSON Schema (`additionalProperties: false`).
    JsonSchema {
        /// The JSON Schema the output must conform to.
        schema: serde_json::Value,
    },
    /// Forward-compatible catch-all preserving the raw JSON of an unknown
    /// `type` value.
    Unknown(serde_json::Value),
}

impl Serialize for OutputFormat {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            OutputFormat::JsonSchema { schema } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "json_schema")?;
                map.serialize_entry("schema", schema)?;
                map.end()
            }
            OutputFormat::Unknown(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for OutputFormat {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let tag = value.get("type").and_then(|t| t.as_str());
        match tag {
            Some("json_schema") => {
                #[derive(Deserialize)]
                struct Fields {
                    schema: serde_json::Value,
                }
                let r: Fields = serde_json::from_value(value).map_err(de::Error::custom)?;
                Ok(OutputFormat::JsonSchema { schema: r.schema })
            }
            _ => Ok(OutputFormat::Unknown(value)),
        }
    }
}

impl OutputFormat {
    /// Builds a `json_schema` output format.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::OutputFormat;
    /// let fmt = OutputFormat::json_schema(serde_json::json!({"type": "object"}));
    /// assert!(matches!(fmt, OutputFormat::JsonSchema { .. }));
    /// ```
    pub fn json_schema(schema: serde_json::Value) -> Self {
        OutputFormat::JsonSchema { schema }
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

    // Fixture: output_config (docs/wire-api.md "POST /v1/messages").
    #[test]
    fn output_config_round_trips() {
        let j = serde_json::json!({
            "effort": "high",
            "format": {
                "type": "json_schema",
                "schema": {
                    "type": "object",
                    "properties": {"answer": {"type": "string"}},
                    "additionalProperties": false
                }
            }
        });
        assert_eq!(rt::<OutputConfig>(j.clone()), j);
    }

    #[test]
    fn effort_levels_round_trip() {
        for level in ["low", "medium", "high", "xhigh", "max"] {
            let j = serde_json::json!(level);
            assert_eq!(rt::<Effort>(j.clone()), j);
        }
    }

    #[test]
    fn unknown_output_format_is_preserved() {
        let j = serde_json::json!({"type": "future_format", "detail": 1});
        let parsed: OutputFormat = serde_json::from_value(j.clone()).unwrap();
        assert!(matches!(parsed, OutputFormat::Unknown(_)));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), j);
    }
}
