use crate::{EmbeddingError, EmbeddingProvider};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// OpenAI-compatible embedding client.
/// Works with OpenAI, Azure OpenAI, and any compatible endpoint.
pub struct OpenAiEmbeddings {
    http: reqwest::Client,
    api_base: String,
    api_key: String,
    model: String,
    dims: usize,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    input: Vec<String>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

impl OpenAiEmbeddings {
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        dimensions: usize,
        api_base: Option<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_base: api_base.unwrap_or_else(|| "https://api.openai.com/v1".into()),
            api_key: api_key.into(),
            model: model.into(),
            dims: dimensions,
        }
    }

    async fn request_embeddings(
        &self,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let url = format!("{}/embeddings", self.api_base);

        let request = EmbeddingRequest {
            input: texts,
            model: self.model.clone(),
            dimensions: Some(self.dims),
        };

        debug!(model = %self.model, "requesting embeddings from OpenAI");

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Api {
                status: status.as_u16(),
                message: body,
            });
        }

        let resp: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| EmbeddingError::Format(e.to_string()))?;

        // Sort by index to preserve input ordering
        let mut data = resp.data;
        data.sort_by_key(|d| d.index);

        Ok(data.into_iter().map(|d| d.embedding).collect())
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OpenAiEmbeddings {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let results = self.request_embeddings(vec![text.to_string()]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::Format("empty response".into()))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let texts: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        self.request_embeddings(texts).await
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
