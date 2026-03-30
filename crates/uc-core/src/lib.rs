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
pub mod keyword_index;
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

/// The main entry point for the Memoryport engine.
pub struct Engine {
    config: Config,
    user_id: String,
    index: Arc<Index>,
    keyword_index: Option<Arc<keyword_index::KeywordIndex>>,
    embeddings: Arc<dyn EmbeddingProvider>,
    arweave: Arc<ArweaveClient>,
    writer: Arc<Writer>,
    master_key: Option<crypto::MasterKey>,
    keystore: Option<Arc<keystore::KeyStore>>,
    retriever: Retriever,
    reranker: Box<dyn Reranker>,
    batcher: Batcher,
    chunker_config: ChunkerConfig,
}

impl Engine {
    /// Get the user ID (wallet address or "local" for users without a wallet).
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

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
            // Read passphrase from config first, fall back to env var
            let passphrase = config.encryption.passphrase.clone()
                .filter(|s| !s.is_empty())
                .or_else(|| std::env::var(&config.encryption.passphrase_env).ok())
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

        // Open BM25 keyword index (best-effort — degrades gracefully if it fails)
        let keyword_index = match keyword_index::KeywordIndex::open(&index_path) {
            Ok(ki) => {
                info!("BM25 keyword index ready");
                Some(Arc::new(ki))
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to open keyword index, BM25 search disabled");
                None
            }
        };

        // Create batcher with flush callback
        let flush_writer = writer.clone();
        let flush_index = index.clone();
        let flush_embeddings = embeddings.clone();
        let flush_keyword_index = keyword_index.clone();

        let on_flush: FlushCallback = Arc::new(move |batch: Batch| {
            let writer = flush_writer.clone();
            let index = flush_index.clone();
            let embeddings = flush_embeddings.clone();
            let kw_index = flush_keyword_index.clone();
            Box::pin(async move {
                // 1. Compute embeddings with enriched text.
                // Prepend context to each chunk before embedding to improve
                // retrieval quality:
                // - Date prefix: "[March 15, 2023]" so temporal queries match
                // - Previous turn context: the preceding message in the session
                //   gives conversational context (Anthropic's Contextual Retrieval)
                let enriched_texts: Vec<String> = batch.chunks.iter().map(|c| {
                    // Date-enriched embedding: prepend the chunk's date so temporal
                    // queries ("last week", "in March") match chunks from those dates.
                    // Exp 28 showed this improves temporal reasoning from 50% to 61.5%.
                    let ts_secs = c.timestamp / 1000;
                    if ts_secs > 0 {
                        if let Some(dt) = chrono::DateTime::from_timestamp(ts_secs, 0) {
                            return format!("[{}] {}", dt.format("%B %d, %Y"), c.content);
                        } else {
                            tracing::debug!(timestamp = ts_secs, "timestamp out of range for date prefix");
                        }
                    }
                    c.content.clone()
                }).collect();
                let text_refs: Vec<&str> = enriched_texts.iter().map(|s| s.as_str()).collect();
                let vectors = embeddings.embed_batch(&text_refs).await.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

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

                // 3b. Index in BM25 keyword index (best-effort)
                if let Some(ref ki) = kw_index {
                    for chunk in &batch.chunks {
                        if let Err(e) = ki.index_chunk(
                            &chunk.id.to_string(),
                            &chunk.session_id,
                            &user_id,
                            &chunk.content,
                        ).await {
                            tracing::warn!(error = %e, "BM25 index failed for chunk (non-fatal)");
                        }
                    }
                    if let Err(e) = ki.commit().await {
                        tracing::warn!(error = %e, "BM25 commit failed, retrying...");
                        let _ = ki.commit().await;
                    }
                }

                // 4. Extract facts in background (non-blocking)
                let bg_index = index.clone();
                let bg_embeddings = embeddings.clone();
                let bg_user_id = user_id.clone();
                let bg_chunks: Vec<_> = batch.chunks.iter().map(|c| (c.id.to_string(), c.content.clone(), c.session_id.clone(), c.timestamp)).collect();
                tokio::spawn(async move {
                    let mut all_facts = Vec::new();
                    for (chunk_id, content, session_id, timestamp) in &bg_chunks {
                        let extraction = facts::extract_facts(content, chunk_id, session_id, &bg_user_id, *timestamp);
                        all_facts.extend(extraction.facts);
                    }

                    if all_facts.is_empty() { return; }

                    let fact_texts: Vec<&str> = all_facts.iter().map(|f| f.content.as_str()).collect();
                    let fact_vectors = match bg_embeddings.embed_batch(&fact_texts).await {
                        Ok(v) => v,
                        Err(e) => { tracing::warn!(error = %e, "failed to embed facts (non-fatal)"); return; }
                    };

                    for fact in &all_facts {
                        let existing = bg_index
                            .search_facts_by_predicate(&bg_user_id, &fact.subject, &fact.predicate, true)
                            .await
                            .unwrap_or_default();

                        let existing_as_facts: Vec<facts::Fact> = existing.iter().map(|r| facts::Fact {
                            id: match uuid::Uuid::parse_str(&r.fact_id) {
                                Ok(id) => id,
                                Err(e) => { tracing::warn!(fact_id = %r.fact_id, error = %e, "invalid fact UUID"); uuid::Uuid::new_v4() }
                            },
                            content: r.content.clone(),
                            subject: r.subject.clone(),
                            predicate: r.predicate.clone(),
                            object: r.object.clone(),
                            source_chunk_id: String::new(),
                            session_id: r.session_id.clone(),
                            user_id: bg_user_id.clone(),
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
                            let _ = bg_index.mark_fact_superseded(&c.old_fact_id, &c.new_fact_id).await;
                            tracing::debug!(old = %c.old_fact_id, new = %c.new_fact_id, reason = %c.reason, "fact superseded");
                        }
                    }

                    if let Err(e) = bg_index.insert_facts(&all_facts, &fact_vectors).await {
                        tracing::warn!(error = %e, "failed to insert facts (non-fatal)");
                    } else {
                        tracing::debug!(count = all_facts.len(), "extracted and stored facts");
                    }
                });

                Ok(())
            })
        });

        // Derive user_id from wallet address (unique per user) or "local" if no wallet
        let user_id = arweave.address()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "local".to_string());
        info!(user_id = %user_id, "engine user_id set");

