//! Send a single message and print the reply.
//!
//! Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --example basic
//! ```

use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Reads the API key from the ANTHROPIC_API_KEY environment variable.
    let client = Client::from_env()?;

    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(1024)
        .system("You are a concise assistant.")
        .messages(vec![MessageParam::user(
            "In one sentence, what makes Rust's ownership model unusual?",
        )])
        .build()?;

    let message = client.messages().create(&request).await?;

    // `text()` concatenates every text block in the response.
    println!("{}", message.text());
    println!(
        "\n[stop_reason: {:?}, output_tokens: {}]",
        message.stop_reason, message.usage.output_tokens
    );

    Ok(())
}
