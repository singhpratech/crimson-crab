//! JSON Schema generation from Rust types (the `schemars` feature).
//!
//! Two callers share this module: [`crate::api::messages::Messages::parse`],
//! which needs the *strict* schema shape `output_config.format` requires, and
//! [`crate::types::Tool::from_type`], which needs the schema exactly as
//! `schemars` emits it. [`generate`] is therefore the common half, and
//! [`strictify`] the extra pass `parse` applies on top.
//!
//! Subschemas are inlined: the structured-output schema subset is defined in
//! terms of a single self-contained schema object, and inlining also means
//! neither caller has to resolve `$ref`s against `$defs` afterwards. The
//! exception is a **recursive** type, which has no finite inlined form —
//! `schemars` falls back to `$defs`/`$ref` there, detectable via
//! [`contains_ref`].

use schemars::generate::SchemaSettings;
use schemars::JsonSchema;

/// Generates `T`'s JSON Schema with subschemas inlined.
///
/// The `$schema` meta-keyword `schemars` puts on the root is removed: it
/// declares a dialect rather than constraining the value, and the API's schema
/// field does not expect it. `title` and `description` (the latter derived from
/// doc comments) are kept — they are annotations the model reads.
///
/// A **recursive** type has no finite inlined form; for those, `schemars`
/// falls back to emitting `$defs`/`$ref` rather than recursing forever. Use
/// [`contains_ref`] to detect that case where a self-contained schema is
/// required (structured output does not accept references).
pub(crate) fn generate<T: JsonSchema>() -> serde_json::Value {
    let settings = SchemaSettings::default().with(|settings| settings.inline_subschemas = true);
    let mut schema = settings
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value();
    if let Some(object) = schema.as_object_mut() {
        object.remove("$schema");
    }
    schema
}

/// Reports whether the schema still carries a `$ref` or `$defs` keyword — the
/// fallback `schemars` uses for a **recursive** type, which cannot be inlined.
///
/// Property *names* do not count: the keys of a `properties` map are data, not
/// schema keywords, so a field literally named `$ref` is not a reference.
pub(crate) fn contains_ref(schema: &serde_json::Value) -> bool {
    match schema {
        serde_json::Value::Object(object) => {
            if object.contains_key("$ref") || object.contains_key("$defs") {
                return true;
            }
            object
                .iter()
                .any(|(key, value)| match (key.as_str(), value) {
                    ("properties", serde_json::Value::Object(properties)) => {
                        properties.values().any(contains_ref)
                    }
                    _ => contains_ref(value),
                })
        }
        serde_json::Value::Array(items) => items.iter().any(contains_ref),
        _ => false,
    }
}

