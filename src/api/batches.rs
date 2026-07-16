//! The Message Batches endpoint: create, poll, cancel, and stream results.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use serde::{Deserialize, Serialize};

use crate::api::messages::MessagesRequest;
use crate::client::Client;
use crate::error::{Error, Result};
use crate::types::{string_enum, tagged_enum, Message};

string_enum! {
    /// The processing status of a [`MessageBatch`].
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::api::BatchStatus;
    /// assert_eq!(BatchStatus::InProgress.as_str(), "in_progress");
    /// ```
    pub enum BatchStatus {
        /// The batch is still being processed.
        InProgress = "in_progress",
        /// A cancellation was requested and is in progress.
        Canceling = "canceling",
        /// The batch has finished (results are available).
        Ended = "ended",
    }
}

/// The per-status request tallies for a [`MessageBatch`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchRequestCounts {
    /// Requests still processing.
    #[serde(default)]
    pub processing: u64,
    /// Requests that succeeded.
    #[serde(default)]
    pub succeeded: u64,
    /// Requests that errored.
    #[serde(default)]
    pub errored: u64,
    /// Requests that were canceled.
    #[serde(default)]
    pub canceled: u64,
    /// Requests that expired.
    #[serde(default)]
    pub expired: u64,
}

/// A Message Batch (`POST /v1/messages/batches`).
///
/// # Examples
///
/// ```
/// use crimson_crab::api::{BatchStatus, MessageBatch};
///
/// let batch: MessageBatch = serde_json::from_value(serde_json::json!({
///     "id": "msgbatch_01",
///     "type": "message_batch",
///     "processing_status": "in_progress",
///     "request_counts": {"processing": 2, "succeeded": 0, "errored": 0, "canceled": 0, "expired": 0}
/// })).unwrap();
/// assert_eq!(batch.processing_status, BatchStatus::InProgress);
/// assert_eq!(batch.request_counts.processing, 2);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageBatch {
    /// The batch id (`msgbatch_...`).
    pub id: String,
    /// The object type; always `"message_batch"`.
    #[serde(rename = "type")]
    pub object_type: String,
    /// The current processing status.
    pub processing_status: BatchStatus,
    /// The per-status request tallies.
    pub request_counts: BatchRequestCounts,
    /// When the batch was created (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When the batch expires (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// When processing ended (ISO 8601), if it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    /// When cancellation was initiated (ISO 8601), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_initiated_at: Option<String>,
    /// When the batch was archived (ISO 8601), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// The URL from which to fetch results, once available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results_url: Option<String>,
    /// Forward-compatible catch-all for any other fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A page of batches from `GET /v1/messages/batches`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchPage {
    /// The batches on this page.
    pub data: Vec<MessageBatch>,
    /// Whether more pages are available.
    #[serde(default)]
    pub has_more: bool,
    /// The id of the first item on this page (a pagination cursor).
    #[serde(default)]
    pub first_id: Option<String>,
    /// The id of the last item on this page (a pagination cursor).
    #[serde(default)]
    pub last_id: Option<String>,
}

/// Pagination parameters for [`Batches::list`].
#[derive(Clone, Debug, Default)]
pub struct BatchListParams {
    /// Return items after this id.
    pub after_id: Option<String>,
    /// Return items before this id.
    pub before_id: Option<String>,
    /// The maximum number of items per page.
    pub limit: Option<u32>,
}

impl BatchListParams {
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(limit) = self.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(after_id) = &self.after_id {
            query.push(("after_id", after_id.clone()));
        }
        if let Some(before_id) = &self.before_id {
            query.push(("before_id", before_id.clone()));
        }
        query
    }
}

/// A single entry in the `requests` array sent to
/// `POST /v1/messages/batches`.
///
/// # Examples
///
/// ```
/// use crimson_crab::api::{BatchRequestItem, MessagesRequest};
/// use crimson_crab::types::MessageParam;
///
/// let request = MessagesRequest::builder()
///     .model("claude-opus-4-8")
///     .max_tokens(1024)
///     .messages(vec![MessageParam::user("Hi")])
///     .build()
///     .unwrap();
/// let item = BatchRequestItem::from_request("r1", &request).unwrap();
/// assert_eq!(item.custom_id, "r1");
/// assert_eq!(item.params["model"], serde_json::json!("claude-opus-4-8"));
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchRequestItem {
    /// A caller-chosen id echoed back on the matching [`BatchResult`].
    pub custom_id: String,
    /// The full `/v1/messages` request body for this entry.
    pub params: serde_json::Value,
}

