//! Stream a response, printing text deltas as they arrive, then inspect the
//! accumulated final message.
//!
//! Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --example streaming
//! ```

use std::io::Write;

use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::prelude::*;
use crimson_crab::streaming::{ContentDelta, StreamEvent};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::from_env()?;

    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(1024)
        .messages(vec![MessageParam::user("Write a haiku about async Rust.")])
        .build()?;

    let mut stream = client.messages().stream(&request).await?;

    // Print incremental text as the model produces it. The stream also
    // accumulates a complete `Message` behind the scenes.
    while let Some(event) = stream.next().await {
        if let StreamEvent::ContentBlockDelta {
            delta: ContentDelta::TextDelta { text },
            ..
        } = event?
        {
            print!("{text}");
            std::io::stdout().flush()?;
        }
    }
    println!();

    // After draining, the accumulated message matches a non-streaming response.
    if let Some(message) = stream.final_message() {
        println!("\n[stop_reason: {:?}]", message.stop_reason);
        println!("[total output tokens: {}]", message.usage.output_tokens);
    }

    Ok(())
}
