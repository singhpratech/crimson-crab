//! Tool definitions, the tool-list union, tool choice, and tool-result content.
//!
//! In v0.1 custom tool schemas are raw [`serde_json::Value`]s (typed/derived
//! schemas are a v0.2 feature). Server tools (`web_search_*`, `code_execution`,
//! `bash`, …) are passed through untouched via [`ToolUnion::Raw`] so the SDK
//! does not need to model each one.

use serde::de::{self, Deserializer};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use crate::types::cache::CacheControl;
use crate::types::content::ContentBlockParam;

/// A custom (client-side) tool definition.
///
/// Wire shape:
///
/// ```jsonc
/// {"name": "get_weather", "description": "...", "input_schema": { /* JSON Schema */ },
///  "strict": true, "cache_control": {"type": "ephemeral"}}
/// ```
///
/// # Examples
///
/// ```
/// use crimson_crab::types::Tool;
///
/// let tool = Tool::new(
///     "get_weather",
///     "Get the current weather for a location",
///     serde_json::json!({
///         "type": "object",
///         "properties": {"location": {"type": "string"}},
///         "required": ["location"]
///     }),
/// );
/// assert_eq!(tool.name, "get_weather");
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// The tool name the model uses to invoke it.
    pub name: String,
    /// A description of what the tool does and when to use it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The JSON Schema describing the tool's `input` object.
    pub input_schema: serde_json::Value,
    /// When `true`, requires the input to strictly match the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    /// Optional cache breakpoint (place on the last tool to cache the tool set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

impl Tool {
    /// Creates a custom tool from a name, description, and JSON Schema.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::Tool;
    ///
    /// let tool = Tool::new("noop", "does nothing", serde_json::json!({"type": "object"}));
    /// assert_eq!(tool.description.as_deref(), Some("does nothing"));
    /// ```
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: Some(description.into()),
            input_schema,
            strict: None,
            cache_control: None,
        }
    }

    /// Sets `strict` mode and returns the updated tool.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::Tool;
    ///
    /// let tool = Tool::new("t", "d", serde_json::json!({"type": "object"})).strict(true);
    /// assert_eq!(tool.strict, Some(true));
    /// ```
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = Some(strict);
        self
    }

    /// Attaches a cache breakpoint and returns the updated tool.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::{CacheControl, Tool};
    ///
    /// let tool = Tool::new("t", "d", serde_json::json!({"type": "object"}))
    ///     .cache_control(CacheControl::ephemeral());
    /// assert!(tool.cache_control.is_some());
    /// ```
    pub fn cache_control(mut self, cache_control: CacheControl) -> Self {
        self.cache_control = Some(cache_control);
        self
    }
}

/// An entry in a request's `tools` array: either a modeled custom [`Tool`] or a
/// raw server-tool definition passed through verbatim.
///
/// Deserialization prefers [`ToolUnion::Custom`] (which requires an
/// `input_schema`); server tools such as `{"type": "web_search_20260209", …}`
/// lack that field and fall through to [`ToolUnion::Raw`], so new server tools
/// work without an SDK release.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::{Tool, ToolUnion};
///
/// let custom: ToolUnion = Tool::new("t", "d", serde_json::json!({"type": "object"})).into();
/// assert!(matches!(custom, ToolUnion::Custom(_)));
///
/// let server: ToolUnion =
///     serde_json::json!({"type": "web_search_20260209", "name": "web_search"}).into();
/// assert!(matches!(server, ToolUnion::Raw(_)));
///
/// // A server-tool definition deserializes into `Raw`, not `Custom`.
/// let parsed: ToolUnion =
///     serde_json::from_value(serde_json::json!({"type": "bash_20250124", "name": "bash"}))
///         .unwrap();
/// assert!(matches!(parsed, ToolUnion::Raw(_)));
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolUnion {
    /// A modeled custom tool.
    Custom(Tool),
    /// Any other tool definition, passed through as raw JSON.
    Raw(serde_json::Value),
}

impl From<Tool> for ToolUnion {
    fn from(tool: Tool) -> Self {
        ToolUnion::Custom(tool)
    }
}

impl From<serde_json::Value> for ToolUnion {
    fn from(value: serde_json::Value) -> Self {
        ToolUnion::Raw(value)
    }
}

