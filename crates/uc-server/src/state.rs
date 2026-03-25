use crate::db::UserDb;
use crate::pool::EnginePool;
use crate::rate_limit::RateLimiter;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

pub struct AppState {
    pub pool: Arc<EnginePool>,
    pub user_db: Arc<UserDb>,
    pub rate_limiter: Arc<RateLimiter>,
    pub server_config: ServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default = "default_max_engines")]
    pub max_engines: usize,
    pub admin_api_key: Option<String>,
    #[serde(default = "default_request_body_limit")]
    pub request_body_limit: usize,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_rate_limit_rps")]
    pub rate_limit_rps: u32,
    #[serde(default = "default_metrics_enabled")]
    pub metrics_enabled: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            data_dir: default_data_dir(),
            max_engines: default_max_engines(),
            admin_api_key: None,
            request_body_limit: default_request_body_limit(),
            request_timeout_secs: default_request_timeout_secs(),
            rate_limit_rps: default_rate_limit_rps(),
            metrics_enabled: default_metrics_enabled(),
        }
    }
}

fn default_listen() -> String { "0.0.0.0:8080".into() }
fn default_data_dir() -> String { "~/.memoryport-server".into() }
fn default_max_engines() -> usize { 100 }
fn default_request_body_limit() -> usize { 10 * 1024 * 1024 } // 10MB
fn default_request_timeout_secs() -> u64 { 30 }
fn default_rate_limit_rps() -> u32 { 60 }
fn default_metrics_enabled() -> bool { true }

impl ServerConfig {
    pub fn resolved_data_dir(&self) -> PathBuf {
        let path = &self.data_dir;
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = directories::BaseDirs::new() {
                return home.home_dir().join(stripped);
            }
        }
        PathBuf::from(path)
    }
}
