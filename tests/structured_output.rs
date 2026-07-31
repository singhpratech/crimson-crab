//! Wiremock integration tests for the `schemars` feature: the request
//! `messages().parse::<T>()` sends, the value it returns, its error behavior,
//! and the schema `Tool::from_type::<T>()` derives.

#![cfg(feature = "schemars")]

use crimson_crab::api::MessagesRequest;
use crimson_crab::model_ids::CLAUDE_OPUS_5;
use crimson_crab::types::{Effort, MessageParam, OutputConfig, OutputFormat, StopReason, Tool};
use crimson_crab::{Client, Error};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A contact extracted from free-form text.
#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
struct Contact {
    /// The contact's full name.
    name: String,
    /// The employer, or `null` if none was mentioned.
    company: Option<String>,
}

fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .api_key("sk-test")
        .base_url(server.uri())
        .max_retries(0)
        .build()
        .expect("client builds")
}

fn request() -> MessagesRequest {
    MessagesRequest::builder()
        .model(CLAUDE_OPUS_5)
        .max_tokens(512)
        .messages(vec![MessageParam::user("Extract the contact.")])
        .build()
        .expect("request builds")
}

/// A message whose single text block is `text`.
fn message_with(text: &str, stop_reason: &str) -> serde_json::Value {
    json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-opus-5",
        "content": [{"type": "text", "text": text}],
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {"input_tokens": 9, "output_tokens": 7}
    })
}

async fn mount_message(server: &MockServer, body: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(server)
        .await;
}

/// The body of the single request the server received.
async fn sent_body(server: &MockServer) -> serde_json::Value {
    let requests = server
        .received_requests()
        .await
        .expect("request recording is enabled");
    assert_eq!(requests.len(), 1, "expected exactly one request");
    serde_json::from_slice(&requests[0].body).expect("request body is JSON")
}

