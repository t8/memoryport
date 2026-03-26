mod agentic;
mod anthropic;
mod models;
mod routes;

use axum::routing::{get, post};
use axum::Router;
use clap::Parser;
use std::sync::Arc;
use uc_core::config::Config;
use uc_core::Engine;

use crate::routes::ProxyState;

#[derive(Parser)]
#[command(
    name = "uc-proxy",
    about = "Memoryport — LLM API proxy with automatic context injection and memory capture"
)]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "uc.toml")]
    config: String,

    /// Listen address.
    #[arg(short, long)]
    listen: Option<String>,

    /// Default user ID.
    #[arg(long, default_value = "default")]
    user_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let config = Config::from_file(&cli.config).unwrap_or_else(|_| {
        tracing::debug!(path = %cli.config, "config file not found, using defaults");
        Config::default_config()
    });

    let listen = cli
        .listen
        .unwrap_or_else(|| config.proxy.listen.clone());

    let agentic_config = routes::HotConfig::new(
        std::path::PathBuf::from(&cli.config),
        config.proxy.agentic.clone(),
    );

    let engine = Arc::new(Engine::new(config).await?);

    let state = Arc::new(ProxyState {
        engine,
        http: reqwest::Client::new(),
        user_id: cli.user_id,
        sessions: routes::SessionManager::new(1800), // 30 min inactivity = new session
        context_budget: 50_000,
        agentic_config,
        no_tool_models: tokio::sync::Mutex::new(std::collections::HashSet::new()),
    });

    let app = Router::new()
        // Anthropic Messages API
        .route("/v1/messages", post(anthropic::proxy_messages))
        // OpenAI Chat Completions API (also used by Ollama)
        .route("/v1/chat/completions", post(routes::proxy_completions))
        // Ollama native API — forward all /api/* to real Ollama
        .route("/api/{*rest}", axum::routing::any(routes::forward_ollama_any))
        // Health (respond to both our format and Ollama's root check)
        .route("/", get(routes::ollama_root))
        .route("/health", get(routes::health))
        .with_state(state);

    tracing::info!(
        listen = %listen,
        "starting Memoryport proxy (Anthropic + OpenAI)"
    );

    let listener = tokio::net::TcpListener::bind(&listen).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
