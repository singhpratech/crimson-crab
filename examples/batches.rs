//! Submit a Message Batch, poll until it ends, and stream the JSONL results.
//!
//! Run with: `ANTHROPIC_API_KEY=... cargo run --example batches`

use std::time::Duration;

use crimson_crab::api::batches::{BatchRequestItem, BatchResultOutcome, BatchStatus};
use crimson_crab::prelude::*;
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;

    // 1. One entry per request, each keyed by your own custom_id.
    let requests: Vec<BatchRequestItem> = (0..3)
        .map(|i| {
            let request = MessagesRequest::builder()
                .model(crimson_crab::model_ids::CLAUDE_OPUS_4_8)
                .max_tokens(64)
                .messages(vec![MessageParam::user(format!(
                    "Classify sentiment (one word): review #{i}"
                ))])
                .build()?;
            BatchRequestItem::from_request(format!("review-{i}"), &request)
        })
        .collect::<crimson_crab::Result<_>>()?;

    let batch = client.batches().create(&requests).await?;
    println!("batch {} created", batch.id);

    // 2. Poll until processing ends (most batches finish well within an hour).
    let batch = loop {
        let current = client.batches().get(&batch.id).await?;
        if current.processing_status == BatchStatus::Ended {
            break current;
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    };

    // 3. Stream the JSONL results; they arrive in ANY order — key by custom_id.
    let mut results = client.batches().results(&batch.id).await?;
    while let Some(result) = results.next().await {
        let result = result?;
        match result.result {
            BatchResultOutcome::Succeeded(success) => {
                println!("{}: {}", result.custom_id, success.message.text());
            }
            other => println!("{}: {:?}", result.custom_id, other),
        }
    }
    Ok(())
}
