use crate::{EmbeddingError, EmbeddingProvider};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Ollama embedding client using the /api/embed endpoint.
pub struct OllamaEmbeddings {
    http: reqwest::Client,
    api_base: String,
    model: String,
    dims: usize,
}

#[derive(Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl OllamaEmbeddings {
    pub fn new(
        model: impl Into<String>,
        dimensions: usize,
        api_base: Option<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_base: api_base.unwrap_or_else(|| "http://localhost:11434".into()),
            model: model.into(),
            dims: dimensions,
        }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OllamaEmbeddings {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let results = self.embed_batch(&[text]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::Format("empty response".into()))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/api/embed", self.api_base);

        let request = OllamaEmbedRequest {
            model: self.model.clone(),
            input: texts.iter().map(|t| t.to_string()).collect(),
        };

        debug!(model = %self.model, count = texts.len(), "requesting embeddings from Ollama");

        let response = self.http.post(&url).json(&request).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Api {
                status: status.as_u16(),
                message: body,
            });
        }

        let resp: OllamaEmbedResponse = response
            .json()
            .await
            .map_err(|e| EmbeddingError::Format(e.to_string()))?;

        Ok(resp.embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
