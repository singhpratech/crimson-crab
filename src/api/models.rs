//! The Models endpoint: [`ModelInfo`], retrieval, and pagination.

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

/// Metadata about a single model (`GET /v1/models/{id}`).
///
/// `capabilities` is kept as a raw [`serde_json::Value`] so new capability keys
/// deserialize without an SDK release.
///
/// # Examples
///
/// ```
/// use crimson_crab::api::ModelInfo;
///
/// let info: ModelInfo = serde_json::from_value(serde_json::json!({
///     "id": "claude-opus-4-8",
///     "display_name": "Claude Opus 4.8",
///     "type": "model",
///     "max_input_tokens": 1000000,
///     "max_tokens": 128000,
///     "capabilities": {"vision": {"supported": true}}
/// })).unwrap();
/// assert_eq!(info.id, "claude-opus-4-8");
/// assert_eq!(info.max_input_tokens, Some(1_000_000));
/// assert!(info.capabilities.is_object());
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The model id.
    pub id: String,
    /// The object type; typically `"model"`.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub object_type: Option<String>,
    /// A human-readable display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// The creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// The model's context window (input token limit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    /// The model's maximum output tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// The nested capability tree, kept as raw JSON for forward compatibility.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub capabilities: serde_json::Value,
    /// Forward-compatible catch-all for any other model fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A page of models from `GET /v1/models`.
///
/// # Examples
///
/// ```
/// use crimson_crab::api::ModelPage;
///
/// let page: ModelPage = serde_json::from_value(serde_json::json!({
///     "data": [{"id": "claude-opus-4-8"}],
///     "has_more": false,
///     "first_id": "claude-opus-4-8",
///     "last_id": "claude-opus-4-8"
/// })).unwrap();
/// assert_eq!(page.data.len(), 1);
/// assert!(!page.has_more);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelPage {
    /// The models on this page.
    pub data: Vec<ModelInfo>,
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

/// Pagination parameters for [`Models::list`].
///
/// # Examples
///
/// ```
/// use crimson_crab::api::ModelListParams;
///
/// let params = ModelListParams { limit: Some(20), ..Default::default() };
/// assert_eq!(params.limit, Some(20));
/// ```
#[derive(Clone, Debug, Default)]
pub struct ModelListParams {
    /// Return items after this id (a cursor from a previous `last_id`).
    pub after_id: Option<String>,
    /// Return items before this id (a cursor from a previous `first_id`).
    pub before_id: Option<String>,
    /// The maximum number of items per page.
    pub limit: Option<u32>,
}

impl ModelListParams {
    /// Builds the query-string pairs for this set of parameters.
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

/// A handle to the Models endpoint, obtained via [`Client::models`].
///
/// [`Client::models`]: crate::Client::models
///
/// # Examples
///
/// ```no_run
/// use crimson_crab::api::ModelListParams;
///
/// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
/// let opus = client.models().get("claude-opus-4-8").await?;
/// let page = client.models().list(&ModelListParams::default()).await?;
/// println!("{} models, first is {}", page.data.len(), opus.id);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Models<'a> {
    client: &'a Client,
}

impl<'a> Models<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Retrieves a single model (`GET /v1/models/{id}`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
    /// let model = client.models().get("claude-opus-4-8").await?;
    /// let _ = model.max_input_tokens;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get(&self, id: &str) -> Result<ModelInfo> {
        let path = format!("/v1/models/{id}");
        self.client.http().get_json(&path, &[]).await
    }

    /// Lists models (`GET /v1/models`) with optional pagination.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crimson_crab::api::ModelListParams;
    ///
    /// # async fn demo(client: &crimson_crab::Client) -> crimson_crab::Result<()> {
    /// let params = ModelListParams { limit: Some(10), ..Default::default() };
    /// let page = client.models().list(&params).await?;
    /// let _ = page.has_more;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list(&self, params: &ModelListParams) -> Result<ModelPage> {
        let query = params.to_query();
        self.client
            .http()
            .get_json_query("/v1/models", &query, &[])
            .await
    }
}
