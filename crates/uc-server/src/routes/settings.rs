use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub embeddings: EmbeddingsSettings,
    pub retrieval: RetrievalSettings,
    pub arweave: ArweaveSettings,
    pub encryption: EncryptionSettings,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbeddingsSettings {
    pub provider: String,
    pub model: String,
    pub dimensions: usize,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RetrievalSettings {
    pub gating_enabled: bool,
    pub similarity_top_k: usize,
    pub recency_window: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArweaveSettings {
    pub gateway: String,
    pub wallet_path: Option<String>,
    pub api_key: Option<String>,
    pub api_endpoint: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptionSettings {
    pub enabled: bool,
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SettingsResponse>, ApiError> {
    // Read config fresh from disk so we pick up saves without restart
    let disk_config = if state.config_path.exists() {
        uc_core::config::Config::from_file(&state.config_path).ok()
    } else {
        None
    };
    let config = disk_config.as_ref().unwrap_or_else(|| state.pool.base_config());

    // Check for API key in config or env
    let has_api_key = config.resolved_api_key().is_some();

    // Resolve wallet address
    let address = config.resolved_wallet_path()
        .or_else(|| {
            if has_api_key {
                Some(uc_core::config::expand_tilde_pub("~/.memoryport/wallet.json"))
            } else {
                None
            }
        })
        .filter(|p| p.exists())
        .and_then(|p| uc_arweave::Wallet::from_file(&p).ok())
        .map(|w| w.address.clone());

    Ok(Json(SettingsResponse {
        embeddings: EmbeddingsSettings {
            provider: config.embeddings.provider.clone(),
            model: config.embeddings.model.clone(),
            dimensions: config.embeddings.dimensions,
            api_key: config.embeddings.api_key.as_ref().map(|_| "••••••••".into()),
            api_base: config.embeddings.api_base.clone(),
        },
        retrieval: RetrievalSettings {
            gating_enabled: config.retrieval.gating_enabled,
            similarity_top_k: config.retrieval.similarity_top_k,
            recency_window: config.retrieval.recency_window,
        },
        arweave: ArweaveSettings {
            gateway: config.arweave.gateway.clone(),
            wallet_path: config.arweave.wallet_path.clone(),
            api_key: if has_api_key { Some("••••••••".into()) } else { None },
            api_endpoint: config.arweave.api_endpoint.clone(),
            address,
        },
        encryption: EncryptionSettings {
            enabled: config.encryption.enabled,
        },
    }))
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SettingsUpdate>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Read current config file, update fields, write back
    let config_path = &state.config_path;
    let mut config = if config_path.exists() {
        let content = std::fs::read_to_string(config_path)
            .map_err(|e| ApiError::Internal(format!("failed to read config: {e}")))?;
        toml::from_str::<toml::Value>(&content)
            .unwrap_or_else(|_| toml::Value::Table(toml::map::Map::new()))
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let table = config.as_table_mut().unwrap();

    // Update arweave section
    if let Some(ref arweave) = body.arweave {
        let section = table
            .entry("arweave")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if let Some(section) = section.as_table_mut() {
            if let Some(ref key) = arweave.api_key {
                if key != "••••••••" && !key.is_empty() {
                    section.insert("api_key".into(), toml::Value::String(key.clone()));
                    // Auto-set wallet_path if not already set
                    if !section.contains_key("wallet_path") {
                        section.insert(
                            "wallet_path".into(),
                            toml::Value::String("~/.memoryport/wallet.json".into()),
                        );
                    }
                }
            }
        }
    }

    // Update embeddings section
    if let Some(ref embeddings) = body.embeddings {
        let section = table
            .entry("embeddings")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if let Some(section) = section.as_table_mut() {
            section.insert("provider".into(), toml::Value::String(embeddings.provider.clone()));
            section.insert("model".into(), toml::Value::String(embeddings.model.clone()));
            section.insert("dimensions".into(), toml::Value::Integer(embeddings.dimensions as i64));
            if let Some(ref key) = embeddings.api_key {
                if key != "••••••••" && !key.is_empty() {
                    section.insert("api_key".into(), toml::Value::String(key.clone()));
                }
            }
        }
    }

    // Update retrieval section
    if let Some(ref retrieval) = body.retrieval {
        let section = table
            .entry("retrieval")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if let Some(section) = section.as_table_mut() {
            section.insert("gating_enabled".into(), toml::Value::Boolean(retrieval.gating_enabled));
        }
    }

    // Update encryption section
    if let Some(ref encryption) = body.encryption {
        let section = table
            .entry("encryption")
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        if let Some(section) = section.as_table_mut() {
            section.insert("enabled".into(), toml::Value::Boolean(encryption.enabled));
        }
    }

    // Write back
    let toml_str = toml::to_string_pretty(&config)
        .map_err(|e| ApiError::Internal(format!("failed to serialize config: {e}")))?;
    std::fs::write(config_path, &toml_str)
        .map_err(|e| ApiError::Internal(format!("failed to write config: {e}")))?;

    Ok(Json(serde_json::json!({
        "status": "Settings saved. Restart the server for changes to take effect."
    })))
}

#[derive(Debug, Deserialize)]
pub struct SettingsUpdate {
    pub embeddings: Option<EmbeddingsSettings>,
    pub retrieval: Option<RetrievalSettings>,
    pub arweave: Option<ArweaveSettingsUpdate>,
    pub encryption: Option<EncryptionSettings>,
}

#[derive(Debug, Deserialize)]
pub struct ArweaveSettingsUpdate {
    pub api_key: Option<String>,
}
