//! Live smoke test against the real Claude API.
//!
//! Unlike the unit and `wiremock` tests, this example makes real network calls
//! and needs a valid key. It exercises the three core paths — token counting, a
//! non-streaming request, and streaming — and is handy to run by hand before
//! cutting a release.
//!
//! Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --example live_smoke
//! ```

use std::io::Write;

use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::prelude::*;
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Reads the API key from the ANTHROPIC_API_KEY environment variable.
    let client = Client::from_env()?;

    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(128)
        .system("You are a concise assistant. Answer in one short sentence.")
        .messages(vec![MessageParam::user(
            "In one sentence, what does the Rust borrow checker do?",
        )])
        .build()?;

    // 1. Token counting (a separate endpoint).
    println!("== count_tokens ==");
    let count = client
        .messages()
        .count_tokens(&request.as_count_request())
        .await?;
    println!("input_tokens: {}", count.input_tokens);

    // 2. Non-streaming request.
    println!("\n== create (non-streaming) ==");
    let message = client.messages().create(&request).await?;
    println!("reply: {}", message.text());
    println!(
        "stop_reason: {:?} | output_tokens: {}",
        message.stop_reason, message.usage.output_tokens
    );

    // 3. Streaming request — print text deltas as they arrive, then read the
    //    accumulated final message.
    println!("\n== stream ==");
    print!("reply (live): ");
    std::io::stdout().flush().ok();
    let mut stream = client.messages().stream(&request).await?;
    while let Some(event) = stream.next().await {
        if let Ok(StreamEvent::ContentBlockDelta {
            delta: ContentDelta::TextDelta { text },
            ..
        }) = event
        {
            print!("{text}");
            std::io::stdout().flush().ok();
        }
    }
    println!();
    if let Some(msg) = stream.final_message() {
        println!("final output_tokens: {}", msg.usage.output_tokens);
    }

    println!("\ncrimson-crab reached the live Claude API successfully.");
    Ok(())
}
