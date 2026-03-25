use crate::llm::{LlmError, LlmProvider};
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

// -- OpenAI LLM (text generation) --

/// OpenAI-compatible text generation client for query expansion / HyDE.
pub struct OpenAiLlm {
    http: reqwest::Client,
    api_base: String,
    api_key: String,
    model: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

impl OpenAiLlm {
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        api_base: Option<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_base: api_base.unwrap_or_else(|| "https://api.openai.com/v1".into()),
            api_key: api_key.into(),
            model: model.into(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiLlm {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> Result<String, LlmError> {
        let url = format!("{}/chat/completions", self.api_base);

        let mut messages = Vec::new();
        if let Some(sys) = system {
            messages.push(ChatMessage {
                role: "system".into(),
                content: sys.into(),
            });
        }
        messages.push(ChatMessage {
            role: "user".into(),
            content: prompt.into(),
        });

        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            temperature: Some(0.7),
            max_tokens: Some(500),
        };

        debug!(model = %self.model, "requesting LLM generation");

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
            return Err(LlmError::Api {
                status: status.as_u16(),
                message: body,
            });
        }

        let resp: ChatResponse = response
            .json()
            .await
            .map_err(|e| LlmError::Format(e.to_string()))?;

        resp.choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LlmError::Format("empty response".into()))
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
