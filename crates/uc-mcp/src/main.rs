mod server;

use clap::Parser;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use std::sync::Arc;
use uc_core::config::Config;
use uc_core::Engine;

use crate::server::UcMcpServer;

#[derive(Parser)]
#[command(name = "uc-mcp", about = "Unlimited Context MCP server")]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "uc.toml")]
    config: String,

    /// Default user ID for operations.
    #[arg(short, long, default_value = "default")]
    user_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to stderr — stdout is the JSON-RPC channel
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let config = Config::from_file(&cli.config).unwrap_or_else(|_| {
        tracing::debug!(path = %cli.config, "config file not found, using defaults");
        Config::default_config()
    });

    let engine = Arc::new(Engine::new(config).await?);
    let user_id = if cli.user_id == "default" {
        engine.user_id().to_string()
    } else {
        cli.user_id
    };
    let server = UcMcpServer::new(engine, user_id);

    tracing::info!("starting Unlimited Context MCP server");

    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
