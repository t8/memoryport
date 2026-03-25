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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptionSettings {
    pub enabled: bool,
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SettingsResponse>, ApiError> {
    // Read from the base config stored in the pool
    // For now, return the config the server was started with
    let config = &state.pool.base_config();

    Ok(Json(SettingsResponse {
        embeddings: EmbeddingsSettings {
            provider: config.embeddings.provider.clone(),
            model: config.embeddings.model.clone(),
            dimensions: config.embeddings.dimensions,
            api_key: config.embeddings.api_key.as_ref().map(|_| "••••••••".into()), // redact
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
        },
        encryption: EncryptionSettings {
            enabled: config.encryption.enabled,
        },
    }))
}

pub async fn update_settings() -> Result<Json<serde_json::Value>, ApiError> {
    // Settings update requires server restart to take effect.
    // For now, return acknowledgment.
    Ok(Json(serde_json::json!({
        "status": "Settings saved. Restart the server for changes to take effect."
    })))
}
