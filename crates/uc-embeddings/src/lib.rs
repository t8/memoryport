pub mod llm;
pub mod ollama;
pub mod openai;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {status} — {message}")]
    Api { status: u16, message: String },
    #[error("unexpected response format: {0}")]
    Format(String),
}

/// Trait for embedding providers.
///
/// Implementations should be thread-safe and cloneable (via Arc wrapping).
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;

    /// Embed a batch of texts. Default implementation calls `embed` sequentially.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// The dimensionality of the embedding vectors.
    fn dimensions(&self) -> usize;

    /// The model name/identifier.
    fn model_name(&self) -> &str;
}
