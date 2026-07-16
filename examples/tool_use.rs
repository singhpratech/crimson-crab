//! A manual agentic tool-use loop: the model calls a `get_weather` tool, we run
//! it, feed the result back, and let the model produce a final answer.
//!
//! The loop follows the wire contract: when `stop_reason == tool_use`, append
//! the assistant's content verbatim, then a single user message whose content is
//! one `tool_result` block per `tool_use` id.
//!
//! Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --example tool_use
//! ```

use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::prelude::*;
use crimson_crab::types::ContentBlock;

/// A stand-in for a real weather API.
fn run_get_weather(input: &serde_json::Value) -> String {
    let location = input
        .get("location")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    format!("{{\"location\": \"{location}\", \"temp_f\": 72, \"conditions\": \"sunny\"}}")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::from_env()?;

    let weather_tool = Tool::new(
        "get_weather",
        "Get the current weather for a location",
        serde_json::json!({
            "type": "object",
            "properties": {"location": {"type": "string", "description": "City name"}},
            "required": ["location"]
        }),
    );

    let mut messages = vec![MessageParam::user("What's the weather in Paris?")];

    // Bound the loop so a misbehaving model cannot spin forever.
    for _ in 0..5 {
        let request = MessagesRequest::builder()
            .model(CLAUDE_OPUS_4_8)
            .max_tokens(1024)
            .messages(messages.clone())
            .tool(weather_tool.clone())
            .tool_choice(ToolChoice::auto())
            .build()?;

        let message = client.messages().create(&request).await?;

        if message.stop_reason != Some(StopReason::ToolUse) {
            println!("{}", message.text());
            break;
        }

        // Answer every tool call, then echo the assistant turn back verbatim.
        // `into_param()` converts the response blocks into request blocks
        // directly, with no lossy `serde_json` round-trip.
        let mut tool_results = Vec::new();
        for block in &message.content {
            if let ContentBlock::ToolUse(call) = block {
                let output = run_get_weather(&call.input);
                tool_results.push(ContentBlockParam::tool_result(&call.id, output));
            }
        }
        messages.push(message.into_param());
        messages.push(MessageParam::user(tool_results));
    }

    Ok(())
}