        // Migrate data from legacy "default" user_id to wallet address
        if user_id != "local" && user_id != "default" {
            match index.migrate_user_id("default", &user_id).await {
                Ok(0) => {} // nothing to migrate
                Ok(n) => info!(count = n, "migrated chunks from 'default' to wallet user_id"),
                Err(e) => tracing::warn!(error = %e, "user_id migration failed (non-fatal)"),
            }
        }

        let batcher = Batcher::new(50, Duration::from_secs(60), &user_id, on_flush);

        Ok(Self {
            config,
            user_id,
            index,
            keyword_index,
            embeddings,
            arweave,
            writer,
            master_key,
            keystore,
            retriever,
            reranker,
            batcher,
            chunker_config: ChunkerConfig::default(),
        })
    }

    /// Store text content. Chunks it and buffers in the batcher.
    ///
    /// For conversation turns: user turns are buffered until the next assistant
    /// turn arrives for the same session. The user+assistant pair is then stored
    /// as a single "round" chunk, keeping the Q&A context together in the embedding.
    /// This improves retrieval quality (LongMemEval paper's #1 recommendation).
    pub async fn store(
        &self,
        text: &str,
        params: StoreParams,
    ) -> Result<Vec<Uuid>, EngineError> {
        self.batcher.set_user_id(&params.user_id).await;

        let timestamp = params.timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        // Round-level buffering for conversations: buffer user turns,
        // combine with the next assistant turn.
        let store_text: String;
        let store_role: Option<Role>;

        store_text = text.to_string();
        store_role = params.role;

        let mut chunks = chunker::chunk_text(
            &store_text,
            &params.session_id,
            params.chunk_type,
            store_role,
            &self.chunker_config,
            timestamp,
        );

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
        reference_time: Option<i64>,
    ) -> Result<AssembledContext, EngineError> {
        // 1. Hybrid retrieve (chunks + facts with RRF fusion)
        let candidates = self
            .retriever
            .retrieve_hybrid(text, user_id, active_session_id, reference_time)
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
    /// Explicit retrieval — bypasses gating (for search bars, CLI, MCP tools).
    pub async fn retrieve(
        &self,
        text: &str,
        user_id: &str,
        active_session_id: Option<&str>,
        reference_time: Option<i64>,
    ) -> Result<Vec<SearchResult>, EngineError> {
        let results = self.search(text, user_id, 50, reference_time).await?;

        let ranked = self
            .reranker
            .rerank(text, results, active_session_id)
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
        reference_time: Option<i64>,
    ) -> Result<Vec<SearchResult>, EngineError> {
        // Run analyzer to detect signals (session refs, temporal)
        let signals = match reference_time {
            Some(t) => analyzer::analyze_query_at(text, t),
            None => analyzer::analyze_query(text),
        };

        let query_vector = self.embeddings.embed(text).await?;

        // ── Parallel: vector search + BM25 keyword search ──
        let params = models::QueryParams {
            user_id: user_id.to_string(),
            top_k,
            session_id: signals.explicit_session.clone(),
            chunk_type: None,
            time_range: signals.temporal_range,
        };
        let mut results = self.index.search(&query_vector, &params).await?;

        let mut seen: std::collections::HashSet<String> =
            results.iter().map(|r| r.chunk_id.clone()).collect();

        // Temporal fallback: if temporal filter yielded few results, retry without it.
        if signals.temporal_range.is_some() && results.len() < top_k / 2 {
            let fallback_params = models::QueryParams {
                user_id: user_id.to_string(),
                top_k,
                session_id: signals.explicit_session.clone(),
                chunk_type: None,
                time_range: None,
            };
            let fallback = self.index.search(&query_vector, &fallback_params).await?;
            for r in fallback {
                if seen.insert(r.chunk_id.clone()) {
                    results.push(r);
                }
            }
        }

        // Sort by score descending, truncate to top_k
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
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

    /// Look up a single chunk by its ID.
    pub async fn get_chunk_by_id(&self, chunk_id: &str) -> Result<Option<SearchResult>, EngineError> {
        let result = self.index.get_chunk_by_id(chunk_id).await?;
        Ok(result)
    }

    /// Resolve memoryport://chunk/ URIs in text to inline content.
    pub async fn resolve_refs(&self, text: &str) -> String {
        let prefix = "memoryport://chunk/";
        if !text.contains(prefix) {
            return text.to_string();
        }

        let mut result = text.to_string();
        let mut start = 0;
        while let Some(pos) = result[start..].find(prefix) {
            let abs_pos = start + pos;
            let id_start = abs_pos + prefix.len();
            let id_end = result[id_start..].find(|c: char| !c.is_alphanumeric() && c != '-')
                .map(|i| id_start + i)
                .unwrap_or(result.len());
            let chunk_id = result[id_start..id_end].to_string();

            if let Ok(Some(chunk)) = self.index.get_chunk_by_id(&chunk_id).await {
                let role = chunk.role.map(|r| r.as_str().to_string()).unwrap_or("unknown".into());
                let replacement = format!(
                    "[Referenced memory from session {} — {}]: {}",
                    chunk.session_id, role, chunk.content
                );
                result.replace_range(abs_pos..id_end, &replacement);
                start = abs_pos + replacement.len();
            } else {
                start = id_end;
            }
        }
        result
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
        .map_err(|e| {
            tracing::error!(error = %e, "graph computation failed");
            EngineError::Index(crate::index::IndexError::NoResults)
        })?;
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

    /// Sync all local chunks to Arweave that haven't been uploaded yet.
    /// Returns the number of chunks synced.
    pub async fn sync_to_arweave(&self) -> Result<usize, EngineError> {
        use crate::models::{Batch, Chunk, ChunkMetadata};
        use uuid::Uuid;

        let all_chunks = self.index.get_all_chunks(&self.user_id).await?;
        let unsynced: Vec<_> = all_chunks.into_iter()
            .filter(|c| c.arweave_tx_id.is_empty())
            .collect();

        if unsynced.is_empty() {
            return Ok(0);
        }

        let total = unsynced.len();
        info!(count = total, "syncing unsynced chunks to Arweave");

        for batch_chunks in unsynced.chunks(50) {
            let chunks: Vec<Chunk> = batch_chunks.iter().map(|r| {
                Chunk {
                    id: Uuid::parse_str(&r.chunk_id).unwrap_or_else(|_| Uuid::new_v4()),
                    content: r.content.clone(),
                    chunk_type: r.chunk_type.clone(),
                    role: r.role.clone(),
                    session_id: r.session_id.clone(),
                    timestamp: r.timestamp,
                    metadata: ChunkMetadata::default(),
                }
            }).collect();

            let batch = Batch::new(chunks, &self.user_id);

            match self.writer.write_batch(&batch).await {
                Ok(_receipt) => {
                    info!(count = batch.chunks.len(), "synced batch to Arweave");
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to sync batch to Arweave");
                }
            }
        }

        Ok(total)
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




