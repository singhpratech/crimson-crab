//! High-level endpoint handles and their request/response types.
//!
//! Each submodule exposes a lightweight handle borrowed from a
//! [`crate::Client`] plus the typed request and response values for that
//! endpoint:
//!
//! * [`messages`] — create a message and count tokens.
//! * [`models`] — retrieve and list model metadata.
//! * [`batches`] — create, poll, cancel, and stream Message Batches.

pub mod batches;
pub mod messages;
pub mod models;

pub use batches::{
    BatchCanceled, BatchErrored, BatchExpired, BatchListParams, BatchPage, BatchRequestCounts,
    BatchRequestItem, BatchResult, BatchResultOutcome, BatchResults, BatchStatus, BatchSucceeded,
    Batches, MessageBatch,
};
pub use messages::{
    CountTokensRequest, CountTokensResponse, Messages, MessagesRequest, MessagesRequestBuilder,
};
pub use models::{ModelInfo, ModelListParams, ModelPage, Models};
