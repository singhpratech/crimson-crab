//! Wiremock integration tests for the tool runner: the conversation it builds
//! across turns, parallel tool calls, the three ways a tool call can fail
//! without ending the run, and the turn cap.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crimson_crab::api::MessagesRequest;
use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::types::{MessageParam, StopReason, Tool};
use crimson_crab::{Client, Error};
use serde::Deserialize;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The arguments of the `get_weather` tool.
#[derive(Debug, Deserialize)]
struct GetWeather {
    city: String,
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
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(1024)
        .messages(vec![MessageParam::user("What's the weather in Paris?")])
        .build()
        .expect("request builds")
}

fn weather_tool() -> Tool {
    Tool::new(
        "get_weather",
        "Get the current weather for a city",
        json!({
            "type": "object",
            "properties": {"city": {"type": "string"}},
            "required": ["city"]
        }),
    )
}

/// A response whose content is the given blocks.
fn message_with(blocks: serde_json::Value, stop_reason: &str) -> serde_json::Value {
    json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-opus-4-8",
        "content": blocks,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {"input_tokens": 9, "output_tokens": 7}
    })
}

/// A `tool_use`-stopped response with a single call.
fn tool_use_response(id: &str, name: &str, input: serde_json::Value) -> serde_json::Value {
    message_with(
        json!([{"type": "tool_use", "id": id, "name": name, "input": input}]),
        "tool_use",
    )
}

/// A final `end_turn` response.
fn end_turn_response(text: &str) -> serde_json::Value {
    message_with(json!([{"type": "text", "text": text}]), "end_turn")
}

/// Mounts `bodies` as an ordered sequence: the first request gets `bodies[0]`,
/// the second `bodies[1]`, and so on. Lower wiremock priorities match first, and
/// each mock retires after one match, so the sequence advances by itself.
async fn mount_sequence(server: &MockServer, bodies: Vec<serde_json::Value>) {
    for (index, body) in bodies.into_iter().enumerate() {
        let priority = u8::try_from(index + 1).expect("fewer than 255 responses");
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .up_to_n_times(1)
            .with_priority(priority)
            .mount(server)
            .await;
    }
}

/// The JSON bodies of every request the server received, in order.
async fn sent_bodies(server: &MockServer) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .expect("request recording is enabled")
        .iter()
        .map(|request| serde_json::from_slice(&request.body).expect("request body is JSON"))
        .collect()
}

#[tokio::test]
async fn two_turn_run_sends_assistant_turn_and_tool_result() {
    let server = MockServer::start().await;
    mount_sequence(
        &server,
        vec![
            tool_use_response("toolu_1", "get_weather", json!({"city": "Paris"})),
            end_turn_response("It is 22C in Paris."),
        ],
    )
    .await;

    let result = client_for(&server)
        .messages()
        .runner(request())
        .tool(weather_tool(), |args: GetWeather| async move {
            Ok::<_, String>(format!("22C in {}", args.city))
        })
        .run()
        .await
        .expect("the run completes");

    assert_eq!(result.turns, 2);
    assert_eq!(result.message.stop_reason, Some(StopReason::EndTurn));
    assert_eq!(result.message.text(), "It is 22C in Paris.");
    // user → assistant(tool_use) → user(tool_result) → assistant(final).
    assert_eq!(result.messages.len(), 4);

    let bodies = sent_bodies(&server).await;
    assert_eq!(bodies.len(), 2);

    // `.tool()` is the single source of truth for the tool definition.
    assert_eq!(bodies[0]["tools"][0]["name"], json!("get_weather"));
    assert_eq!(bodies[0]["messages"].as_array().map(Vec::len), Some(1));

    // The second request replays the assistant turn verbatim, then answers it.
    let messages = bodies[1]["messages"]
        .as_array()
        .expect("messages is an array");
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["role"], json!("assistant"));
    assert_eq!(
        messages[1]["content"][0],
        json!({
            "type": "tool_use",
            "id": "toolu_1",
            "name": "get_weather",
            "input": {"city": "Paris"}
        })
    );
    assert_eq!(messages[2]["role"], json!("user"));
    assert_eq!(
        messages[2]["content"][0],
        json!({
            "type": "tool_result",
            "tool_use_id": "toolu_1",
            "content": "22C in Paris"
        })
    );
}