impl BatchRequestItem {
    /// Creates an item from a raw params object.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::api::BatchRequestItem;
    /// let item = BatchRequestItem::new("r1", serde_json::json!({"model": "claude-opus-4-8"}));
    /// assert_eq!(item.custom_id, "r1");
    /// ```
    pub fn new(custom_id: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            custom_id: custom_id.into(),
            params,
        }
    }

    /// Creates an item from a [`MessagesRequest`], serializing its body.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::api::{BatchRequestItem, MessagesRequest};
    /// use crimson_crab::types::MessageParam;
    ///
    /// let request = MessagesRequest::builder()
    ///     .model("claude-opus-4-8")
    ///     .max_tokens(64)
    ///     .messages(vec![MessageParam::user("Hi")])
    ///     .build()
    ///     .unwrap();
    /// let item = BatchRequestItem::from_request("r1", &request).unwrap();
    /// assert_eq!(item.params["max_tokens"], serde_json::json!(64));
    /// ```
    pub fn from_request(custom_id: impl Into<String>, request: &MessagesRequest) -> Result<Self> {
        Ok(Self {
            custom_id: custom_id.into(),
            params: request.to_json_body()?,
        })
    }
}

/// The payload of a successful [`BatchResult`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchSucceeded {
    /// The generated message.
    pub message: Message,
}

/// The payload of an errored [`BatchResult`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchErrored {
    /// The raw error envelope for the failed request.
    pub error: serde_json::Value,
}

/// The payload of a canceled [`BatchResult`] (no fields).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchCanceled {}

/// The payload of an expired [`BatchResult`] (no fields).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchExpired {}

tagged_enum! {
    /// The outcome of one batched request.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::api::BatchResultOutcome;
    ///
    /// let outcome: BatchResultOutcome =
    ///     serde_json::from_value(serde_json::json!({"type": "canceled"})).unwrap();
    /// assert!(matches!(outcome, BatchResultOutcome::Canceled(_)));
    /// ```
    // A successful result carries a full `Message`, so this enum is intentionally
    // large; batch results are streamed one at a time, so boxing would only add
    // indirection without a meaningful memory win.
    #[allow(clippy::large_enum_variant)]
    pub enum BatchResultOutcome {
        /// The request succeeded.
        Succeeded(BatchSucceeded) = "succeeded",
        /// The request errored.
        Errored(BatchErrored) = "errored",
        /// The request was canceled.
        Canceled(BatchCanceled) = "canceled",
        /// The request expired.
        Expired(BatchExpired) = "expired",
    }
}

/// One line of a batch results stream: a `custom_id` and its outcome.
///
/// # Examples
///
/// ```
/// use crimson_crab::api::{BatchResult, BatchResultOutcome};
///
/// let result: BatchResult = serde_json::from_value(serde_json::json!({
///     "custom_id": "r1",
///     "result": {"type": "expired"}
/// })).unwrap();
/// assert_eq!(result.custom_id, "r1");
/// assert!(matches!(result.result, BatchResultOutcome::Expired(_)));
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchResult {
    /// The caller-chosen id supplied when the batch was created.
    pub custom_id: String,
    /// The outcome for this request.
    pub result: BatchResultOutcome,
}

/// A [`Stream`] of [`BatchResult`]s decoded from a JSONL response.
///
/// Results arrive in any order; key them by `custom_id`, never by position. The
/// stream yields one item per non-empty line.
pub struct BatchResults {
    #[cfg(not(target_arch = "wasm32"))]
    inner: Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>> + Send>>,
    #[cfg(target_arch = "wasm32")]
    inner: Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>>>>,
    buffer: Vec<u8>,
    finished: bool,
}

impl BatchResults {
    pub(crate) fn new(response: reqwest::Response) -> Self {
        Self {
            inner: Box::pin(response.bytes_stream()),
            buffer: Vec::new(),
            finished: false,
        }
    }

