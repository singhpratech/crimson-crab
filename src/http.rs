//! Internal HTTP transport: header injection, retry with exponential backoff +
//! jitter, and non-2xx error mapping. Not part of the public API.
//!
//! Every request carries `x-api-key`, `anthropic-version: 2023-06-01`, and
//! `content-type: application/json`; when a request specifies beta flags they
//! are joined with commas into a single `anthropic-beta` header. Connection
//! errors, timeouts, `408`/`409`/`429`, and `>= 500` responses are retried up
//! to `max_retries` times, honoring a `retry-after` header when present.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{self, Error, Result};

/// A cheap-to-clone HTTP transport bound to a base URL, API key, and retry
/// budget.
#[derive(Clone)]
pub(crate) struct HttpClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    max_retries: u32,
    /// The total-request deadline applied to non-streaming (unary) requests.
    ///
    /// It is deliberately **not** applied to streaming responses: the reqwest
    /// client is configured with a matching *idle* read timeout instead, so a
    /// long but actively-flowing SSE generation is never truncated just because
    /// total elapsed time crossed a deadline.
    request_timeout: Duration,
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpClient")
            .field("base_url", &self.base_url)
            .field("api_key", &"[redacted]")
            .field("max_retries", &self.max_retries)
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

impl HttpClient {
    pub(crate) fn new(
        client: reqwest::Client,
        base_url: String,
        api_key: String,
        max_retries: u32,
        request_timeout: Duration,
    ) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        Self {
            client,
            base_url,
            api_key,
            max_retries,
            request_timeout,
        }
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        betas: &[String],
    ) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self
            .client
            .request(method, url)
            .header("x-api-key", self.api_key.as_str())
            .header("anthropic-version", "2023-06-01")
            .header(reqwest::header::CONTENT_TYPE, "application/json");
        if !betas.is_empty() {
            builder = builder.header("anthropic-beta", betas.join(","));
        }
        builder
    }

    pub(crate) async fn post_json<R: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
        betas: &[String],
    ) -> Result<R> {
        let bytes = serde_json::to_vec(body)?;
        let builder = self.request(reqwest::Method::POST, path, betas).body(bytes);
        self.send_json(builder).await
    }

    pub(crate) async fn post_no_body<R: DeserializeOwned>(
        &self,
        path: &str,
        betas: &[String],
    ) -> Result<R> {
        let builder = self.request(reqwest::Method::POST, path, betas);
        self.send_json(builder).await
    }

    /// Executes a POST and returns the raw success response for streaming bodies.
    ///
    /// Retries (via [`Self::execute`]) happen only before the response head is
    /// received; once the body starts streaming there is no further retry.
    pub(crate) async fn post_raw(
        &self,
        path: &str,
        body: &serde_json::Value,
        betas: &[String],
    ) -> Result<reqwest::Response> {
        let bytes = serde_json::to_vec(body)?;
        let builder = self.request(reqwest::Method::POST, path, betas).body(bytes);
        self.execute(builder).await
    }

    pub(crate) async fn get_json<R: DeserializeOwned>(
        &self,
        path: &str,
        betas: &[String],
    ) -> Result<R> {
        let builder = self.request(reqwest::Method::GET, path, betas);
        self.send_json(builder).await
    }

    pub(crate) async fn get_json_query<R: DeserializeOwned, Q: Serialize + ?Sized>(
        &self,
        path: &str,
        query: &Q,
        betas: &[String],
    ) -> Result<R> {
        let builder = self.request(reqwest::Method::GET, path, betas).query(query);
        self.send_json(builder).await
    }

    /// Executes a GET and returns the raw success response for streaming bodies.
    pub(crate) async fn get_raw(&self, path: &str, betas: &[String]) -> Result<reqwest::Response> {
        let builder = self.request(reqwest::Method::GET, path, betas);
        self.execute(builder).await
    }

    async fn send_json<R: DeserializeOwned>(&self, builder: reqwest::RequestBuilder) -> Result<R> {
        // Unary requests get a total-request deadline. Streaming requests
        // (`post_raw`/`get_raw`) deliberately skip this and rely on the client's
        // idle read timeout instead, so a slow-but-flowing stream is not killed
        // by a total deadline mid-generation.
        let builder = builder.timeout(self.request_timeout);
        let response = self.execute(builder).await?;
        let bytes = response.bytes().await?;
        serde_json::from_slice(&bytes).map_err(Error::from)
    }

    /// Sends a request, retrying retryable failures with backoff, and returns
    /// the first successful response (non-2xx statuses are mapped to [`Error`]).
    async fn execute(&self, builder: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut attempt: u32 = 0;
        loop {
            let Some(attempt_builder) = builder.try_clone() else {
                return Err(Error::Config(
                    "request body could not be cloned for retry".to_string(),
                ));
            };
            match attempt_builder.send().await {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let err = map_error_response(response).await;
                    if attempt < self.max_retries && err.is_retryable() {
                        let delay = err.retry_after().unwrap_or_else(|| backoff_delay(attempt));
                        sleep(delay).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(err);
                }
                Err(source) => {
                    let err = Error::from(source);
                    if attempt < self.max_retries && err.is_retryable() {
                        sleep(backoff_delay(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }
}

/// Consumes an error response and maps its status, body, and headers to [`Error`].
async fn map_error_response(response: reqwest::Response) -> Error {
    let status = response.status().as_u16();
    let request_id = header_string(response.headers(), "request-id");
    // Cap `retry-after` at 60s (matching official SDK behavior): a broken or
    // hostile server must not be able to park the retry loop for hours by
    // sending an absurd value — beyond the cap we fall back to normal backoff.
    const MAX_RETRY_AFTER_SECS: u64 = 60;
    let retry_after = header_string(response.headers(), "retry-after")
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|secs| *secs <= MAX_RETRY_AFTER_SECS)
        .map(Duration::from_secs);
    match response.bytes().await {
        Ok(bytes) => error::from_status(status, &bytes, request_id, retry_after),
        Err(source) => Error::from(source),
    }
}

fn header_string(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string())
}

/// Full-jitter exponential backoff: a random delay in `[0, min(cap, base·2^n)]`
/// with a 0.5s base and 8s cap.
fn backoff_delay(attempt: u32) -> Duration {
    const BASE_SECS: f64 = 0.5;
    const CAP_SECS: f64 = 8.0;
    let exponential = BASE_SECS * 2f64.powi(attempt as i32);
    let ceiling = exponential.min(CAP_SECS);
    Duration::from_secs_f64(ceiling * fastrand::f64())
}

/// A runtime-agnostic async sleep backed by a short-lived timer thread.
///
/// The SDK deliberately avoids a tokio dependency, so this provides the small
/// amount of timing the retry loop needs without one.
async fn sleep(duration: Duration) {
    if duration.is_zero() {
        return;
    }
    Timer::new(duration).await;
}

struct TimerState {
    done: bool,
    waker: Option<Waker>,
}

struct Timer {
    state: Arc<Mutex<TimerState>>,
}

impl Timer {
    fn new(duration: Duration) -> Self {
        let state = Arc::new(Mutex::new(TimerState {
            done: false,
            waker: None,
        }));
        let thread_state = Arc::clone(&state);
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            let mut guard = lock(&thread_state);
            guard.done = true;
            if let Some(waker) = guard.waker.take() {
                waker.wake();
            }
        });
        Self { state }
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut guard = lock(&self.state);
        if guard.done {
            Poll::Ready(())
        } else {
            guard.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// Locks a mutex, recovering the guard on poison rather than panicking (the SDK
/// forbids `unwrap`/`expect` in library code).
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