#[tokio::test]
async fn parallel_tool_calls_are_all_answered_in_one_message_in_order() {
    let server = MockServer::start().await;
    mount_sequence(
        &server,
        vec![
            message_with(
                json!([
                    {"type": "text", "text": "Looking both up."},
                    {"type": "tool_use", "id": "toolu_slow", "name": "get_weather",
                     "input": {"city": "Paris"}},
                    {"type": "tool_use", "id": "toolu_fast", "name": "get_time",
                     "input": {"city": "Tokyo"}}
                ]),
                "tool_use",
            ),
            end_turn_response("Done."),
        ],
    )
    .await;

    let time_tool = Tool::new("get_time", "Get the local time", json!({"type": "object"}));

    let result = client_for(&server)
        .messages()
        .runner(request())
        // The first call finishes last, so a result order that follows
        // completion rather than the wire would be caught here.
        .tool(weather_tool(), |args: GetWeather| async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            Ok::<_, String>(format!("22C in {}", args.city))
        })
        .tool_raw(time_tool, |input: serde_json::Value| async move {
            Ok::<_, String>(json!({"city": input["city"], "time": "09:00"}))
        })
        .run()
        .await
        .expect("the run completes");

    assert_eq!(result.turns, 2);

    let bodies = sent_bodies(&server).await;
    assert_eq!(bodies[0]["tools"].as_array().map(Vec::len), Some(2));

    let messages = bodies[1]["messages"]
        .as_array()
        .expect("messages is an array");
    // Both results ride in a single user message, in wire order.
    assert_eq!(messages.len(), 3);
    let results = messages[2]["content"]
        .as_array()
        .expect("content is an array");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["tool_use_id"], json!("toolu_slow"));
    assert_eq!(results[0]["content"], json!("22C in Paris"));
    assert_eq!(results[1]["tool_use_id"], json!("toolu_fast"));
    // A non-string result is sent as its JSON text.
    assert_eq!(
        results[1]["content"],
        json!(r#"{"city":"Tokyo","time":"09:00"}"#)
    );
}

#[tokio::test]
async fn handler_error_becomes_an_error_result_and_the_run_continues() {
    let server = MockServer::start().await;
    mount_sequence(
        &server,
        vec![
            tool_use_response("toolu_1", "get_weather", json!({"city": "Paris"})),
            end_turn_response("I could not reach the weather service."),
        ],
    )
    .await;

    let result = client_for(&server)
        .messages()
        .runner(request())
        .tool(weather_tool(), |_args: GetWeather| async move {
            Err::<String, _>("upstream weather API is down")
        })
        .run()
        .await
        .expect("a failing tool does not fail the run");

    assert_eq!(result.turns, 2);
    assert_eq!(result.message.stop_reason, Some(StopReason::EndTurn));

    let bodies = sent_bodies(&server).await;
    let error_result = &bodies[1]["messages"][2]["content"][0];
    assert_eq!(error_result["type"], json!("tool_result"));
    assert_eq!(error_result["tool_use_id"], json!("toolu_1"));
    assert_eq!(error_result["is_error"], json!(true));
    assert_eq!(
        error_result["content"],
        json!("upstream weather API is down")
    );
}

#[tokio::test]
async fn unknown_tool_name_becomes_an_error_result_listing_registered_tools() {
    let server = MockServer::start().await;
    mount_sequence(
        &server,
        vec![
            tool_use_response("toolu_1", "get_stock_price", json!({"ticker": "ANTH"})),
            end_turn_response("I do not have that tool."),
        ],
    )
    .await;

    let result = client_for(&server)
        .messages()
        .runner(request())
        .tool(weather_tool(), |args: GetWeather| async move {
            Ok::<_, String>(format!("22C in {}", args.city))
        })
        .run()
        .await
        .expect("an unknown tool name does not fail the run");

    assert_eq!(result.turns, 2);

    let bodies = sent_bodies(&server).await;
    let error_result = &bodies[1]["messages"][2]["content"][0];
    assert_eq!(error_result["tool_use_id"], json!("toolu_1"));
    assert_eq!(error_result["is_error"], json!(true));
    let content = error_result["content"]
        .as_str()
        .expect("tool_result content is text");
    assert!(content.contains("get_stock_price"), "{content}");
    assert!(content.contains("get_weather"), "{content}");
}

#[tokio::test]
async fn undeserializable_input_becomes_an_error_result_naming_the_type() {
    let server = MockServer::start().await;
    mount_sequence(
        &server,
        vec![
            // `city` is a number, not a string: `GetWeather` cannot parse it.
            tool_use_response("toolu_1", "get_weather", json!({"city": 42})),
            end_turn_response("Let me try again."),
        ],
    )
    .await;

    let result = client_for(&server)
        .messages()
        .runner(request())
        .tool(weather_tool(), |args: GetWeather| async move {
            Ok::<_, String>(format!("22C in {}", args.city))
        })
        .run()
        .await
        .expect("a bad input does not fail the run");

    assert_eq!(result.turns, 2);

    let bodies = sent_bodies(&server).await;
    let error_result = &bodies[1]["messages"][2]["content"][0];
    assert_eq!(error_result["is_error"], json!(true));
    let content = error_result["content"]
        .as_str()
        .expect("tool_result content is text");
    assert!(content.contains("GetWeather"), "{content}");
    assert!(content.contains("invalid type"), "{content}");
}

