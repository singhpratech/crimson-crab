//! Prompt caching: mark a large system prompt with a cache breakpoint and watch
//! the usage counters. The first call writes the cache; a second call with the
//! same prefix reads it, cutting input-token cost.
//!
//! Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --example prompt_caching
//! ```

use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::prelude::*;
use crimson_crab::types::TextBlockParam;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::from_env()?;

    // A large, reusable instruction block worth caching. Place the cache
    // breakpoint on the last cacheable block via `cache_control`.
    let mut system_block = TextBlockParam::new(format!(
        "You are a support agent. Follow this policy exactly:\n{}",
        "- Always be polite and concise.\n".repeat(200)
    ));
    system_block.cache_control = Some(CacheControl::ephemeral());
    let system: Vec<ContentBlockParam> = vec![ContentBlockParam::Text(system_block)];

    let ask = |question: &str| {
        MessagesRequest::builder()
            .model(CLAUDE_OPUS_4_8)
            .max_tokens(256)
            .system(system.clone())
            .messages(vec![MessageParam::user(question.to_string())])
            .build()
    };

    let first = client
        .messages()
        .create(&ask("What are your top two rules?")?)
        .await?;
    println!(
        "first  -> cache_creation: {:?}, cache_read: {:?}",
        first.usage.cache_creation_input_tokens, first.usage.cache_read_input_tokens
    );

    let second = client.messages().create(&ask("Restate rule one.")?).await?;
    println!(
        "second -> cache_creation: {:?}, cache_read: {:?}",
        second.usage.cache_creation_input_tokens, second.usage.cache_read_input_tokens
    );

    Ok(())
}
