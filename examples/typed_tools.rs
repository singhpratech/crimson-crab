//! A manual agentic tool-use loop whose tool schema is derived from its
//! argument type: `Tool::from_type::<GetWeather>(...)` replaces the
//! hand-written JSON Schema in `tool_use.rs`, and the model's `input`
//! deserializes straight back into `GetWeather`.
//!
//! Requires the `schemars` feature. Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --features schemars --example typed_tools
//! ```

use crimson_crab::model_ids::CLAUDE_OPUS_5;
use crimson_crab::prelude::*;
use crimson_crab::types::ContentBlock;
use schemars::JsonSchema;
use serde::Deserialize;

/// The arguments of the `get_weather` tool.
#[derive(Debug, Deserialize, JsonSchema)]
struct GetWeather {
    /// The city to look up, e.g. "Paris".
    location: String,
    /// The temperature unit to answer in; defaults to Fahrenheit.
    unit: Option<Unit>,
}

/// A temperature unit.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Unit {
    /// Degrees Celsius.
    Celsius,
    /// Degrees Fahrenheit.
    Fahrenheit,
}

/// A stand-in for a real weather API.
fn run_get_weather(args: &GetWeather) -> String {
    let (temp, unit) = match args.unit {
        Some(Unit::Celsius) => (22, "C"),
        _ => (72, "F"),
    };
    format!(
        "{{\"location\": \"{}\", \"temp\": {temp}, \"unit\": \"{unit}\", \"conditions\": \"sunny\"}}",
        args.location
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::from_env()?;

    // The schema — including the doc comments as `description`s — comes from
    // `GetWeather`, so the tool definition cannot drift from the type that
    // parses its input.
    let weather_tool =
        Tool::from_type::<GetWeather>("get_weather", "Get the current weather for a location");

    let mut messages = vec![MessageParam::user(
        "What's the weather in Paris, in Celsius?",
    )];

    // Bound the loop so a misbehaving model cannot spin forever.
    for _ in 0..5 {
        let request = MessagesRequest::builder()
            .model(CLAUDE_OPUS_5)
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
        let mut tool_results = Vec::new();
        for block in &message.content {
            if let ContentBlock::ToolUse(call) = block {
                // A tool call whose input does not fit the type is reported back
                // to the model as an error result rather than aborting the loop.
                let result = match serde_json::from_value::<GetWeather>(call.input.clone()) {
                    Ok(args) => ContentBlockParam::tool_result(&call.id, run_get_weather(&args)),
                    Err(err) => ContentBlockParam::ToolResult(ToolResultBlockParam::error(
                        &call.id,
                        format!("invalid arguments: {err}"),
                    )),
                };
                tool_results.push(result);
            }
        }
        messages.push(message.into_param());
        messages.push(MessageParam::user(tool_results));
    }

    Ok(())
}