    /// Parses one line, returning `None` for blank lines.
    fn parse_line(line: &[u8]) -> Option<Result<BatchResult>> {
        let trimmed = trim_ascii(line);
        if trimmed.is_empty() {
            return None;
        }
        Some(serde_json::from_slice::<BatchResult>(trimmed).map_err(Error::from))
    }
}

impl Stream for BatchResults {
    type Item = Result<BatchResult>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(position) = this.buffer.iter().position(|&byte| byte == b'\n') {
                let line: Vec<u8> = this.buffer.drain(..=position).collect();
                match BatchResults::parse_line(&line) {
                    Some(item) => return Poll::Ready(Some(item)),
                    None => continue,
                }
            }

            if this.finished {
                if !this.buffer.is_empty() {
                    let line = std::mem::take(&mut this.buffer);
                    if let Some(item) = BatchResults::parse_line(&line) {
                        return Poll::Ready(Some(item));
                    }
                }
                return Poll::Ready(None);
            }

            match this.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    this.buffer.extend_from_slice(&bytes);
                    continue;
                }
                Poll::Ready(Some(Err(source))) => {
                    this.finished = true;
                    return Poll::Ready(Some(Err(Error::from(source))));
                }
                Poll::Ready(None) => {
                    this.finished = true;
                    continue;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Trims leading and trailing ASCII whitespace from a byte slice.
///
/// A hand-rolled equivalent of `[u8]::trim_ascii`, which is not available on the
/// crate's MSRV.
fn trim_ascii(mut slice: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = slice {
        if first.is_ascii_whitespace() {
            slice = rest;
        } else {
            break;
        }
    }
    while let [rest @ .., last] = slice {
        if last.is_ascii_whitespace() {
            slice = rest;
        } else {
            break;
        }
    }
    slice
}

/// A handle to the Message Batches endpoint, obtained via [`Client::batches`].
///
/// [`Client::batches`]: crate::Client::batches
///
/// # Examples
///
/// ```no_run
/// use crimson_crab::api::{BatchRequestItem, MessagesRequest};
/// use crimson_crab::types::MessageParam;
///
/// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
/// let request = MessagesRequest::builder()
///     .model("claude-opus-4-8")
///     .max_tokens(1024)
///     .messages(vec![MessageParam::user("Hi")])
///     .build()?;
/// let item = BatchRequestItem::from_request("r1", &request)?;
/// let batch = client.batches().create(&[item]).await?;
/// let _fetched = client.batches().get(&batch.id).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Batches<'a> {
    client: &'a Client,
}

impl<'a> Batches<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Creates a batch from a set of request items
    /// (`POST /v1/messages/batches`).
    pub async fn create(&self, requests: &[BatchRequestItem]) -> Result<MessageBatch> {
        let body = serde_json::json!({ "requests": requests });
        self.client
            .http()
            .post_json("/v1/messages/batches", &body, &[])
            .await
    }

    /// Retrieves a batch (`GET /v1/messages/batches/{id}`).
    pub async fn get(&self, id: &str) -> Result<MessageBatch> {
        let path = format!("/v1/messages/batches/{id}");
        self.client.http().get_json(&path, &[]).await
    }

    /// Lists batches (`GET /v1/messages/batches`) with optional pagination.
    pub async fn list(&self, params: &BatchListParams) -> Result<BatchPage> {
        let query = params.to_query();
        self.client
            .http()
            .get_json_query("/v1/messages/batches", &query, &[])
            .await
    }

    /// Requests cancellation of a batch
    /// (`POST /v1/messages/batches/{id}/cancel`).
    pub async fn cancel(&self, id: &str) -> Result<MessageBatch> {
        let path = format!("/v1/messages/batches/{id}/cancel");
        self.client.http().post_no_body(&path, &[]).await
    }

    /// Streams a completed batch's results
    /// (`GET /v1/messages/batches/{id}/results`), decoding one
    /// [`BatchResult`] per JSONL line.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
    /// let mut results = client.batches().results("msgbatch_01").await?;
    /// // `results` implements `futures_core::Stream`; drive it with your runtime.
    /// let _ = &mut results;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn results(&self, id: &str) -> Result<BatchResults> {
        let path = format!("/v1/messages/batches/{id}/results");
        let response = self.client.http().get_raw(&path, &[]).await?;
        Ok(BatchResults::new(response))
    }
}