/// Rewrites a schema in place into the strict shape `output_config.format`
/// expects, recursing through every nested subschema.
///
/// Two transforms are applied to each object schema (any node carrying a
/// `properties` map):
///
/// 1. `"additionalProperties": false` is set, so the model cannot invent
///    fields.
/// 2. Every property is listed in `required`. `schemars` omits `Option<T>`
///    fields from `required` and instead widens their `type` to include
///    `"null"`, so a required-but-nullable property still deserializes into
///    `Option<T>` — which is what makes "everything is required" safe.
///
/// `required` is emitted in the order the schema's `properties` map iterates,
/// so it stays stable for a given schema.
pub(crate) fn strictify(schema: &mut serde_json::Value) {
    match schema {
        serde_json::Value::Object(object) => {
            if let Some(serde_json::Value::Object(properties)) = object.get("properties") {
                let required: Vec<serde_json::Value> = properties
                    .keys()
                    .map(|name| serde_json::Value::String(name.clone()))
                    .collect();
                object.insert("required".to_string(), serde_json::Value::Array(required));
                object.insert(
                    "additionalProperties".to_string(),
                    serde_json::Value::Bool(false),
                );
            }
            // Recurse into every nested value. Subschemas live under a handful
            // of keywords (`items`, `anyOf`, …), but walking all values is both
            // simpler and future-proof: keywords whose values are not schemas
            // (`type`, `required`, `enum`, …) hold strings, arrays of strings,
            // or booleans, none of which carry a `properties` map, so visiting
            // them is a no-op. The one exception is `properties` itself: its
            // value is a map *of* schemas, not a schema — treating it as one
            // would inject phantom entries whenever a property is literally
            // named "properties" — so descend straight into its values.
            for (key, value) in object.iter_mut() {
                match (key.as_str(), &mut *value) {
                    ("properties", serde_json::Value::Object(properties)) => {
                        for subschema in properties.values_mut() {
                            strictify(subschema);
                        }
                    }
                    _ => strictify(value),
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strictify(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `required` list of a schema, sorted so the assertion does not depend
    /// on how the underlying JSON object orders its keys.
    fn required(schema: &serde_json::Value) -> Vec<String> {
        let mut names: Vec<String> = schema["required"]
            .as_array()
            .expect("required is an array")
            .iter()
            .map(|name| {
                name.as_str()
                    .expect("required entry is a string")
                    .to_string()
            })
            .collect();
        names.sort();
        names
    }

    /// A postal address.
    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct Address {
        street: String,
        zip: Option<String>,
    }

    /// Extracted contact details.
    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct Contact {
        /// The contact's full name.
        name: String,
        company: Option<String>,
        tags: Vec<String>,
        addresses: Vec<Address>,
        home: Option<Address>,
    }

    #[test]
    fn generated_schema_is_inlined_and_has_no_meta_keyword() {
        let schema = generate::<Contact>();
        assert!(schema.get("$schema").is_none(), "$schema must be stripped");
        assert!(schema.get("$defs").is_none(), "subschemas must be inlined");
        assert!(
            !serde_json::to_string(&schema)
                .expect("schema serializes")
                .contains("$ref"),
            "no $ref may survive inlining"
        );
        // Doc comments become descriptions the model can read.
        assert_eq!(
            schema["properties"]["name"]["description"],
            serde_json::json!("The contact's full name.")
        );
    }

    #[test]
    fn strictify_handles_nested_structs_vecs_and_options() {
        let mut schema = generate::<Contact>();
        strictify(&mut schema);

        // (a) every object schema denies extra properties — including the ones
        // nested inside a `Vec` and inside an `Option`.
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        assert_eq!(
            schema["properties"]["addresses"]["items"]["additionalProperties"],
            serde_json::json!(false)
        );
        assert_eq!(
            schema["properties"]["home"]["additionalProperties"],
            serde_json::json!(false)
        );

        // (b) every property is required at every level — including the ones
        // `schemars` would have left optional because they are `Option<T>`.
        // (Compared as a set: the order follows the `properties` map's.)
        assert_eq!(
            required(&schema),
            ["addresses", "company", "home", "name", "tags"]
        );
        assert_eq!(
            required(&schema["properties"]["addresses"]["items"]),
            ["street", "zip"]
        );

        // …which is only sound because `Option<T>` is nullable in the schema.
        assert_eq!(
            schema["properties"]["company"]["type"],
            serde_json::json!(["string", "null"])
        );
        assert_eq!(
            schema["properties"]["home"]["type"],
            serde_json::json!(["object", "null"])
        );
        assert_eq!(
            schema["properties"]["addresses"]["items"]["properties"]["zip"]["type"],
            serde_json::json!(["string", "null"])
        );

        // Non-schema keywords are left alone.
        assert_eq!(schema["type"], serde_json::json!("object"));
        assert_eq!(
            schema["properties"]["tags"]["items"]["type"],
            serde_json::json!("string")
        );
    }

    #[test]
    fn contains_ref_detects_recursive_types() {
        /// A tree node that contains more tree nodes.
        #[derive(JsonSchema)]
        #[allow(dead_code)]
        struct Node {
            label: String,
            children: Vec<Node>,
        }

        let schema = generate::<Node>();
        assert!(contains_ref(&schema), "recursive types fall back to $ref");
        assert!(!contains_ref(&generate::<Contact>()));
    }

    #[test]
    fn contains_ref_ignores_a_property_named_ref() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "$ref": {"type": "string"},
                "$defs": {"type": "integer"}
            }
        });
        assert!(
            !contains_ref(&schema),
            "property names are data, not keywords"
        );
    }

    #[test]
    fn strictify_is_idempotent() {
        let mut once = generate::<Contact>();
        strictify(&mut once);
        let mut twice = once.clone();
        strictify(&mut twice);
        assert_eq!(once, twice);
    }

    #[test]
    fn strictify_survives_a_property_named_properties() {
        /// A struct whose field is literally named `properties`.
        #[derive(JsonSchema)]
        #[allow(dead_code)]
        struct Widget {
            properties: Address,
        }

        let mut schema = generate::<Widget>();
        strictify(&mut schema);

        // The properties *map* must hold exactly the declared field — no
        // phantom `required`/`additionalProperties` entries injected into it.
        let map = schema["properties"]
            .as_object()
            .expect("properties is a map");
        assert_eq!(map.keys().collect::<Vec<_>>(), ["properties"]);

        // And the field's own schema is still strictified.
        assert_eq!(
            schema["properties"]["properties"]["additionalProperties"],
            serde_json::json!(false)
        );
        assert_eq!(
            required(&schema["properties"]["properties"]),
            ["street", "zip"]
        );
    }

    #[test]
    fn strictify_leaves_schemas_without_properties_untouched() {
        let mut schema = serde_json::json!({"type": "string", "enum": ["a", "b"]});
        let before = schema.clone();
        strictify(&mut schema);
        assert_eq!(schema, before);
    }
}
