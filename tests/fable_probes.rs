//! Adversarial probes from the pre-release deep review: retry bounding under a
//! permanently-failing server, hostile `retry-after` values, and full-surface
//! request serialization.

use std::time::{Duration, Instant};

use crimson_crab::prelude::*;
use crimson_crab::types::{CacheControl, SystemPrompt, ThinkingConfig, Tool, ToolChoice};
use crimson_crab::Error;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(server: &MockServer, max_retries: u32) -> Client {
    Client::builder()
        .api_key("sk-ant-test")
        .base_url(server.uri())
        .max_retries(max_retries)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client")
}

fn minimal_request() -> MessagesRequest {
    MessagesRequest::builder()
        .model("claude-opus-4-8")
        .max_tokens(64)
        .messages(vec![MessageParam::user("hi")])
        .build()
        .expect("request")
}

/// A server that fails with 529 forever must exhaust `max_retries` and return a
/// typed error — never spin, never hang.
#[tokio::test]
async fn infinite_529_is_bounded_by_max_retries() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(529).set_body_string(
            r#"{"type":"error","error":{"type":"overloaded_error","message":"overloaded"}}"#,
        ))
        .expect(3) // initial attempt + exactly 2 retries
        .mount(&server)
        .await;

    let client = client_for(&server, 2);
    let err = client
        .messages()
        .create(&minimal_request())
        .await
        .expect_err("must fail after retries");
    assert!(err.is_retryable(), "529 maps to a retryable typed error");
    server.verify().await;
}

/// A hostile `retry-after` (hours) must NOT park the retry loop: the value is
/// capped, so the whole call completes in normal-backoff time.
#[tokio::test]
async fn absurd_retry_after_does_not_hang() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "99999999")
                .set_body_string(
                    r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#,
                ),
        )
        .mount(&server)
        .await;

    let client = client_for(&server, 1);
    let started = Instant::now();
    let err = client
        .messages()
        .create(&minimal_request())
        .await
        .expect_err("rate limited");
    // Full-jitter backoff for one retry is at most ~0.5s; anything under 10s
    // proves the absurd header was ignored rather than slept on.
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "retry loop parked on hostile retry-after: {:?}",
        started.elapsed()
    );
    assert!(matches!(err, Error::RateLimit { .. }));
}

/// Serializes a request exercising the full option surface and checks the exact
/// wire JSON for the fields the API contract cares about.
#[test]
fn maximal_request_serializes_to_spec() {
    let request = MessagesRequest::builder()
        .model("claude-opus-4-8")
        .max_tokens(1024)
        .system(SystemPrompt::from("be terse"))
        .messages(vec![
            MessageParam::user("hello"),
            MessageParam::assistant("hi, how can I help?"),
            MessageParam::user("call the tool"),
        ])
        .tools(vec![Tool::new(
            "get_weather",
            "Get weather",
            serde_json::json!({
                "type": "object",
                "properties": {"location": {"type": "string"}},
                "required": ["location"]
            }),
        )
        .into()])
        .tool_choice(ToolChoice::Auto {
            disable_parallel_tool_use: None,
        })
        .thinking(ThinkingConfig::adaptive())
        .stop_sequences(vec!["END".to_string()])
        .cache_control(CacheControl::ephemeral())
        .betas(vec!["files-api-2025-04-14".to_string()])
        .build()
        .expect("request");

    let value = serde_json::to_value(&request).expect("serialize");
    let object = value.as_object().expect("object");

    assert_eq!(object["model"], "claude-opus-4-8");
    assert_eq!(object["max_tokens"], 1024);
    assert_eq!(object["system"], "be terse");
    assert_eq!(object["messages"].as_array().expect("messages").len(), 3);
    assert_eq!(object["tools"][0]["name"], "get_weather");
    assert_eq!(object["tool_choice"]["type"], "auto");
    assert_eq!(object["thinking"]["type"], "adaptive");
    assert_eq!(object["stop_sequences"][0], "END");
    assert_eq!(object["cache_control"]["type"], "ephemeral");
    // `betas` must travel as a header, never in the body.
    assert!(
        !object.contains_key("betas"),
        "betas leaked into the request body"
    );
    // Absent optionals must be omitted entirely, not serialized as null.
    for key in ["temperature", "top_p", "top_k", "metadata", "stream"] {
        assert!(!object.contains_key(key), "{key} serialized when unset");
    }
}
