//! Extended thinking: enable adaptive reasoning with a display mode and an
//! effort level, then separate the thinking blocks from the final answer.
//!
//! Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --example thinking
//! ```

use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::prelude::*;
use crimson_crab::types::ContentBlock;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::from_env()?;

    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(4096)
        // Let the model choose its own reasoning budget and surface a summary.
        .thinking(ThinkingConfig::adaptive_with_display(
            ThinkingDisplay::Summarized,
        ))
        // Ask for high reasoning effort before it answers.
        .output_config(OutputConfig {
            effort: Some(Effort::High),
            format: None,
        })
        .messages(vec![MessageParam::user(
            "A bat and a ball cost $1.10 total. The bat costs $1.00 more than \
             the ball. How much does the ball cost? Explain briefly.",
        )])
        .build()?;

    let message = client.messages().create(&request).await?;

    for block in &message.content {
        match block {
            ContentBlock::Thinking(thinking) => {
                println!("--- thinking ---\n{}\n", thinking.thinking);
            }
            ContentBlock::Text(text) => {
                println!("--- answer ---\n{}", text.text);
            }
            _ => {}
        }
    }

    Ok(())
}
