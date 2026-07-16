//! The [`Client`] and its [`ClientBuilder`].
//!
//! A [`Client`] is the entry point to the API. Construct one with
//! [`Client::from_env`] (reads `ANTHROPIC_API_KEY`) or via [`Client::builder`]
//! for full control over the base URL, request timeout, and retry budget, then
//! reach the endpoints through the [`Client::messages`], [`Client::models`],
//! and [`Client::batches`] handles.

use std::time::Duration;

use crate::api::batches::Batches;
use crate::api::messages::Messages;
use crate::api::models::Models;
use crate::error::{Error, Result};
use crate::http::HttpClient;

/// The default API base URL.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// The default per-request timeout (10 minutes).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);
/// The default number of automatic retries for retryable failures.
const DEFAULT_MAX_RETRIES: u32 = 2;

/// A handle to the Claude API.
///
/// `Client` is cheap to clone (it shares an inner connection pool), so it can be
/// stored once and reused across tasks.
///
/// # Examples
///
/// ```
/// use crimson_crab::Client;
///
/// # fn main() -> crimson_crab::Result<()> {
/// let client = Client::builder().api_key("sk-ant-example").build()?;
/// let _ = client.messages();
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct Client {
    http: HttpClient,
}

impl Client {
    /// Starts building a client with the default base URL, timeout, and retries.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::Client;
    /// let builder = Client::builder();
    /// let _ = builder.api_key("sk-ant-example");
    /// ```
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Builds a client from the `ANTHROPIC_API_KEY` environment variable.
    ///
    /// Returns [`Error::Config`] if the variable is unset.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crimson_crab::Client;
    ///
    /// # fn main() -> crimson_crab::Result<()> {
    /// let client = Client::from_env()?;
    /// let _ = client.models();
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            Error::Config("environment variable ANTHROPIC_API_KEY is not set".to_string())
        })?;
        Self::builder().api_key(api_key).build()
    }

    /// Returns the Messages endpoint handle (`/v1/messages`).
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> crimson_crab::Result<()> {
    /// let client = crimson_crab::Client::builder().api_key("k").build()?;
    /// let _messages = client.messages();
    /// # Ok(())
    /// # }
    /// ```
    pub fn messages(&self) -> Messages<'_> {
        Messages::new(self)
    }

    /// Returns the Models endpoint handle (`/v1/models`).
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> crimson_crab::Result<()> {
    /// let client = crimson_crab::Client::builder().api_key("k").build()?;
    /// let _models = client.models();
    /// # Ok(())
    /// # }
    /// ```
    pub fn models(&self) -> Models<'_> {
        Models::new(self)
    }

    /// Returns the Message Batches endpoint handle (`/v1/messages/batches`).
    ///
    /// # Examples
    ///
    /// ```
    /// # fn main() -> crimson_crab::Result<()> {
    /// let client = crimson_crab::Client::builder().api_key("k").build()?;
    /// let _batches = client.batches();
    /// # Ok(())
    /// # }
    /// ```
    pub fn batches(&self) -> Batches<'_> {
        Batches::new(self)
    }

    /// Internal access to the transport for endpoint handles.
    pub(crate) fn http(&self) -> &HttpClient {
        &self.http
    }
}

/// A builder for [`Client`].
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use crimson_crab::Client;
///
/// # fn main() -> crimson_crab::Result<()> {
/// let client = Client::builder()
///     .api_key("sk-ant-example")
///     .base_url("https://api.anthropic.com")
///     .timeout(Duration::from_secs(120))
///     .max_retries(3)
///     .build()?;
/// let _ = client.messages();
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct ClientBuilder {
    api_key: Option<String>,
    base_url: String,
    timeout: Duration,
    max_retries: u32,
}

impl std::fmt::Debug for ClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientBuilder")
            // Never print the API key; only whether one has been set.
            .field("api_key", &self.api_key.as_ref().map(|_| "[redacted]"))
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: DEFAULT_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }
}

impl ClientBuilder {
    /// Sets the API key (`x-api-key`). Required.
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Overrides the base URL (default `https://api.anthropic.com`).
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Sets the per-request timeout (default 10 minutes).
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the maximum number of automatic retries (default 2; `0` disables).
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Builds the [`Client`].
    ///
    /// Returns [`Error::Config`] if the API key is missing or empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::Client;
    /// assert!(Client::builder().build().is_err()); // no api key
    /// assert!(Client::builder().api_key("k").build().is_ok());
    /// ```
    pub fn build(self) -> Result<Client> {
        let api_key = self.api_key.filter(|key| !key.is_empty()).ok_or_else(|| {
            Error::Config(
                "`api_key` is required (set it explicitly or via ANTHROPIC_API_KEY)".to_string(),
            )
        })?;
        // Use an *idle* read timeout (reset after each successful read) rather
        // than a total-request `timeout`, so a long-running but actively-flowing
        // streaming response is not truncated once total elapsed time crosses a
        // deadline. The total deadline is applied per-request on the unary paths
        // inside `HttpClient` (see `request_timeout`).
        let http_client = reqwest::Client::builder()
            .read_timeout(self.timeout)
            .build()?;
        let http = HttpClient::new(
            http_client,
            self.base_url,
            api_key,
            self.max_retries,
            self.timeout,
        );
        Ok(Client { http })
    }
}