/// How the model should choose among the available tools.
///
/// Any variant may set `disable_parallel_tool_use` to force at most one tool
/// call per turn. An unknown `type` value deserializes to
/// [`ToolChoice::Unknown`], which preserves the raw JSON and re-serializes it
/// unchanged, so a stored request using a newer `tool_choice` shape round-trips
/// instead of being rewritten to `{"type":"unknown"}`.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::ToolChoice;
///
/// assert_eq!(
///     serde_json::to_value(ToolChoice::auto()).unwrap(),
///     serde_json::json!({"type": "auto"})
/// );
/// assert_eq!(
///     serde_json::to_value(ToolChoice::tool("get_weather")).unwrap(),
///     serde_json::json!({"type": "tool", "name": "get_weather"})
/// );
///
/// // A newer tool_choice shape is preserved verbatim.
/// let novel = serde_json::json!({"type": "future_choice", "name": "x"});
/// let parsed: ToolChoice = serde_json::from_value(novel.clone()).unwrap();
/// assert_eq!(serde_json::to_value(&parsed).unwrap(), novel);
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ToolChoice {
    /// The model decides whether and which tools to call.
    Auto {
        /// Force at most one tool call this turn.
        disable_parallel_tool_use: Option<bool>,
    },
    /// The model must call at least one tool.
    Any {
        /// Force at most one tool call this turn.
        disable_parallel_tool_use: Option<bool>,
    },
    /// The model must not call any tool.
    None {},
    /// The model must call the named tool.
    Tool {
        /// The tool the model is required to call.
        name: String,
        /// Force at most one tool call this turn.
        disable_parallel_tool_use: Option<bool>,
    },
    /// Forward-compatible catch-all preserving the raw JSON of an unknown
    /// `type` value.
    Unknown(serde_json::Value),
}

impl Serialize for ToolChoice {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ToolChoice::Auto {
                disable_parallel_tool_use,
            }
            | ToolChoice::Any {
                disable_parallel_tool_use,
            } => {
                let tag = if matches!(self, ToolChoice::Auto { .. }) {
                    "auto"
                } else {
                    "any"
                };
                let len = 1 + usize::from(disable_parallel_tool_use.is_some());
                let mut map = serializer.serialize_map(Some(len))?;
                map.serialize_entry("type", tag)?;
                if let Some(flag) = disable_parallel_tool_use {
                    map.serialize_entry("disable_parallel_tool_use", flag)?;
                }
                map.end()
            }
            ToolChoice::None {} => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "none")?;
                map.end()
            }
            ToolChoice::Tool {
                name,
                disable_parallel_tool_use,
            } => {
                let len = 2 + usize::from(disable_parallel_tool_use.is_some());
                let mut map = serializer.serialize_map(Some(len))?;
                map.serialize_entry("type", "tool")?;
                map.serialize_entry("name", name)?;
                if let Some(flag) = disable_parallel_tool_use {
                    map.serialize_entry("disable_parallel_tool_use", flag)?;
                }
                map.end()
            }
            ToolChoice::Unknown(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ToolChoice {
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
        struct Parallel {
            #[serde(default)]
            disable_parallel_tool_use: Option<bool>,
        }
        #[derive(Deserialize)]
        struct ToolFields {
            name: String,
            #[serde(default)]
            disable_parallel_tool_use: Option<bool>,
        }

        let choice = match tag.as_deref() {
            Some("auto") => {
                let r: Parallel = serde_json::from_value(value).map_err(de::Error::custom)?;
                ToolChoice::Auto {
                    disable_parallel_tool_use: r.disable_parallel_tool_use,
                }
            }
            Some("any") => {
                let r: Parallel = serde_json::from_value(value).map_err(de::Error::custom)?;
                ToolChoice::Any {
                    disable_parallel_tool_use: r.disable_parallel_tool_use,
                }
            }
            Some("none") => ToolChoice::None {},
            Some("tool") => {
                let r: ToolFields = serde_json::from_value(value).map_err(de::Error::custom)?;
                ToolChoice::Tool {
                    name: r.name,
                    disable_parallel_tool_use: r.disable_parallel_tool_use,
                }
            }
            _ => ToolChoice::Unknown(value),
        };
        Ok(choice)
    }
}

impl ToolChoice {
    /// `{"type": "auto"}` — the model decides.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ToolChoice;
    /// assert!(matches!(ToolChoice::auto(), ToolChoice::Auto { .. }));
    /// ```
    pub fn auto() -> Self {
        ToolChoice::Auto {
            disable_parallel_tool_use: None,
        }
    }

    /// `{"type": "any"}` — the model must use a tool.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ToolChoice;
    /// assert!(matches!(ToolChoice::any(), ToolChoice::Any { .. }));
    /// ```
    pub fn any() -> Self {
        ToolChoice::Any {
            disable_parallel_tool_use: None,
        }
    }

