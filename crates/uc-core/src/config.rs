use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub arweave: ArweaveConfig,
    #[serde(default)]
    pub index: IndexConfig,
    #[serde(default)]
    pub retrieval: RetrievalConfig,
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub encryption: EncryptionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArweaveConfig {
    #[serde(default = "default_gateway")]
    pub gateway: String,
    #[serde(default = "default_turbo_endpoint")]
    pub turbo_endpoint: String,
    pub wallet_path: Option<String>,
}

fn default_gateway() -> String {
    "https://arweave.net".into()
}

fn default_turbo_endpoint() -> String {
    "https://upload.ardrive.io".into()
}

impl Default for ArweaveConfig {
    fn default() -> Self {
        Self {
            gateway: default_gateway(),
            turbo_endpoint: default_turbo_endpoint(),
            wallet_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    #[serde(default = "default_index_path")]
    pub path: String,
    #[serde(default = "default_embedding_dimensions")]
    pub embedding_dimensions: usize,
}

fn default_index_path() -> String {
    "~/.unlimited-context/index".into()
}

fn default_embedding_dimensions() -> usize {
    1536
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            path: default_index_path(),
            embedding_dimensions: default_embedding_dimensions(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: usize,
    #[serde(default = "default_recency_window")]
    pub recency_window: usize,
    #[serde(default = "default_similarity_top_k")]
    pub similarity_top_k: usize,
    #[serde(default)]
    pub rerank: bool,
    #[serde(default)]
    pub query_expansion: bool,
    #[serde(default)]
    pub hyde: bool,
    /// LLM provider for query expansion / HyDE: "openai" or "ollama".
    pub llm_provider: Option<String>,
    /// LLM model for query expansion / HyDE (e.g., "gpt-4o-mini").
    pub llm_model: Option<String>,
}

fn default_max_context_tokens() -> usize {
    50_000
}

fn default_recency_window() -> usize {
    20
}

fn default_similarity_top_k() -> usize {
    50
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: default_max_context_tokens(),
            recency_window: default_recency_window(),
            similarity_top_k: default_similarity_top_k(),
            rerank: false,
            query_expansion: false,
            hyde: false,
            llm_provider: None,
            llm_model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_embedding_dimensions")]
    pub dimensions: usize,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
}

fn default_provider() -> String {
    "openai".into()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".into()
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: default_embedding_model(),
            dimensions: default_embedding_dimensions(),
            api_key: None,
            api_base: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    pub upstream: Option<String>,
}

fn default_listen() -> String {
    "127.0.0.1:8080".into()
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            upstream: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Environment variable name containing the master passphrase.
    #[serde(default = "default_passphrase_env")]
    pub passphrase_env: String,
}

fn default_passphrase_env() -> String {
    "UC_MASTER_PASSPHRASE".into()
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            passphrase_env: default_passphrase_env(),
        }
    }
}

impl Config {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        Self {
            arweave: ArweaveConfig::default(),
            index: IndexConfig::default(),
            retrieval: RetrievalConfig::default(),
            embeddings: EmbeddingsConfig::default(),
            proxy: ProxyConfig::default(),
            encryption: EncryptionConfig::default(),
        }
    }

    /// Resolve the index path, expanding ~ to the home directory.
    pub fn resolved_index_path(&self) -> PathBuf {
        expand_tilde(&self.index.path)
    }

    /// Resolve the wallet path, expanding ~ to the home directory.
    pub fn resolved_wallet_path(&self) -> Option<PathBuf> {
        self.arweave.wallet_path.as_ref().map(|p| expand_tilde(p))
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = directories::BaseDirs::new() {
            return home.home_dir().join(stripped);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default_config();
        assert_eq!(config.arweave.gateway, "https://arweave.net");
        assert_eq!(config.embeddings.provider, "openai");
        assert_eq!(config.index.embedding_dimensions, 1536);
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
[arweave]
gateway = "https://custom.gateway"

[embeddings]
provider = "ollama"
model = "nomic-embed-text"
dimensions = 768
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.arweave.gateway, "https://custom.gateway");
        assert_eq!(config.embeddings.provider, "ollama");
        assert_eq!(config.embeddings.dimensions, 768);
    }
}
