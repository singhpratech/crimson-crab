//! Typed structured output: derive the schema from a Rust type and let
//! `messages().parse::<T>()` constrain the response and deserialize it.
//!
//! This is the typed counterpart to `structured_output.rs`, which writes the
//! same JSON Schema by hand.
//!
//! Requires the `schemars` feature. Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --features schemars --example typed_parse
//! ```

use crimson_crab::model_ids::CLAUDE_OPUS_5;
use crimson_crab::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;

/// A contact extracted from free-form text.
#[derive(Debug, Deserialize, JsonSchema)]
struct Contact {
    /// The contact's full name.
    name: String,
    /// The contact's email address.
    email: String,
    /// The employer, or `null` if the text does not mention one.
    ///
    /// `Option<T>` becomes a *nullable* property rather than an absent one, so
    /// the model always emits the key and may answer `null` for it.
    company: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::from_env()?;

    // No `output_config` needed: `parse` derives the schema from `Contact` and
    // sets `output_config.format` on its own copy of the request.
    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_5)
        .max_tokens(512)
        .messages(vec![MessageParam::user(
            "Extract the contact: 'Ada Lovelace <ada@analytical.dev> at Analytical Engines'.",
        )])
        .build()?;

    let parsed: ParsedMessage<Contact> = client.messages().parse(&request).await?;

    println!("name:    {}", parsed.data.name);
    println!("email:   {}", parsed.data.email);
    println!(
        "company: {}",
        parsed.data.company.as_deref().unwrap_or("(none)")
    );

    // The whole message is still there, so usage and stop reason stay reachable.
    println!(
        "\n[{} in / {} out tokens, stop_reason {:?}]",
        parsed.message.usage.input_tokens,
        parsed.message.usage.output_tokens,
        parsed.message.stop_reason,
    );

    Ok(())
}