    /// `{"type": "none"}` — the model must not use a tool.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ToolChoice;
    /// assert!(matches!(ToolChoice::none(), ToolChoice::None {}));
    /// ```
    pub fn none() -> Self {
        ToolChoice::None {}
    }

    /// `{"type": "tool", "name": ...}` — the model must use the named tool.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::types::ToolChoice;
    /// assert!(matches!(ToolChoice::tool("t"), ToolChoice::Tool { .. }));
    /// ```
    pub fn tool(name: impl Into<String>) -> Self {
        ToolChoice::Tool {
            name: name.into(),
            disable_parallel_tool_use: None,
        }
    }
}

/// The `content` of a [`crate::types::content::ToolResultBlockParam`]: either a
/// plain string or an array of `text`/`image` content blocks.
///
/// # Examples
///
/// ```
/// use crimson_crab::types::{ContentBlockParam, ToolResultContent};
///
/// let s: ToolResultContent = "72F and sunny".into();
/// assert_eq!(serde_json::to_value(&s).unwrap(), serde_json::json!("72F and sunny"));
///
/// let blocks: ToolResultContent = vec![ContentBlockParam::text("see image")].into();
/// assert!(serde_json::to_value(&blocks).unwrap().is_array());
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// A plain string result.
    Text(String),
    /// An array of content blocks (typically `text` and `image`).
    Blocks(Vec<ContentBlockParam>),
}

impl From<String> for ToolResultContent {
    fn from(s: String) -> Self {
        ToolResultContent::Text(s)
    }
}

impl From<&str> for ToolResultContent {
    fn from(s: &str) -> Self {
        ToolResultContent::Text(s.to_string())
    }
}

impl From<Vec<ContentBlockParam>> for ToolResultContent {
    fn from(blocks: Vec<ContentBlockParam>) -> Self {
        ToolResultContent::Blocks(blocks)
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

    // Fixture: custom tool definition (docs/wire-api.md "Tools").
    #[test]
    fn custom_tool_definition_round_trips() {
        let j = serde_json::json!({
            "name": "get_weather",
            "description": "Get current weather for a location",
            "input_schema": {
                "type": "object",
                "properties": {"location": {"type": "string"}},
                "required": ["location"]
            },
            "strict": true,
            "cache_control": {"type": "ephemeral"}
        });
        assert_eq!(rt::<Tool>(j.clone()), j);
    }

    // Fixture: a custom tool as a ToolUnion stays Custom.
    #[test]
    fn tool_union_custom() {
        let j = serde_json::json!({
            "name": "noop",
            "description": "does nothing",
            "input_schema": {"type": "object"}
        });
        let parsed: ToolUnion = serde_json::from_value(j.clone()).unwrap();
        assert!(matches!(parsed, ToolUnion::Custom(_)));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), j);
    }

    // Fixture: server tool passthrough (docs/wire-api.md "Server tools").
    #[test]
    fn tool_union_server_tool_is_raw() {
        let j = serde_json::json!({"type": "web_search_20260209", "name": "web_search"});
        let parsed: ToolUnion = serde_json::from_value(j.clone()).unwrap();
        assert!(matches!(parsed, ToolUnion::Raw(_)));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), j);
    }

    // Fixture: tool_choice variants (docs/wire-api.md).
    #[test]
    fn tool_choice_variants_round_trip() {
        for j in [
            serde_json::json!({"type": "auto"}),
            serde_json::json!({"type": "any"}),
            serde_json::json!({"type": "none"}),
            serde_json::json!({"type": "tool", "name": "get_weather"}),
            serde_json::json!({"type": "auto", "disable_parallel_tool_use": true}),
        ] {
            assert_eq!(rt::<ToolChoice>(j.clone()), j);
        }
    }

    #[test]
    fn unknown_tool_choice_is_preserved() {
        // A newer tool_choice shape must round-trip verbatim, not collapse to
        // `{"type":"unknown"}`.
        let j = serde_json::json!({"type": "future_choice", "name": "x"});
        let parsed: ToolChoice = serde_json::from_value(j.clone()).unwrap();
        assert!(matches!(parsed, ToolChoice::Unknown(_)));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), j);
    }

    #[test]
    fn tool_result_content_string_and_blocks() {
        let s = serde_json::json!("done");
        assert_eq!(rt::<ToolResultContent>(s.clone()), s);

        let blocks = serde_json::json!([{"type": "text", "text": "done"}]);
        assert_eq!(rt::<ToolResultContent>(blocks.clone()), blocks);
    }
}
