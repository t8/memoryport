pub mod account;
pub mod analytics;
pub mod analyzer;
pub mod assembler;
pub mod batcher;
pub mod chunker;
pub mod config;
pub mod contradiction;
pub mod crypto;
pub mod enhancer;
pub mod entities;
pub mod facts;
pub mod gate;
pub mod graph;
pub mod index;
pub mod keystore;
pub mod models;
pub mod profile;
pub mod rebuild;
pub mod reranker;
pub mod retriever;
pub mod tagger;
pub mod writer;

use crate::batcher::{Batcher, FlushCallback};
use crate::chunker::ChunkerConfig;
use crate::config::Config;
use crate::index::Index;
use crate::models::*;
use crate::reranker::{HeuristicReranker, Reranker};
use crate::retriever::Retriever;
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
    #[error("retriever error: {0}")]
    Retriever(#[from] retriever::RetrieverError),
    #[error("reranker error: {0}")]
    Reranker(#[from] reranker::RerankerError),
    #[error("rebuild error: {0}")]
    Rebuild(#[from] rebuild::RebuildError),
    #[error("crypto error: {0}")]
    Crypto(#[from] crypto::CryptoError),
    #[error("keystore error: {0}")]
    KeyStore(#[from] keystore::KeyStoreError),
}

/// The main entry point for the Unlimited Context engine.
pub struct Engine {
    config: Config,
    index: Arc<Index>,
    embeddings: Arc<dyn EmbeddingProvider>,
    arweave: Arc<ArweaveClient>,
    master_key: Option<crypto::MasterKey>,
    keystore: Option<Arc<keystore::KeyStore>>,
    retriever: Retriever,
    reranker: Box<dyn Reranker>,
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

        // Create Arweave client
        // If API key is set but no wallet exists, auto-generate one
        let api_key = config.resolved_api_key();
        let arweave = if let Some(ref wallet_path) = config.arweave.wallet_path {
            let resolved = crate::config::expand_tilde_pub(wallet_path);
            if resolved.exists() {
                let wallet = Wallet::from_file(&resolved)?;
                info!(address = %wallet.address, "loaded Arweave wallet");
                ArweaveClient::new(wallet, &config.arweave.turbo_endpoint, &config.arweave.gateway)
            } else if api_key.is_some() {
                // API key configured but wallet file doesn't exist — generate one
                info!(path = %resolved.display(), "generating new Arweave wallet");
                let wallet = Wallet::generate()?;
                if let Some(parent) = resolved.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                wallet.save_to_file(&resolved)?;
                info!(address = %wallet.address, path = %resolved.display(), "wallet generated and saved");
                ArweaveClient::new(wallet, &config.arweave.turbo_endpoint, &config.arweave.gateway)
            } else {
                ArweaveClient::read_only(&config.arweave.turbo_endpoint, &config.arweave.gateway)
            }
        } else if api_key.is_some() {
            // API key but no wallet_path — use default path
            let default_path = crate::config::expand_tilde_pub("~/.memoryport/wallet.json");
            if default_path.exists() {
                let wallet = Wallet::from_file(&default_path)?;
                info!(address = %wallet.address, "loaded Arweave wallet from default path");
                ArweaveClient::new(wallet, &config.arweave.turbo_endpoint, &config.arweave.gateway)
            } else {
                info!("generating new Arweave wallet at default path");
                let wallet = Wallet::generate()?;
                if let Some(parent) = default_path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                wallet.save_to_file(&default_path)?;
                info!(address = %wallet.address, "wallet generated and saved");
                ArweaveClient::new(wallet, &config.arweave.turbo_endpoint, &config.arweave.gateway)
            }
        } else {
            ArweaveClient::read_only(&config.arweave.turbo_endpoint, &config.arweave.gateway)
        };
        let arweave = Arc::new(arweave);

        // Initialize encryption if enabled
        let (master_key, keystore) = if config.encryption.enabled {
            let passphrase = std::env::var(&config.encryption.passphrase_env)
                .unwrap_or_default();
            if passphrase.is_empty() {
                tracing::warn!(
                    env = %config.encryption.passphrase_env,
                    "encryption enabled but passphrase env var is empty"
                );
                (None, None)
            } else {
                // Use a deterministic salt derived from the index path for reproducibility
                let salt_input = config.index.path.as_bytes();
                let mut salt = [0u8; 16];
                let hash = <sha2::Sha256 as sha2::Digest>::digest(salt_input);
                salt.copy_from_slice(&hash[..16]);

                let mk = crypto::derive_master_key(&passphrase, &salt)?;
                let ks_path = index_path.with_extension("keys.db");
                let ks = Arc::new(keystore::KeyStore::open(&ks_path)?);
                info!("encryption enabled");
                (Some(mk), Some(ks))
            }
        } else {
            (None, None)
        };

        // Create AccountClient if API key is configured
        let account_client = if let Some(ref key) = api_key {
            let endpoint = config.resolved_api_endpoint();
            let client = Arc::new(account::AccountClient::new(endpoint, key.clone()));

            // Register wallet address on first use (best-effort)
            if let Some(addr) = arweave.address() {
                match client.validate(addr).await {
                    Ok(v) => info!(user_id = %v.user_id, tier = %v.tier, "registered with Memoryport"),
                    Err(e) => tracing::warn!(error = %e, "failed to validate API key (will retry on upload)"),
                }
            }

            Some(client)
        } else {
            None
        };

        let writer = {
            let mut w = Writer::new_from_arc(arweave.clone())
                .with_arweave_enabled(config.arweave.enabled);
            if let (Some(ref mk), Some(ref ks)) = (&master_key, &keystore) {
                w = w.with_encryption(mk.clone(), ks.clone());
            }
            if let Some(ref ac) = account_client {
                w = w.with_account(ac.clone());
            }
            Arc::new(w)
        };

        // Create query enhancer (if LLM configured for expansion / HyDE)
        let enhancer: Option<Arc<dyn enhancer::QueryEnhancer>> = if config.retrieval.query_expansion || config.retrieval.hyde {
            let llm_provider = config.retrieval.llm_provider.as_deref().unwrap_or(&config.embeddings.provider);
            let llm_model = config.retrieval.llm_model.as_deref().unwrap_or("gpt-4o-mini");
            let llm: Arc<dyn uc_embeddings::llm::LlmProvider> = match llm_provider {
                "ollama" => Arc::new(uc_embeddings::ollama::OllamaLlm::new(
                    llm_model,
                    config.embeddings.api_base.clone(),
                )),
                _ => {
                    let api_key = config.embeddings.api_key.clone()
                        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
                        .unwrap_or_default();
                    Arc::new(uc_embeddings::openai::OpenAiLlm::new(
                        api_key,
                        llm_model,
                        config.embeddings.api_base.clone(),
                    ))
                }
            };
            info!(
                provider = llm_provider,
                model = llm_model,
                expansion = config.retrieval.query_expansion,
                hyde = config.retrieval.hyde,
                "query enhancement enabled"
            );
            Some(Arc::new(enhancer::LlmQueryEnhancer::new(
                llm,
                config.retrieval.query_expansion,
                config.retrieval.hyde,
            )))
        } else {
            None
        };

        // Warm up embedding provider (prevents 15s Ollama cold-start)
        match embeddings.embed_batch(&["warmup"]).await {
            Ok(_) => tracing::debug!("embedding provider warmed up"),
            Err(e) => tracing::warn!(error = %e, "embedding warmup failed (non-fatal)"),
        }

        // Initialize retrieval gate (Gate 2: embedding routing)
        let retrieval_gate = if config.retrieval.gating_enabled {
            match gate::RetrievalGate::init(embeddings.as_ref()).await {
                Ok(g) => {
                    info!("retrieval gate initialized");
                    Some(g)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to init retrieval gate, gating disabled");
                    None
                }
            }
        } else {
            None
        };

        // Keep embedding provider warm (prevents model eviction)
        {
            let keepalive_embeddings = embeddings.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(240)); // 4 min
                interval.tick().await; // skip first immediate tick
                loop {
                    interval.tick().await;
                    let _ = keepalive_embeddings.embed_batch(&["keepalive"]).await;
                }
            });
        }

        let mut retriever = Retriever::new(
            index.clone(),
            embeddings.clone(),
            config.retrieval.clone(),
        );
        if let Some(e) = enhancer {
            retriever = retriever.with_enhancer(e);
        }
        if let Some(g) = retrieval_gate {
            retriever = retriever.with_gate(g);
        }

        // Create reranker
        let reranker: Box<dyn Reranker> = Box::new(HeuristicReranker::default());

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

                // 3. Insert into LanceDB index with user_id from batch
                let user_id = batch.user_id.clone();
                let entries: Vec<(Chunk, Vec<f32>, String, u32)> = batch
                    .chunks
                    .iter()
                    .zip(vectors.iter())
                    .enumerate()
                    .map(|(i, (chunk, vec))| {
                        (chunk.clone(), vec.clone(), receipt.tx_id.clone(), i as u32)
                    })
                    .collect();
                index.insert(&entries, &user_id).await.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

                // 4. Extract facts from chunks and store in facts table
                let mut all_facts = Vec::new();
                for chunk in &batch.chunks {
                    let extraction = facts::extract_facts(
                        &chunk.content,
                        &chunk.id.to_string(),
                        &chunk.session_id,
                        &user_id,
                        chunk.timestamp,
                    );
                    all_facts.extend(extraction.facts);
                }

                if !all_facts.is_empty() {
                    // Embed fact content
                    let fact_texts: Vec<&str> = all_facts.iter().map(|f| f.content.as_str()).collect();
                    match embeddings.embed_batch(&fact_texts).await {
                        Ok(fact_vectors) => {
                            // Detect contradictions against existing facts
                            for fact in &all_facts {
                                let existing = index
                                    .search_facts_by_predicate(&user_id, &fact.subject, &fact.predicate, true)
                                    .await
                                    .unwrap_or_default();

                                let existing_as_facts: Vec<facts::Fact> = existing.iter().map(|r| facts::Fact {
                                    id: uuid::Uuid::parse_str(&r.fact_id).unwrap_or_default(),
                                    content: r.content.clone(),
                                    subject: r.subject.clone(),
                                    predicate: r.predicate.clone(),
                                    object: r.object.clone(),
                                    source_chunk_id: String::new(),
                                    session_id: r.session_id.clone(),
                                    user_id: user_id.clone(),
                                    document_date: r.document_date,
                                    event_date: r.event_date,
                                    valid: r.valid,
                                    superseded_by: None,
                                    confidence: r.confidence,
                                    created_at: 0,
                                }).collect();

                                let contradictions = contradiction::detect_contradictions(
                                    std::slice::from_ref(fact),
                                    &existing_as_facts,
                                );

                                for c in &contradictions {
                                    let _ = index.mark_fact_superseded(&c.old_fact_id, &c.new_fact_id).await;
                                    tracing::debug!(
                                        old = %c.old_fact_id,
                                        new = %c.new_fact_id,
                                        reason = %c.reason,
                                        "fact superseded"
                                    );
                                }
                            }

                            // Insert facts into LanceDB
                            if let Err(e) = index.insert_facts(&all_facts, &fact_vectors).await {
                                tracing::warn!(error = %e, "failed to insert facts (non-fatal)");
                            } else {
                                tracing::debug!(count = all_facts.len(), "extracted and stored facts");
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "failed to embed facts (non-fatal)");
                        }
                    }
                }

                Ok(())
            })
        });

        let batcher = Batcher::new(50, Duration::from_secs(60), "default", on_flush);

        Ok(Self {
            config,
            index,
            embeddings,
            arweave,
            master_key,
            keystore,
            retriever,
            reranker,
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
        // Set the batcher's user_id for this store operation
        self.batcher.set_user_id(&params.user_id).await;

        let timestamp = params.timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        let mut chunks = chunker::chunk_text(
            text,
            &params.session_id,
            params.chunk_type,
            params.role,
            &self.chunker_config,
            timestamp,
        );

        // Tag source integration + model on each chunk
        for chunk in &mut chunks {
            chunk.metadata.source_integration = params.source_integration.clone();
            chunk.metadata.source_model = params.source_model.clone();
        }

        let ids: Vec<Uuid> = chunks.iter().map(|c| c.id).collect();

        self.batcher.add_many(chunks).await?;

        Ok(ids)
    }

    /// Full retrieval pipeline: hybrid retrieve → rerank → assemble.
    pub async fn query(
        &self,
        text: &str,
        user_id: &str,
        active_session_id: Option<&str>,
        max_tokens: u32,
    ) -> Result<AssembledContext, EngineError> {
        // 1. Hybrid retrieve (chunks + facts with RRF fusion)
        let candidates = self
            .retriever
            .retrieve_hybrid(text, user_id, active_session_id)
            .await?;

        // 2. Rerank
        let ranked = self
            .reranker
            .rerank(text, candidates, active_session_id)
            .await?;

        // 3. Assemble context
        let context = assembler::assemble_context(&ranked, max_tokens);

        Ok(context)
    }

    /// Retrieve raw results without assembly (useful for debugging / CLI).
    /// Uses hybrid retrieval (chunks + facts with RRF fusion).
    pub async fn retrieve(
        &self,
        text: &str,
        user_id: &str,
        active_session_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, EngineError> {
        let candidates = self
            .retriever
            .retrieve_hybrid(text, user_id, active_session_id)
            .await?;

        let ranked = self
            .reranker
            .rerank(text, candidates, active_session_id)
            .await?;

        Ok(ranked)
    }

    /// Direct vector search — bypasses gating but applies session/temporal analysis.
    /// Use for explicit user search requests (search bars, CLI retrieve).
    pub async fn search(
        &self,
        text: &str,
        user_id: &str,
        top_k: usize,
    ) -> Result<Vec<SearchResult>, EngineError> {
        // Run analyzer to detect signals (session refs, temporal)
        let signals = analyzer::analyze_query(text);

        let query_vector = self.embeddings.embed(text).await?;
        let params = models::QueryParams {
            user_id: user_id.to_string(),
            top_k,
            session_id: signals.explicit_session,
            chunk_type: None,
            // Apply temporal range for production use; benchmark data may have
            // different timestamps so the filter may not match.
            time_range: signals.temporal_range,
        };
        let results = self.index.search(&query_vector, &params).await?;
        Ok(results)
    }

    /// Get all chunks for a session, ordered chronologically.
    pub async fn get_session(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<Vec<SearchResult>, EngineError> {
        let results = self.index.get_all_for_session(user_id, session_id).await?;
        Ok(results)
    }

    /// Compute analytics aggregates for a user.
    pub async fn analytics(
        &self,
        user_id: &str,
    ) -> Result<analytics::AnalyticsData, EngineError> {
        let data = analytics::compute_analytics(&self.index, user_id)
            .await
            .map_err(|_e| EngineError::Index(crate::index::IndexError::NoResults))?;
        Ok(data)
    }

    /// Compute session-level knowledge graph.
    pub async fn graph(
        &self,
        user_id: &str,
    ) -> Result<graph::GraphData, EngineError> {
        let data = graph::compute_session_graph(
            &self.index,
            self.embeddings.as_ref(),
            user_id,
            0.75, // similarity threshold — only connect closely related sessions
        )
        .await
        .map_err(|_| EngineError::Index(crate::index::IndexError::NoResults))?;
        Ok(data)
    }

    /// List all sessions for a user.
    pub async fn list_sessions(
        &self,
        user_id: &str,
    ) -> Result<Vec<models::SessionSummary>, EngineError> {
        let sessions = self.index.list_sessions(user_id).await?;
        Ok(sessions)
    }

    /// Force-flush any buffered chunks.
    pub async fn flush(&self) -> Result<(), EngineError> {
        self.batcher.flush().await?;
        Ok(())
    }

    /// Rebuild the local index from Arweave.
    pub async fn rebuild_index(&self, user_id: &str) -> Result<rebuild::RebuildProgress, EngineError> {
        let progress = rebuild::rebuild_index(
            &self.arweave,
            &self.index,
            self.embeddings.as_ref(),
            user_id,
            self.master_key.as_ref(),
            self.keystore.as_deref(),
        )
        .await?;
        Ok(progress)
    }

    /// Logical deletion: destroy the batch key for a transaction.
    /// The ciphertext on Arweave becomes permanently unreadable.
    pub async fn delete_batch(&self, tx_id: &str) -> Result<bool, EngineError> {
        let ks = self.keystore.as_ref().ok_or_else(|| {
            EngineError::Crypto(crypto::CryptoError::Encrypt(
                "encryption/keystore not enabled".into(),
            ))
        })?;
        let destroyed = ks.destroy(tx_id).await?;
        Ok(destroyed)
    }

    /// Get the Arweave wallet address, if configured.
    pub fn arweave_address(&self) -> Option<String> {
        self.arweave.address().map(|s| s.to_string())
    }

    /// Return engine status.
    /// Compact the index to merge fragmented files.
    pub async fn optimize(&self) -> Result<(), EngineError> {
        self.index.optimize().await?;
        Ok(())
    }

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
