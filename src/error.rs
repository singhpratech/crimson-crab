//! Errors returned by the SDK.
//!
//! Every fallible operation returns [`Result<T>`], whose error type is [`Error`].
//! HTTP responses that are not `2xx` are mapped to a per-status [`Error`]
//! variant carrying a structured [`ApiError`] (the API's error envelope plus the
//! captured `request-id`). Transport failures, timeouts, (de)serialization
//! problems, and configuration mistakes have their own variants.

use std::time::Duration;

use serde::Deserialize;

/// A convenient alias for a [`std::result::Result`] whose error is [`Error`].
///
/// # Examples
///
/// ```
/// fn parse(id: &str) -> crimson_crab::Result<u32> {
///     id.parse().map_err(|_| crimson_crab::Error::Config("bad id".into()))
/// }
/// assert!(parse("abc").is_err());
/// ```
pub type Result<T> = std::result::Result<T, Error>;

/// The structured body of an API error.
///
/// `error_type` and `message` come from the wire envelope
/// (`{"type":"error","error":{"type":..., "message":...}}`); `request_id` is
/// captured from the `request-id` response header (falling back to the
/// envelope's `request_id` field) so it can be quoted when reporting an issue.
///
/// # Examples
///
/// ```
/// use crimson_crab::ApiError;
///
/// let err = ApiError {
///     error_type: "invalid_request_error".to_string(),
///     message: "max_tokens is required".to_string(),
///     request_id: Some("req_123".to_string()),
/// };
/// assert_eq!(err.error_type, "invalid_request_error");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    /// The API's machine-readable error type, e.g. `"invalid_request_error"`.
    pub error_type: String,
    /// A human-readable description of what went wrong.
    pub message: String,
    /// The `request-id` associated with the failed request, if any.
    pub request_id: Option<String>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.error_type)?;
        if let Some(request_id) = &self.request_id {
            write!(f, " [request-id: {request_id}]")?;
        }
        Ok(())
    }
}

/// All errors the SDK can return.
///
/// The per-status variants (`BadRequest`, `Authentication`, …) mirror the
/// documented HTTP error codes; any other non-2xx status maps to
/// [`Error::Api`]. Use [`Error::is_retryable`] to decide whether an error is
/// worth retrying (the client already retries automatically up to
/// `max_retries`).
///
/// # Examples
///
/// ```
/// use crimson_crab::{ApiError, Error};
///
/// let err = Error::Overloaded(ApiError {
///     error_type: "overloaded_error".into(),
///     message: "overloaded".into(),
///     request_id: None,
/// });
/// assert!(err.is_retryable());
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// `400 Bad Request` — the request was malformed or invalid.
    #[error("bad request (400): {0}")]
    BadRequest(ApiError),
    /// `401 Unauthorized` — the API key is missing or invalid.
    #[error("authentication error (401): {0}")]
    Authentication(ApiError),
    /// `403 Forbidden` — the key lacks permission (also `billing_error`).
    #[error("permission denied (403): {0}")]
    PermissionDenied(ApiError),
    /// `404 Not Found` — the endpoint or resource does not exist.
    #[error("not found (404): {0}")]
    NotFound(ApiError),
    /// `413 Payload Too Large` — the request exceeds size limits.
    #[error("request too large (413): {0}")]
    RequestTooLarge(ApiError),
    /// `429 Too Many Requests` — rate limited; carries `retry-after` if sent.
    #[error("rate limited (429): {err}")]
    RateLimit {
        /// The structured error body.
        err: ApiError,
        /// The server-suggested delay before retrying, from `retry-after`.
        retry_after: Option<Duration>,
    },
    /// `529 Overloaded` — the API is temporarily overloaded.
    #[error("overloaded (529): {0}")]
    Overloaded(ApiError),
    /// Any other non-2xx status not covered by a dedicated variant.
    #[error("api error ({status}): {err}")]
    Api {
        /// The HTTP status code.
        status: u16,
        /// The structured error body.
        err: ApiError,
    },
    /// The request timed out (retryable).
    #[error("request timed out")]
    Timeout,
    /// A transport-level failure (DNS, TCP, TLS, …).
    #[error(transparent)]
    Connection(reqwest::Error),
    /// A JSON (de)serialization failure.
    #[error("JSON error: {0}")]
    Serde(#[from] serde_json::Error),
    /// A streaming decode error (e.g. a malformed batch-results line).
    #[error("stream error: {0}")]
    Stream(String),
    /// A configuration error (e.g. a missing API key or invalid builder input).
    #[error("configuration error: {0}")]
    Config(String),
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Error::Timeout
        } else {
            Error::Connection(err)
        }
    }
}

