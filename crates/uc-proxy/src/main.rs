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
    about = "Unlimited Context — OpenAI-compatible API proxy with automatic context injection"
)]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "uc.toml")]
    config: String,

    /// Listen address.
    #[arg(short, long)]
    listen: Option<String>,

    /// Upstream LLM API URL.
    #[arg(short, long)]
    upstream: Option<String>,

    /// Default user ID.
    #[arg(long, default_value = "default")]
    user_id: String,

    /// Default session ID.
    #[arg(long, default_value = "default")]
    session_id: String,
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
    let upstream = cli
        .upstream
        .or_else(|| config.proxy.upstream.clone())
        .unwrap_or_else(|| "https://api.openai.com".into());

    let engine = Arc::new(Engine::new(config).await?);

    let state = Arc::new(ProxyState {
        engine,
        upstream: upstream.clone(),
        http: reqwest::Client::new(),
        user_id: cli.user_id,
        session_id: cli.session_id,
        context_budget: 50_000,
    });

    let app = Router::new()
        .route("/v1/chat/completions", post(routes::proxy_completions))
        .route("/health", get(routes::health))
        .with_state(state);

    tracing::info!(listen = %listen, upstream = %upstream, "starting Unlimited Context proxy");

    let listener = tokio::net::TcpListener::bind(&listen).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
