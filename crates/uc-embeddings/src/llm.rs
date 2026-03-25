use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {status} — {message}")]
    Api { status: u16, message: String },
    #[error("unexpected response format: {0}")]
    Format(String),
}

/// Trait for LLM text generation providers.
/// Used for query expansion and HyDE — lightweight, single-turn generation only.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate a text completion given a prompt and optional system message.
    async fn generate(&self, prompt: &str, system: Option<&str>) -> Result<String, LlmError>;

    /// The model name/identifier.
    fn model_name(&self) -> &str;
}
