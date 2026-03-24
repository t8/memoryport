use clap::{Parser, Subcommand};
use uc_core::config::Config;
use uc_core::models::*;
use uc_core::Engine;

#[derive(Parser)]
#[command(name = "uc", about = "Unlimited Context — persistent LLM memory on Arweave")]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "uc.toml")]
    config: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Store text content in Unlimited Context.
    Store {
        /// The text to store.
        text: String,

        /// User ID.
        #[arg(short, long, default_value = "default")]
        user_id: String,

        /// Session ID.
        #[arg(short, long, default_value = "default")]
        session_id: String,

        /// Chunk type: conversation, document, or knowledge.
        #[arg(short = 't', long, default_value = "conversation")]
        chunk_type: String,

        /// Role: user, assistant, or system.
        #[arg(short, long)]
        role: Option<String>,
    },

    /// Query stored content by semantic similarity.
    Query {
        /// The search query text.
        text: String,

        /// User ID.
        #[arg(short, long, default_value = "default")]
        user_id: String,

        /// Number of results to return.
        #[arg(short = 'k', long, default_value = "5")]
        top_k: usize,

        /// Filter by session ID.
        #[arg(short, long)]
        session_id: Option<String>,
    },

    /// Show engine status.
    Status,

    /// Flush any buffered chunks immediately.
    Flush,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let config = Config::from_file(&cli.config).unwrap_or_else(|_| {
        tracing::debug!(
            path = %cli.config,
            "config file not found, using defaults"
        );
        Config::default_config()
    });

    let engine = Engine::new(config).await?;

    match cli.command {
        Commands::Store {
            text,
            user_id,
            session_id,
            chunk_type,
            role,
        } => {
            let chunk_type: ChunkType = chunk_type
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?;
            let role = role
                .map(|r| r.parse::<Role>())
                .transpose()
                .map_err(|e: String| anyhow::anyhow!(e))?;

            let params = StoreParams {
                user_id,
                session_id,
                chunk_type,
                role,
            };
            let ids = engine.store(&text, params).await?;
            engine.flush().await?;
            println!("Stored {} chunk(s):", ids.len());
            for id in &ids {
                println!("  {id}");
            }
        }

        Commands::Query {
            text,
            user_id,
            top_k,
            session_id,
        } => {
            let params = QueryParams {
                user_id,
                top_k,
                session_id,
                chunk_type: None,
            };
            let results = engine.query(&text, params).await?;

            if results.is_empty() {
                println!("No results found.");
            } else {
                for (i, r) in results.iter().enumerate() {
                    println!("--- Result {} (score: {:.4}) ---", i + 1, r.score);
                    println!("  Chunk:   {}", r.chunk_id);
                    println!("  Session: {}", r.session_id);
                    println!("  Type:    {}", r.chunk_type);
                    if let Some(ref role) = r.role {
                        println!("  Role:    {role}");
                    }
                    println!("  Time:    {}", r.timestamp);
                    println!("  Content: {}", truncate(&r.content, 200));
                    println!();
                }
            }
        }

        Commands::Status => {
            let status = engine.status().await?;
            println!("Pending chunks:      {}", status.pending_chunks);
            println!("Indexed chunks:      {}", status.indexed_chunks);
            println!("Index path:          {}", status.index_path);
            println!(
                "Embedding model:     {} ({}d)",
                status.embedding_model, status.embedding_dimensions
            );
        }

        Commands::Flush => {
            engine.flush().await?;
            println!("Flushed.");
        }
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        // Find a char boundary
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}
