use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};
use uc_core::config::Config;
use uc_core::{Engine, EngineError};

/// Per-user Engine cache with LRU eviction.
pub struct EnginePool {
    engines: RwLock<HashMap<String, Arc<Engine>>>,
    access_order: RwLock<Vec<String>>,
    base_config: Config,
    data_dir: PathBuf,
    max_engines: usize,
}

impl EnginePool {
    pub fn new(base_config: Config, data_dir: PathBuf, max_engines: usize) -> Self {
        Self {
            engines: RwLock::new(HashMap::new()),
            access_order: RwLock::new(Vec::new()),
            base_config,
            data_dir,
            max_engines,
        }
    }

    /// Get or create an Engine for the given user.
    pub async fn get_or_create(&self, user_id: &str) -> Result<Arc<Engine>, EngineError> {
        // Fast path: read lock
        {
            let engines = self.engines.read().await;
            if let Some(engine) = engines.get(user_id) {
                self.touch(user_id).await;
                return Ok(engine.clone());
            }
        }

        // Slow path: write lock, create engine
        let mut engines = self.engines.write().await;

        // Double-check after acquiring write lock
        if let Some(engine) = engines.get(user_id) {
            self.touch(user_id).await;
            return Ok(engine.clone());
        }

        // Evict if at capacity
        if engines.len() >= self.max_engines {
            if let Some(evict_id) = self.lru_candidate().await {
                debug!(user_id = %evict_id, "evicting engine from pool");
                engines.remove(&evict_id);
            }
        }

        // Create new engine
        let config = self.config_for_user(user_id);
        info!(user_id, "creating new engine instance");
        let engine = Arc::new(Engine::new(config).await?);
        engines.insert(user_id.to_string(), engine.clone());
        self.touch(user_id).await;

        Ok(engine)
    }

    fn config_for_user(&self, user_id: &str) -> Config {
        let mut config = self.base_config.clone();
        // For the "default" user, use the config's original index path
        // (so local dev mode reads the same data as CLI/MCP)
        if user_id != "default" {
            let user_dir = self.data_dir.join(user_id).join("index");
            config.index.path = user_dir.to_string_lossy().to_string();
        }
        config
    }

    async fn touch(&self, user_id: &str) {
        let mut order = self.access_order.write().await;
        order.retain(|id| id != user_id);
        order.push(user_id.to_string());
    }

    async fn lru_candidate(&self) -> Option<String> {
        let order = self.access_order.read().await;
        order.first().cloned()
    }
}
