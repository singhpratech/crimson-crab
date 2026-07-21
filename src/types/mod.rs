//! Wire types for the Anthropic Claude API.
//!
//! Every type in this module mirrors the JSON shapes documented in
//! `docs/wire-api.md` as closely as Rust allows. The guiding principles are:
//!
//! * **Wire fidelity.** Field names, tags, and nesting match the API exactly.
//! * **Forward compatibility.** Every enum that is discriminated by a `type`
//!   tag (or by a bare string value) carries a catch-all variant, so that
//!   content blocks, stop reasons, and other values the SDK has never seen
//!   still deserialize instead of erroring. See the module-level notes on
//!   `tagged_enum` and `string_enum` for the two mechanisms used.
//!
//! Request-shaped types (those sent *to* the API) apply
//! `#[serde(skip_serializing_if = "Option::is_none")]` to their optional
//! fields; response-shaped types tolerate missing fields on the way in.

pub mod cache;
pub mod content;
pub mod message;
pub mod output;
pub mod thinking;
pub mod tool;

pub use cache::{CacheControl, CacheTtl};
pub use content::{
    Base64Source, CitationsConfig, ContentBlock, ContentBlockParam, ContentSource,
    DocumentBlockParam, DocumentSource, FallbackBlock, FallbackRef, FileSource, ImageBlockParam,
    ImageSource, PlainTextSource, RedactedThinkingBlock, TextBlock, TextBlockParam, ThinkingBlock,
    ToolResultBlockParam, ToolUseBlock, UrlSource, WebSearchToolResultBlock,
};
pub use message::{
    Container, Message, MessageContent, MessageParam, Metadata, Role, StopDetails, StopReason,
    SystemPrompt, Usage,
};
pub use output::{Effort, OutputConfig, OutputFormat};
pub use thinking::{ThinkingConfig, ThinkingDisplay};
pub use tool::{Tool, ToolChoice, ToolResultContent, ToolUnion};

/// Serialize an internally `type`-tagged enum variant.
///
/// The known variants of a [`tagged_enum!`] enum wrap a plain struct that does
/// **not** itself carry a `type` field. This helper serializes that inner
/// struct to a JSON object and injects the variant's `type` tag, reproducing
/// the exact on-the-wire shape. Because it round-trips through
/// [`serde_json::Value`], it is JSON-specific — which is acceptable for an SDK
/// whose only transport is JSON.
pub(crate) fn serialize_tagged<T, S>(inner: &T, tag: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    T: serde::Serialize,
    S: serde::Serializer,
{
    let mut value = serde_json::to_value(inner).map_err(serde::ser::Error::custom)?;
    match &mut value {
        serde_json::Value::Object(map) => {
            map.insert(
                "type".to_string(),
                serde_json::Value::String(tag.to_string()),
            );
        }
        _ => {
            return Err(serde::ser::Error::custom(
                "tagged enum variant did not serialize to a JSON object",
            ));
        }
    }
    serde::Serialize::serialize(&value, serializer)
}

/// Define an internally `type`-tagged enum with a data-preserving catch-all.
///
/// Given a list of `Variant(InnerStruct) = "wire_tag"` arms, this expands to an
/// enum with those newtype variants plus a trailing
/// `Unknown(serde_json::Value)` variant, and hand-written [`serde::Serialize`]
/// / [`serde::Deserialize`] implementations. On deserialization the `type`
/// field selects the variant; anything unrecognised (including objects with no
/// `type` at all) is captured verbatim in `Unknown`, so the SDK never fails on
/// a block type Anthropic adds later, and can replay it back unchanged.
///
/// The inner structs must serialize to a JSON object and must **not** carry a
/// `type` field of their own — the macro owns that key.
macro_rules! tagged_enum {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$var_meta:meta])*
                $variant:ident($inner:ty) = $tag:literal
            ),* $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Clone, Debug, PartialEq)]
        #[non_exhaustive]
        $vis enum $name {
            $(
                $(#[$var_meta])*
                $variant($inner),
            )*
            /// Forward-compatible catch-all: any `type` the SDK does not model
            /// (or an object with no `type` field) is preserved here as raw
            /// JSON and re-serialized unchanged, so unknown values never error.
            Unknown(::serde_json::Value),
        }

        impl ::serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                match self {
                    $(
                        $name::$variant(inner) => {
                            $crate::types::serialize_tagged(inner, $tag, serializer)
                        }
                    )*
                    $name::Unknown(value) => ::serde::Serialize::serialize(value, serializer),
                }
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> ::core::result::Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                let value = ::serde_json::Value::deserialize(deserializer)?;
                let tag = value
                    .get("type")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_owned());
                match tag.as_deref() {
                    $(
                        ::core::option::Option::Some($tag) => ::serde_json::from_value(value)
                            .map($name::$variant)
                            .map_err(::serde::de::Error::custom),
                    )*
                    _ => ::core::result::Result::Ok($name::Unknown(value)),
                }
            }
        }
    };
}

/// Define a string-valued enum with a value-preserving catch-all.
///
/// Given `Variant = "wire_value"` arms, this expands to a fieldless enum with
/// those variants plus a trailing `Unknown(String)` variant, an `as_str`
/// accessor, and hand-written [`serde::Serialize`] / [`serde::Deserialize`]
/// implementations that map to and from the wire strings. An unrecognised
/// string is preserved verbatim in `Unknown` rather than erroring, which keeps
/// the SDK working when the API introduces a new stop reason or effort level.
///
/// This is the string-enum counterpart to [`tagged_enum!`]; `#[serde(other)]`
/// cannot be used here because it is rejected on externally tagged (bare
/// string) enums.
macro_rules! string_enum {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$var_meta:meta])*
                $variant:ident = $s:literal
            ),* $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        $vis enum $name {
            $(
                $(#[$var_meta])*
                $variant,
            )*
            /// Forward-compatible catch-all preserving the raw wire string, so
            /// a value the SDK does not model deserializes instead of erroring.
            Unknown(::std::string::String),
        }

        impl $name {
            /// Returns the wire string representation of this value.
            $vis fn as_str(&self) -> &str {
                match self {
                    $( $name::$variant => $s, )*
                    $name::Unknown(s) => s.as_str(),
                }
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> ::core::result::Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                let s = ::std::string::String::deserialize(deserializer)?;
                ::core::result::Result::Ok(match s.as_str() {
                    $( $s => $name::$variant, )*
                    _ => $name::Unknown(s),
                })
            }
        }
    };
}

pub(crate) use string_enum;
pub(crate) use tagged_enum;
