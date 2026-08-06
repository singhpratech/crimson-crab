//! The same weather agent as `typed_tools.rs`, but with the loop driven for
//! you: `messages().runner(request)` registers each tool next to the async
//! closure that implements it, and `run()` handles the round-trips.
//!
//! Note what the runner does *not* do: it hands back whatever stop reason ended
//! the conversation instead of judging it, and a tool that fails is reported to
//! the model as an errored `tool_result` so it can recover rather than aborting
//! the run.
//!
//! Requires the `schemars` feature (for `Tool::from_type`). Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --features schemars --example tool_runner
//! ```

use crimson_crab::model_ids::CLAUDE_OPUS_5;
use crimson_crab::prelude::*;
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

/// The arguments of the `get_time` tool.
#[derive(Debug, Deserialize, JsonSchema)]
struct GetTime {
    /// The IANA time zone to report, e.g. "Europe/Paris".
    time_zone: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::from_env()?;

    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_5)
        .max_tokens(1024)
        .messages(vec![MessageParam::user(
            "What's the weather in Paris in Celsius, and what time is it there?",
        )])
        .build()?;

    // Each tool is described to the model and implemented in one place. The
    // model may call both in a single turn; the runner executes them
    // concurrently and answers both in one message.
    let result = client
        .messages()
        .runner(request)
        .tool(
            Tool::from_type::<GetWeather>("get_weather", "Get the current weather for a location"),
            |args: GetWeather| async move {
                let (temp, unit) = match args.unit {
                    Some(Unit::Celsius) => (22, "C"),
                    _ => (72, "F"),
                };
                Ok::<_, String>(serde_json::json!({
                    "location": args.location,
                    "temp": temp,
                    "unit": unit,
                    "conditions": "sunny",
                }))
            },
        )
        .tool(
            Tool::from_type::<GetTime>("get_time", "Get the current local time in a time zone"),
            |args: GetTime| async move {
                // A real implementation would consult a clock; a failure here
                // would reach the model as an errored result, not end the run.
                if args.time_zone.contains('/') {
                    Ok(format!("09:00 in {}", args.time_zone))
                } else {
                    Err(format!("`{}` is not an IANA time zone", args.time_zone))
                }
            },
        )
        .max_turns(8)
        .on_turn(|message| {
            eprintln!(
                "[turn] stop_reason={:?} output_tokens={}",
                message.stop_reason, message.usage.output_tokens
            );
        })
        .run()
        .await?;

    println!("{}", result.message.text());
    eprintln!(
        "\n{} turn(s), {} message(s) of history, stop_reason={:?}",
        result.turns,
        result.messages.len(),
        result.message.stop_reason
    );

    Ok(())
}
