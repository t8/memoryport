mod init;

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
    /// Interactive setup wizard — configure provider, register MCP, done.
    Init,

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

    /// Query stored content with full retrieval pipeline (retrieve + rerank + assemble).
    Query {
        /// The search query text.
        text: String,

        /// User ID.
        #[arg(short, long, default_value = "default")]
        user_id: String,

        /// Active session ID for recency and session affinity.
        #[arg(short, long)]
        session_id: Option<String>,

        /// Max tokens for assembled context.
        #[arg(short = 'm', long, default_value = "50000")]
        max_tokens: u32,
    },

    /// Retrieve raw ranked results (without context assembly).
    Retrieve {
        /// The search query text.
        text: String,

        /// User ID.
        #[arg(short, long, default_value = "default")]
        user_id: String,

        /// Active session ID.
        #[arg(short, long)]
        session_id: Option<String>,

        /// Number of results to show.
        #[arg(short = 'k', long, default_value = "10")]
        top_k: usize,
    },

    /// Rebuild the local index from Arweave.
    RebuildIndex {
        /// User ID to rebuild for.
        #[arg(short, long)]
        user_id: String,
    },

    /// Logically delete a batch by destroying its encryption key.
    Delete {
        /// Arweave transaction ID of the batch to delete.
        #[arg(long)]
        tx_id: String,
    },

    /// Start the auto-capture proxy (Anthropic + OpenAI).
    Proxy,

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

    // Init and Proxy don't need an engine — handle early
    if matches!(cli.command, Commands::Init) {
        return init::run_init();
    }
    if matches!(cli.command, Commands::Proxy) {
        return run_proxy(&cli.config);
    }

    let config = Config::from_file(&cli.config).unwrap_or_else(|_| {
        tracing::debug!(
            path = %cli.config,
            "config file not found, using defaults"
        );
        Config::default_config()
    });

    let engine = Engine::new(config).await?;

    match cli.command {
        Commands::Init => unreachable!(),
        Commands::Proxy => unreachable!(),
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
                source_integration: Some("cli".into()),
                source_model: None,
                timestamp: None,
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
            session_id,
            max_tokens,
        } => {
            let context = engine
                .query(&text, &user_id, session_id.as_deref(), max_tokens)
                .await?;

            if context.chunks_included == 0 {
                println!("No results found.");
            } else {
                println!(
                    "--- Assembled Context ({} chunks, ~{} tokens) ---\n",
                    context.chunks_included, context.token_count
                );
                println!("{}", context.formatted);
            }
        }

        Commands::Retrieve {
            text,
            user_id,
            session_id,
            top_k,
        } => {
            let results = engine
                .retrieve(&text, &user_id, session_id.as_deref())
                .await?;

            if results.is_empty() {
                println!("No results found.");
            } else {
                for (i, r) in results.iter().take(top_k).enumerate() {
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

        Commands::RebuildIndex { user_id } => {
            println!("Rebuilding index for user '{user_id}' from Arweave...");
            let progress = engine.rebuild_index(&user_id).await?;
            println!("Rebuild complete:");
            println!("  Transactions found:     {}", progress.transactions_found);
            println!("  Transactions processed: {}", progress.transactions_processed);
            println!("  Chunks indexed:         {}", progress.chunks_indexed);
            println!("  Errors:                 {}", progress.errors);
        }

        Commands::Delete { tx_id } => {
            match engine.delete_batch(&tx_id).await {
                Ok(true) => println!("Batch key destroyed for tx {tx_id}. Data is now permanently unreadable."),
                Ok(false) => println!("No batch key found for tx {tx_id}."),
                Err(e) => println!("Delete failed: {e}"),
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

fn run_proxy(config_path: &str) -> anyhow::Result<()> {
    // Find the uc-proxy binary next to this binary
    let proxy_bin = if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("uc-proxy");
            if sibling.exists() {
                sibling.to_string_lossy().to_string()
            } else {
                "uc-proxy".into()
            }
        } else {
            "uc-proxy".into()
        }
    } else {
        "uc-proxy".into()
    };

    println!("Starting Memoryport proxy...");
    let status = std::process::Command::new(&proxy_bin)
        .arg("--config")
        .arg(config_path)
        .status()?;

    if !status.success() {
        anyhow::bail!("proxy exited with status {}", status);
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}
