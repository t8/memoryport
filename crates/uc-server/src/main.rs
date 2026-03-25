mod app;
mod auth;
mod db;
mod error;
mod metrics;
mod models;
mod pool;
mod rate_limit;
mod routes;
mod state;

use clap::Parser;
use std::sync::Arc;
use uc_core::config::Config;

use crate::db::UserDb;
use crate::pool::EnginePool;
use crate::rate_limit::RateLimiter;
use crate::state::{AppState, ServerConfig};

#[derive(Parser)]
#[command(
    name = "uc-server",
    about = "Unlimited Context — multi-tenant hosted API server"
)]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "uc-server.toml")]
    config: String,
}

/// Combined config file: uc-core Config + server-specific settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct FullConfig {
    #[serde(flatten)]
    pub core: Config,
    #[serde(default)]
    pub server: ServerConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Load config
    let mut full_config: FullConfig = match std::fs::read_to_string(&cli.config) {
        Ok(content) => toml::from_str(&content)?,
        Err(_) => {
            tracing::warn!(path = %cli.config, "config file not found, using defaults");
            FullConfig {
                core: Config::default_config(),
                server: ServerConfig::default(),
            }
        }
    };

    // Apply environment variable overrides
    apply_env_overrides(&mut full_config);

    let server_config = full_config.server.clone();
    let data_dir = server_config.resolved_data_dir();
    std::fs::create_dir_all(&data_dir)?;

    // Init metrics
    let metrics_handle = if server_config.metrics_enabled {
        metrics::init_metrics()
    } else {
        // Still need a handle even if disabled — just don't export
        metrics_exporter_prometheus::PrometheusBuilder::new()
            .install_recorder()
            .expect("failed to install metrics recorder")
    };

    // Open user database
    let db_path = data_dir.join("users.db");
    let user_db = Arc::new(
        UserDb::open(&db_path).map_err(|e| anyhow::anyhow!("failed to open user db: {e}"))?,
    );

    // Create engine pool
    let pool = Arc::new(EnginePool::new(
        full_config.core.clone(),
        data_dir.clone(),
        server_config.max_engines,
    ));

    // Create rate limiter
    let rate_limiter = Arc::new(RateLimiter::new(server_config.rate_limit_rps));

    let state = Arc::new(AppState {
        pool,
        user_db,
        rate_limiter,
        server_config: server_config.clone(),
    });

    let app = app::build_router(state, metrics_handle);

    tracing::info!(
        listen = %server_config.listen,
        data_dir = %data_dir.display(),
        max_engines = server_config.max_engines,
        rate_limit_rps = server_config.rate_limit_rps,
        "starting Unlimited Context server"
    );

    let listener = tokio::net::TcpListener::bind(&server_config.listen).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn apply_env_overrides(config: &mut FullConfig) {
    if let Ok(v) = std::env::var("UC_SERVER_LISTEN") { config.server.listen = v; }
    if let Ok(v) = std::env::var("UC_SERVER_DATA_DIR") { config.server.data_dir = v; }
    if let Ok(v) = std::env::var("UC_SERVER_MAX_ENGINES") { if let Ok(n) = v.parse() { config.server.max_engines = n; } }
    if let Ok(v) = std::env::var("UC_ADMIN_API_KEY") { config.server.admin_api_key = Some(v); }
    if let Ok(v) = std::env::var("UC_RATE_LIMIT_RPS") { if let Ok(n) = v.parse() { config.server.rate_limit_rps = n; } }
    if let Ok(v) = std::env::var("UC_ARWEAVE_GATEWAY") { config.core.arweave.gateway = v; }
    if let Ok(v) = std::env::var("UC_ARWEAVE_TURBO_ENDPOINT") { config.core.arweave.turbo_endpoint = v; }
    if let Ok(v) = std::env::var("UC_ARWEAVE_WALLET_PATH") { config.core.arweave.wallet_path = Some(v); }
    if let Ok(v) = std::env::var("UC_EMBEDDINGS_PROVIDER") { config.core.embeddings.provider = v; }
    if let Ok(v) = std::env::var("UC_EMBEDDINGS_MODEL") { config.core.embeddings.model = v; }
    if let Ok(v) = std::env::var("UC_EMBEDDINGS_DIMENSIONS") { if let Ok(n) = v.parse() { config.core.embeddings.dimensions = n; } }
}