#[tokio::test]
async fn max_turns_exceeded_returns_tool_runner_error_after_exactly_max_turns() {
    let server = MockServer::start().await;
    // Every response asks for the tool again: the model never settles.
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tool_use_response(
            "toolu_1",
            "get_weather",
            json!({"city": "Paris"}),
        )))
        .mount(&server)
        .await;

    let outcome = client_for(&server)
        .messages()
        .runner(request())
        .tool(weather_tool(), |args: GetWeather| async move {
            Ok::<_, String>(format!("22C in {}", args.city))
        })
        .max_turns(2)
        .run()
        .await;

    match outcome {
        Err(Error::ToolRunner { message, turns }) => {
            assert_eq!(turns, 2);
            assert!(message.contains("get_weather"), "{message}");
        }
        other => panic!("expected Error::ToolRunner, got {other:?}"),
    }

    assert_eq!(sent_bodies(&server).await.len(), 2);
}

#[tokio::test]
async fn zero_max_turns_is_a_config_error_and_sends_nothing() {
    let server = MockServer::start().await;

    match client_for(&server)
        .messages()
        .runner(request())
        .max_turns(0)
        .run()
        .await
    {
        Err(Error::Config(message)) => assert!(message.contains("max_turns"), "{message}"),
        other => panic!("expected Error::Config, got {other:?}"),
    }

    assert!(sent_bodies(&server).await.is_empty());
}

#[tokio::test]
async fn on_turn_observes_every_response_before_tools_run() {
    let server = MockServer::start().await;
    mount_sequence(
        &server,
        vec![
            tool_use_response("toolu_1", "get_weather", json!({"city": "Paris"})),
            end_turn_response("It is 22C in Paris."),
        ],
    )
    .await;

    // The counter is shared with the handler so the ordering assertion below
    // ("the hook ran before the tool did") is meaningful.
    static SEEN_BEFORE_TOOL: AtomicUsize = AtomicUsize::new(0);
    SEEN_BEFORE_TOOL.store(0, Ordering::SeqCst);
    let mut stop_reasons = Vec::new();

    let result = client_for(&server)
        .messages()
        .runner(request())
        .tool(weather_tool(), |args: GetWeather| async move {
            let seen = SEEN_BEFORE_TOOL.load(Ordering::SeqCst);
            Ok::<_, String>(format!("22C in {} (hook had run {seen}x)", args.city))
        })
        .on_turn(|message| {
            SEEN_BEFORE_TOOL.fetch_add(1, Ordering::SeqCst);
            stop_reasons.push(message.stop_reason.clone());
        })
        .run()
        .await
        .expect("the run completes");

    assert_eq!(result.turns, 2);
    assert_eq!(
        stop_reasons,
        vec![Some(StopReason::ToolUse), Some(StopReason::EndTurn)]
    );

    let bodies = sent_bodies(&server).await;
    assert_eq!(
        bodies[1]["messages"][2]["content"][0]["content"],
        json!("22C in Paris (hook had run 1x)")
    );
}

#[tokio::test]
async fn a_non_tool_use_stop_reason_ends_the_run_without_erroring() {
    // A refusal is returned as-is: the runner is lower level than `parse()` and
    // leaves the verdict to the caller.
    let server = MockServer::start().await;
    mount_sequence(&server, vec![message_with(json!([]), "refusal")]).await;

    let result = client_for(&server)
        .messages()
        .runner(request())
        .tool(weather_tool(), |args: GetWeather| async move {
            Ok::<_, String>(format!("22C in {}", args.city))
        })
        .run()
        .await
        .expect("a refusal is a result, not an error");

    assert_eq!(result.turns, 1);
    assert_eq!(result.message.stop_reason, Some(StopReason::Refusal));
    assert_eq!(result.messages.len(), 2);
}

#[tokio::test]
async fn registering_a_name_twice_replaces_the_tool_rather_than_duplicating_it() {
    let server = MockServer::start().await;
    mount_sequence(&server, vec![end_turn_response("Nothing to do.")]).await;

    let replacement = Tool::new(
        "get_weather",
        "Get the current weather (v2)",
        json!({"type": "object"}),
    );

    client_for(&server)
        .messages()
        .runner(request())
        .tool(weather_tool(), |args: GetWeather| async move {
            Ok::<_, String>(format!("22C in {}", args.city))
        })
        .tool_raw(replacement, |_input: serde_json::Value| async move {
            Ok::<_, String>("v2")
        })
        .run()
        .await
        .expect("the run completes");

    let bodies = sent_bodies(&server).await;
    let tools = bodies[0]["tools"].as_array().expect("tools is an array");
    assert_eq!(tools.len(), 1);
    assert_eq!(
        tools[0]["description"],
        json!("Get the current weather (v2)")
    );
}
