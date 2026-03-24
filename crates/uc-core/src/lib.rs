pub mod batcher;
pub mod chunker;
pub mod config;
pub mod index;
pub mod models;
pub mod tagger;
pub mod writer;

use crate::batcher::{Batcher, FlushCallback};
use crate::chunker::ChunkerConfig;
use crate::config::Config;
use crate::index::Index;
use crate::models::*;
use crate::writer::Writer;
use std::sync::Arc;
use thiserror::Error;
use tokio::time::Duration;
use tracing::info;
use uc_arweave::{ArweaveClient, Wallet};
use uc_embeddings::EmbeddingProvider;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("config error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("index error: {0}")]
    Index(#[from] index::IndexError),
    #[error("writer error: {0}")]
    Writer(#[from] writer::WriterError),
    #[error("embedding error: {0}")]
    Embedding(#[from] uc_embeddings::EmbeddingError),
    #[error("batcher error: {0}")]
    Batcher(#[from] batcher::BatcherError),
    #[error("wallet error: {0}")]
    Wallet(#[from] uc_arweave::wallet::WalletError),
}

/// The main entry point for the Unlimited Context engine.
pub struct Engine {
    config: Config,
    index: Arc<Index>,
    embeddings: Arc<dyn EmbeddingProvider>,
    #[allow(dead_code)]
    writer: Arc<Writer>,
    batcher: Batcher,
    chunker_config: ChunkerConfig,
}

impl Engine {
    /// Initialize the engine from a config.
    pub async fn new(config: Config) -> Result<Self, EngineError> {
        // Create embedding provider
        let embeddings: Arc<dyn EmbeddingProvider> = create_embedding_provider(&config.embeddings);

        // Open/create LanceDB index
        let index_path = config.resolved_index_path();
        if let Some(parent) = index_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let index = Arc::new(
            Index::open(&index_path, config.index.embedding_dimensions).await?,
        );

        // Create Arweave client (read-only if no wallet)
        let arweave = if config.arweave.wallet_path.is_some() {
            let resolved = config.resolved_wallet_path().unwrap();
            let wallet = Wallet::from_file(&resolved)?;
            info!(address = %wallet.address, "loaded Arweave wallet");
            ArweaveClient::new(wallet, &config.arweave.turbo_endpoint, &config.arweave.gateway)
        } else {
            ArweaveClient::read_only(&config.arweave.turbo_endpoint, &config.arweave.gateway)
        };

        let writer = Arc::new(Writer::new(arweave, "default".into()));

        // Create batcher with flush callback
        let flush_writer = writer.clone();
        let flush_index = index.clone();
        let flush_embeddings = embeddings.clone();

        let on_flush: FlushCallback = Arc::new(move |batch: Batch| {
            let writer = flush_writer.clone();
            let index = flush_index.clone();
            let embeddings = flush_embeddings.clone();
            Box::pin(async move {
                // 1. Compute embeddings
                let texts: Vec<&str> = batch.chunks.iter().map(|c| c.content.as_str()).collect();
                let vectors = embeddings.embed_batch(&texts).await.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

                // 2. Upload to Arweave
                let receipt = writer.write_batch(&batch).await.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

                // 3. Insert into LanceDB index
                let entries: Vec<(Chunk, Vec<f32>, String, u32)> = batch
                    .chunks
                    .iter()
                    .zip(vectors.iter())
                    .enumerate()
                    .map(|(i, (chunk, vec))| {
                        (chunk.clone(), vec.clone(), receipt.tx_id.clone(), i as u32)
                    })
                    .collect();
                index.insert(&entries).await.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

                Ok(())
            })
        });

        let batcher = Batcher::new(50, Duration::from_secs(60), on_flush);

        Ok(Self {
            config,
            index,
            embeddings,
            writer,
            batcher,
            chunker_config: ChunkerConfig::default(),
        })
    }

    /// Store text content. Chunks it and buffers in the batcher.
    pub async fn store(
        &self,
        text: &str,
        params: StoreParams,
    ) -> Result<Vec<Uuid>, EngineError> {
        let timestamp = chrono::Utc::now().timestamp_millis();
        let chunks = chunker::chunk_text(
            text,
            &params.session_id,
            params.chunk_type,
            params.role,
            &self.chunker_config,
            timestamp,
        );

        let ids: Vec<Uuid> = chunks.iter().map(|c| c.id).collect();

        self.batcher.add_many(chunks).await?;

        Ok(ids)
    }

    /// Query stored content by semantic similarity.
    pub async fn query(
        &self,
        text: &str,
        params: QueryParams,
    ) -> Result<Vec<SearchResult>, EngineError> {
        let query_vector = self.embeddings.embed(text).await?;
        let results = self.index.search(&query_vector, &params).await?;
        Ok(results)
    }

    /// Force-flush any buffered chunks.
    pub async fn flush(&self) -> Result<(), EngineError> {
        self.batcher.flush().await?;
        Ok(())
    }

    /// Return engine status.
    pub async fn status(&self) -> Result<EngineStatus, EngineError> {
        let pending = self.batcher.pending_count().await;
        let indexed = self.index.count(None).await?;

        Ok(EngineStatus {
            pending_chunks: pending,
            indexed_chunks: indexed,
            index_path: self.config.resolved_index_path().to_string_lossy().to_string(),
            embedding_model: self.embeddings.model_name().to_string(),
            embedding_dimensions: self.embeddings.dimensions(),
        })
    }
}

/// Engine status information.
#[derive(Debug, Clone)]
pub struct EngineStatus {
    pub pending_chunks: usize,
    pub indexed_chunks: usize,
    pub index_path: String,
    pub embedding_model: String,
    pub embedding_dimensions: usize,
}

fn create_embedding_provider(config: &config::EmbeddingsConfig) -> Arc<dyn EmbeddingProvider> {
    match config.provider.as_str() {
        "ollama" => Arc::new(uc_embeddings::ollama::OllamaEmbeddings::new(
            &config.model,
            config.dimensions,
            config.api_base.clone(),
        )),
        _ => {
            // Default to OpenAI
            let api_key = config
                .api_key
                .clone()
                .or_else(|| std::env::var("OPENAI_API_KEY").ok())
                .unwrap_or_default();
            Arc::new(uc_embeddings::openai::OpenAiEmbeddings::new(
                api_key,
                &config.model,
                config.dimensions,
                config.api_base.clone(),
            ))
        }
    }
}
