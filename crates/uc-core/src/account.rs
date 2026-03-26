use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{info, warn};

const CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("API key validation failed: {0}")]
    ValidationFailed(String),
    #[error("storage limit exceeded: {used}/{limit} bytes")]
    StorageLimitExceeded { used: u64, limit: u64 },
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResponse {
    pub valid: bool,
    pub user_id: String,
    pub tier: String,
    pub storage_used_bytes: u64,
    pub storage_limit_bytes: u64,
    #[serde(default)]
    pub funder_address: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedValidation {
    response: ValidationResponse,
    validated_at: Instant,
}

pub struct AccountClient {
    http: reqwest::Client,
    api_endpoint: String,
    api_key: String,
    cached: Arc<RwLock<Option<CachedValidation>>>,
}

impl AccountClient {
    pub fn new(api_endpoint: String, api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_endpoint,
            api_key,
            cached: Arc::new(RwLock::new(None)),
        }
    }

    /// Validate the API key with the server, registering the wallet address.
    pub async fn validate(
        &self,
        wallet_address: &str,
    ) -> Result<ValidationResponse, AccountError> {
        let url = format!("{}/validate", self.api_endpoint);

        let resp = self
            .http
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(&serde_json::json!({ "wallet_address": wallet_address }))
            .send()
            .await?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AccountError::ValidationFailed("invalid API key".into()));
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            return Err(AccountError::ValidationFailed(
                body["error"].as_str().unwrap_or("forbidden").into(),
            ));
        }
        if !status.is_success() {
            return Err(AccountError::ValidationFailed(format!(
                "server returned {}",
                status
            )));
        }

        let validation: ValidationResponse = resp.json().await?;

        // Cache the result
        let mut cache = self.cached.write().await;
        *cache = Some(CachedValidation {
            response: validation.clone(),
            validated_at: Instant::now(),
        });

        info!(
            user_id = %validation.user_id,
            tier = %validation.tier,
            "API key validated"
        );

        Ok(validation)
    }

    /// Check if upload is allowed (uses cache, re-validates if stale).
    /// Returns (allowed, funder_address) on success.
    /// On network error with fresh cache, returns cached result.
    /// On network error without cache, returns Err.
    pub async fn is_upload_allowed(
        &self,
        wallet_address: &str,
    ) -> Result<(bool, Option<String>), AccountError> {
        // Check cache first
        {
            let cache = self.cached.read().await;
            if let Some(ref cached) = *cache {
                if cached.validated_at.elapsed() < CACHE_TTL {
                    let resp = &cached.response;
                    if resp.storage_used_bytes >= resp.storage_limit_bytes
                        && resp.storage_limit_bytes > 0
                    {
                        return Err(AccountError::StorageLimitExceeded {
                            used: resp.storage_used_bytes,
                            limit: resp.storage_limit_bytes,
                        });
                    }
                    return Ok((resp.valid && resp.tier == "pro", resp.funder_address.clone()));
                }
            }
        }

        // Cache stale or missing — re-validate
        match self.validate(wallet_address).await {
            Ok(resp) => {
                if resp.storage_used_bytes >= resp.storage_limit_bytes
                    && resp.storage_limit_bytes > 0
                {
                    return Err(AccountError::StorageLimitExceeded {
                        used: resp.storage_used_bytes,
                        limit: resp.storage_limit_bytes,
                    });
                }
                Ok((resp.valid && resp.tier == "pro", resp.funder_address))
            }
            Err(AccountError::Http(e)) => {
                // Network error — try stale cache
                let cache = self.cached.read().await;
                if let Some(ref cached) = *cache {
                    warn!("API validation failed ({}), using stale cache", e);
                    let resp = &cached.response;
                    Ok((resp.valid && resp.tier == "pro", resp.funder_address.clone()))
                } else {
                    Err(AccountError::Http(e))
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Report upload usage to the server. Best-effort — logs warning on failure.
    pub async fn report_usage(&self, bytes_stored: u64, tx_id: &str) {
        let url = format!("{}/usage", self.api_endpoint);

        match self
            .http
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(&serde_json::json!({
                "bytes_stored": bytes_stored,
                "tx_id": tx_id,
            }))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                // Update cached storage values
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let mut cache = self.cached.write().await;
                    if let Some(ref mut cached) = *cache {
                        if let Some(used) = body["storage_used_bytes"].as_u64() {
                            cached.response.storage_used_bytes = used;
                        }
                    }
                }
            }
            Ok(resp) => {
                warn!(status = %resp.status(), "usage reporting failed");
            }
            Err(e) => {
                warn!(error = %e, "usage reporting failed");
            }
        }
    }
}