/// The `required` list of a schema, sorted: the order follows the JSON object's
/// key order, which is not part of the contract.
fn required(schema: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = schema["required"]
        .as_array()
        .expect("required is an array")
        .iter()
        .map(|name| name.as_str().expect("string").to_string())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn parse_sends_strict_schema_and_deserializes_response() {
    let server = MockServer::start().await;
    mount_message(
        &server,
        message_with(
            r#"{"name": "Ada Lovelace", "company": "Analytical Engines"}"#,
            "end_turn",
        ),
    )
    .await;

    let parsed = client_for(&server)
        .messages()
        .parse::<Contact>(&request())
        .await
        .expect("parse succeeds");

    // The response text is deserialized into `T`, and the whole message stays
    // reachable for usage and stop-reason handling.
    assert_eq!(
        parsed.data,
        Contact {
            name: "Ada Lovelace".to_string(),
            company: Some("Analytical Engines".to_string()),
        }
    );
    assert_eq!(parsed.message.id, "msg_1");
    assert_eq!(parsed.message.usage.output_tokens, 7);
    assert_eq!(parsed.message.stop_reason, Some(StopReason::EndTurn));

    // The request carried the derived schema as `output_config.format`.
    let body = sent_body(&server).await;
    let format = &body["output_config"]["format"];
    assert_eq!(format["type"], json!("json_schema"));

    let schema = &format["schema"];
    assert_eq!(schema["type"], json!("object"));
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(required(schema), ["company", "name"]);
    // `Option<String>` is required-but-nullable, never absent.
    assert_eq!(
        schema["properties"]["company"]["type"],
        json!(["string", "null"])
    );
    // Doc comments reach the model as descriptions.
    assert_eq!(
        schema["properties"]["name"]["description"],
        json!("The contact's full name.")
    );
    // The schema is self-contained: no `$defs`, no `$ref`, no `$schema`.
    assert!(schema.get("$defs").is_none());
    assert!(schema.get("$schema").is_none());
    assert!(!schema.to_string().contains("$ref"));
}

#[tokio::test]
async fn parse_preserves_effort_and_overrides_format() {
    let server = MockServer::start().await;
    mount_message(
        &server,
        message_with(r#"{"name": "Ada", "company": null}"#, "end_turn"),
    )
    .await;

    let mut request = request();
    request.output_config = Some(OutputConfig {
        effort: Some(Effort::High),
        // Deliberately wrong: `parse` owns the format.
        format: Some(OutputFormat::json_schema(json!({"type": "string"}))),
    });

    let parsed = client_for(&server)
        .messages()
        .parse::<Contact>(&request)
        .await
        .expect("parse succeeds");
    assert_eq!(parsed.data.company, None);

    let body = sent_body(&server).await;
    assert_eq!(body["output_config"]["effort"], json!("high"));
    assert_eq!(
        body["output_config"]["format"]["schema"]["type"],
        json!("object")
    );

    // The caller's own request value is left untouched.
    let format = request
        .output_config
        .as_ref()
        .and_then(|config| config.format.as_ref())
        .expect("format still set");
    assert_eq!(
        format,
        &OutputFormat::json_schema(json!({"type": "string"}))
    );
}

#[tokio::test]
async fn parse_reports_refusal_instead_of_a_parse_failure() {
    let server = MockServer::start().await;
    mount_message(&server, message_with("", "refusal")).await;

    match client_for(&server)
        .messages()
        .parse::<Contact>(&request())
        .await
    {
        Err(Error::StructuredOutput { message, source }) => {
            assert!(
                message.contains("refused"),
                "expected a refusal summary, got {message:?}"
            );
            assert!(message.contains("msg_1"), "summary names the message");
            assert!(
                source.is_none(),
                "a refusal is not a deserialization failure"
            );
        }
        other => panic!("expected StructuredOutput for a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn parse_reports_truncated_output() {
    let server = MockServer::start().await;
    mount_message(&server, message_with(r#"{"name": "Ada"#, "max_tokens")).await;

    match client_for(&server)
        .messages()
        .parse::<Contact>(&request())
        .await
    {
        Err(Error::StructuredOutput { message, source }) => {
            assert!(
                message.contains("max_tokens"),
                "expected a truncation summary, got {message:?}"
            );
            assert!(source.is_none());
        }
        other => panic!("expected StructuredOutput for a truncated response, got {other:?}"),
    }
}

#[tokio::test]
async fn parse_reports_mismatched_text_with_the_serde_source() {
    let server = MockServer::start().await;
    // Schema-valid JSON, wrong shape for `Contact`.
    mount_message(&server, message_with(r#"{"nombre": "Ada"}"#, "end_turn")).await;

    match client_for(&server)
        .messages()
        .parse::<Contact>(&request())
        .await
    {
        Err(Error::StructuredOutput { message, source }) => {
            assert!(
                message.contains("did not match"),
                "expected a schema-mismatch summary, got {message:?}"
            );
            assert!(source.is_some(), "the serde failure is preserved");
        }
        other => panic!("expected StructuredOutput for mismatched text, got {other:?}"),
    }
}

#[test]
fn tool_from_type_derives_the_input_schema() {
    /// The arguments of the weather tool.
    #[derive(Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct GetWeather {
        /// The city to look up, e.g. "Paris".
        location: String,
        /// The number of days to forecast.
        days: Option<i32>,
    }

    let tool =
        Tool::from_type::<GetWeather>("get_weather", "Get the current weather for a location");

    assert_eq!(tool.name, "get_weather");
    assert_eq!(
        tool.description.as_deref(),
        Some("Get the current weather for a location")
    );
    assert_eq!(tool.strict, None);

    let schema = &tool.input_schema;
    assert_eq!(schema["type"], json!("object"));
    // The struct's own doc comment describes the argument object.
    assert_eq!(
        schema["description"],
        json!("The arguments of the weather tool.")
    );
    assert_eq!(
        schema["properties"]["location"]["description"],
        json!("The city to look up, e.g. \"Paris\".")
    );
    assert_eq!(schema["properties"]["location"]["type"], json!("string"));
    assert_eq!(
        schema["properties"]["days"]["type"],
        json!(["integer", "null"])
    );
    // No strictify pass: only the non-`Option` field is required, and extra
    // properties are not forbidden.
    assert_eq!(required(schema), ["location"]);
    assert!(schema.get("additionalProperties").is_none());
    // Still self-contained.
    assert!(schema.get("$schema").is_none());
    assert!(!schema.to_string().contains("$ref"));
}
