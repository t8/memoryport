use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info};
use uc_core::config::Config;
use uc_core::{Engine, EngineError};

/// Per-user Engine cache with LRU eviction.
pub struct EnginePool {
    engines: RwLock<HashMap<String, Arc<Engine>>>,
    creating: Mutex<()>, // Serialize engine creation
    access_order: RwLock<Vec<String>>,
    base_config: Config,
    data_dir: PathBuf,
    max_engines: usize,
}

impl EnginePool {
    pub fn new(base_config: Config, data_dir: PathBuf, max_engines: usize) -> Self {
        Self {
            engines: RwLock::new(HashMap::new()),
            creating: Mutex::new(()),
            access_order: RwLock::new(Vec::new()),
            base_config,
            data_dir,
            max_engines,
        }
    }

    /// Get the base config (used for settings display).
    pub fn base_config(&self) -> &Config {
        &self.base_config
    }

    /// Get or create an Engine for the given user.
    pub async fn get_or_create(&self, user_id: &str) -> Result<Arc<Engine>, EngineError> {
        // Fast path: check cache
        {
            let engines = self.engines.read().await;
            if let Some(engine) = engines.get(user_id) {
                let engine = engine.clone();
                drop(engines);
                self.touch(user_id).await;
                return Ok(engine);
            }
        }

        // Slow path: acquire creation mutex to prevent duplicate creation
        let _guard = self.creating.lock().await;

        // Double-check after acquiring creation lock
        {
            let engines = self.engines.read().await;
            if let Some(engine) = engines.get(user_id) {
                let engine = engine.clone();
                drop(engines);
                self.touch(user_id).await;
                return Ok(engine);
            }
        }

        // Create new engine (only one thread gets here at a time)
        let config = self.config_for_user(user_id);
        info!(user_id, "creating new engine instance");
        let engine = Arc::new(Engine::new(config).await?);

        // Insert into cache
        {
            let mut engines = self.engines.write().await;

            // Evict if at capacity
            if engines.len() >= self.max_engines {
                let order = self.access_order.read().await;
                if let Some(evict_id) = order.first().cloned() {
                    drop(order);
                    debug!(user_id = %evict_id, "evicting engine from pool");
                    engines.remove(&evict_id);
                }
            }

            engines.insert(user_id.to_string(), engine.clone());
        }

        self.touch(user_id).await;
        Ok(engine)
    }

    fn config_for_user(&self, user_id: &str) -> Config {
        let mut config = self.base_config.clone();
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
}
