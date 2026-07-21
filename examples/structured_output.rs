//! Structured output: constrain the response to a JSON Schema via
//! `output_config.format` and parse the result into a typed value.
//!
//! Run with:
//!
//! ```sh
//! ANTHROPIC_API_KEY=sk-ant-... cargo run --example structured_output
//! ```

use crimson_crab::model_ids::CLAUDE_OPUS_4_8;
use crimson_crab::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Contact {
    name: String,
    email: String,
    company: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::from_env()?;

    // A JSON Schema with `additionalProperties: false`, as the API requires.
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "email": {"type": "string"},
            "company": {"type": ["string", "null"]}
        },
        "required": ["name", "email", "company"],
        "additionalProperties": false
    });

    let request = MessagesRequest::builder()
        .model(CLAUDE_OPUS_4_8)
        .max_tokens(512)
        .output_config(OutputConfig {
            effort: None,
            format: Some(OutputFormat::json_schema(schema)),
        })
        .messages(vec![MessageParam::user(
            "Extract the contact: 'Ada Lovelace <ada@analytical.dev> at Analytical Engines'.",
        )])
        .build()?;

    let message = client.messages().create(&request).await?;

    // The response text is guaranteed to match the schema; parse it.
    let contact: Contact = serde_json::from_str(&message.text())?;
    println!("name:    {}", contact.name);
    println!("email:   {}", contact.email);
    println!(
        "company: {}",
        contact.company.as_deref().unwrap_or("(none)")
    );

    Ok(())
}