impl Error {
    /// Returns `true` if retrying the operation might succeed.
    ///
    /// Retryable errors are rate limits, overloads, timeouts, connection
    /// failures, and `408`/`409`/`>= 500` API errors.
    ///
    /// # Examples
    ///
    /// ```
    /// use crimson_crab::{ApiError, Error};
    ///
    /// let bad = Error::BadRequest(ApiError {
    ///     error_type: "invalid_request_error".into(),
    ///     message: "nope".into(),
    ///     request_id: None,
    /// });
    /// assert!(!bad.is_retryable());
    /// assert!(Error::Timeout.is_retryable());
    /// ```
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::RateLimit { .. }
            | Error::Overloaded(_)
            | Error::Timeout
            | Error::Connection(_) => true,
            Error::Api { status, .. } => *status == 408 || *status == 409 || *status >= 500,
            _ => false,
        }
    }

    /// Returns the server-suggested retry delay, if the error carries one.
    ///
    /// Only [`Error::RateLimit`] can carry a `retry-after` value.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use crimson_crab::{ApiError, Error};
    ///
    /// let err = Error::RateLimit {
    ///     err: ApiError { error_type: "rate_limit_error".into(), message: "slow".into(), request_id: None },
    ///     retry_after: Some(Duration::from_secs(3)),
    /// };
    /// assert_eq!(err.retry_after(), Some(Duration::from_secs(3)));
    /// assert_eq!(Error::Timeout.retry_after(), None);
    /// ```
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Error::RateLimit { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

/// Maps an HTTP status and raw response body to the appropriate [`Error`].
pub(crate) fn from_status(
    status: u16,
    body: &[u8],
    request_id: Option<String>,
    retry_after: Option<Duration>,
) -> Error {
    let api = parse_api_error(body, request_id);
    status_to_error(status, api, retry_after)
}

/// Maps an in-stream `error` event body — which carries no HTTP status — to the
/// most appropriate [`Error`] variant, inferring a status from the error type.
pub(crate) fn from_error_body(error_type: String, message: String) -> Error {
    let status = match error_type.as_str() {
        "invalid_request_error" => 400,
        "authentication_error" => 401,
        "permission_error" | "billing_error" => 403,
        "not_found_error" => 404,
        "request_too_large" => 413,
        "rate_limit_error" => 429,
        "overloaded_error" => 529,
        _ => 500,
    };
    let api = ApiError {
        error_type,
        message,
        request_id: None,
    };
    status_to_error(status, api, None)
}

/// Maps a status code and parsed [`ApiError`] to the matching [`Error`] variant.
fn status_to_error(status: u16, api: ApiError, retry_after: Option<Duration>) -> Error {
    match status {
        400 => Error::BadRequest(api),
        401 => Error::Authentication(api),
        403 => Error::PermissionDenied(api),
        404 => Error::NotFound(api),
        413 => Error::RequestTooLarge(api),
        429 => Error::RateLimit {
            err: api,
            retry_after,
        },
        529 => Error::Overloaded(api),
        _ => Error::Api { status, err: api },
    }
}

/// Parses the error envelope, falling back to a raw-body message when the body
/// is not the expected JSON shape.
fn parse_api_error(body: &[u8], request_id: Option<String>) -> ApiError {
    #[derive(Deserialize)]
    struct Envelope {
        error: EnvelopeBody,
        #[serde(default)]
        request_id: Option<String>,
    }

    #[derive(Deserialize)]
    struct EnvelopeBody {
        #[serde(rename = "type", default)]
        error_type: String,
        #[serde(default)]
        message: String,
    }

    match serde_json::from_slice::<Envelope>(body) {
        Ok(env) => ApiError {
            error_type: if env.error.error_type.is_empty() {
                "unknown_error".to_string()
            } else {
                env.error.error_type
            },
            message: if env.error.message.is_empty() {
                String::from_utf8_lossy(body).trim().to_string()
            } else {
                env.error.message
            },
            request_id: request_id.or(env.request_id),
        },
        Err(_) => ApiError {
            error_type: "unknown_error".to_string(),
            message: String::from_utf8_lossy(body).trim().to_string(),
            request_id,
        },
    }
}
