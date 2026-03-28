use crate::analyzer;
use crate::config::RetrievalConfig;
use crate::enhancer::QueryEnhancer;
use crate::gate::RetrievalGate;
use crate::index::Index;
use crate::models::{QueryParams, RetrievalDecision, SearchResult};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tracing::debug;
use uc_embeddings::EmbeddingProvider;

#[derive(Debug, Error)]
pub enum RetrieverError {
    #[error("index error: {0}")]
    Index(#[from] crate::index::IndexError),
    #[error("embedding error: {0}")]
    Embedding(#[from] uc_embeddings::EmbeddingError),
    #[error("enhancer error: {0}")]
    Enhancer(#[from] crate::enhancer::EnhancerError),
}

/// Multi-strategy retriever with three-gate gating system.
pub struct Retriever {
    index: Arc<Index>,
    embeddings: Arc<dyn EmbeddingProvider>,
    enhancer: Option<Arc<dyn QueryEnhancer>>,
    gate: Option<RetrievalGate>,
    config: RetrievalConfig,
}

impl Retriever {
    pub fn new(
        index: Arc<Index>,
        embeddings: Arc<dyn EmbeddingProvider>,
        config: RetrievalConfig,
    ) -> Self {
        Self {
            index,
            embeddings,
            enhancer: None,
            gate: None,
            config,
        }
    }

    pub fn with_enhancer(mut self, enhancer: Arc<dyn QueryEnhancer>) -> Self {
        self.enhancer = Some(enhancer);
        self
    }

    pub fn with_gate(mut self, gate: RetrievalGate) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Run the full retrieval pipeline with three-gate gating.
    pub async fn retrieve(
        &self,
        query: &str,
        user_id: &str,
        active_session_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, RetrieverError> {
        // ── Gate 1: Rule-based ──
        let signals = analyzer::analyze_query(query);

        if self.config.gating_enabled {
            match signals.decision {
                RetrievalDecision::Skip => {
                    debug!(query = %query, gate = "1-rules", "retrieval skipped");
                    return Ok(Vec::new());
                }
                RetrievalDecision::Force => {
                    debug!(query = %query, gate = "1-rules", "retrieval forced");
                }
                RetrievalDecision::Undecided => {
                    debug!(query = %query, gate = "1-rules", "undecided, proceeding to gate 2");
                }
            }
        }

        // Enhance query (expansion + HyDE) — only if not skipped
        let enhanced = if let Some(ref enhancer) = self.enhancer {
            enhancer.enhance(query).await?
        } else {
            crate::enhancer::EnhancedQuery {
                original: query.to_string(),
                expanded_queries: Vec::new(),
                hyde_document: None,
            }
        };

        // Embed the primary query
        let primary_text = enhanced.hyde_document.as_deref().unwrap_or(query);
        let primary_vector = self.embeddings.embed(primary_text).await?;

        // ── Gate 2: Embedding routing ──
        // Skip Gate 2 for small indexes — no performance reason to gate, and
        // the exemplar centroids may not match the user's actual content patterns.
        let index_size = self.index.count(Some(user_id)).await.unwrap_or(0);
        if self.config.gating_enabled && signals.decision == RetrievalDecision::Undecided && index_size >= 100 {
            if let Some(ref gate) = self.gate {
                if !gate.should_retrieve(&primary_vector) {
                    debug!(query = %query, gate = "2-embedding", "retrieval skipped by embedding routing");
                    return Ok(Vec::new());
                }
                debug!(query = %query, gate = "2-embedding", "retrieval approved by embedding routing");
            }
        }

        // ── Search ──
        let mut candidates = Vec::new();

        // Primary vector search
        let vector_params = QueryParams {
            user_id: user_id.to_string(),
            top_k: self.config.similarity_top_k,
            session_id: signals.explicit_session.clone(),
            chunk_type: None,
            time_range: signals.temporal_range,
        };
        let primary_results = self.index.search(&primary_vector, &vector_params).await?;
        debug!(count = primary_results.len(), "primary vector search results");
        candidates.extend(primary_results);

        // Expanded query searches
        for expanded in &enhanced.expanded_queries {
            let exp_vector = self.embeddings.embed(expanded).await?;
            let exp_params = QueryParams {
                user_id: user_id.to_string(),
                top_k: self.config.similarity_top_k / 3,
                session_id: None,
                chunk_type: None,
                time_range: signals.temporal_range,
            };
            let exp_results = self.index.search(&exp_vector, &exp_params).await?;
            debug!(query = %expanded, count = exp_results.len(), "expansion search results");
            candidates.extend(exp_results);
        }

        // Recency window
        if let Some(session_id) = active_session_id {
            let recency_limit = if signals.is_recency_heavy {
                self.config.recency_window * 2
            } else {
                self.config.recency_window
            };
            let recency_results = self.index.get_recent(user_id, session_id, recency_limit).await?;
            debug!(count = recency_results.len(), "recency window results");
            candidates.extend(recency_results);
        }

        // Explicit session lookup
        if let Some(ref explicit_sid) = signals.explicit_session {
            if active_session_id.map_or(true, |s| s != explicit_sid) {
                let session_params = QueryParams {
                    user_id: user_id.to_string(),
                    top_k: 20,
                    session_id: Some(explicit_sid.clone()),
                    chunk_type: None,
                    time_range: None,
                };
                let session_results = self.index.search(&primary_vector, &session_params).await?;
                debug!(count = session_results.len(), session = %explicit_sid, "explicit session results");
                candidates.extend(session_results);
            }
        }

        // Deduplicate
        let deduped = dedup_by_chunk_id(candidates);

        // ── Gate 3: Post-retrieval quality check ──
        if self.config.gating_enabled && self.config.min_relevance_score > 0.0 {
            let best_score = deduped.first().map(|r| r.score).unwrap_or(0.0);
            if best_score < self.config.min_relevance_score {
                debug!(
                    best_score = format!("{:.4}", best_score),
                    threshold = format!("{:.4}", self.config.min_relevance_score),
                    gate = "3-quality",
                    "results below quality threshold, dropping all"
                );
                return Ok(Vec::new());
            }
        }

        debug!(total = deduped.len(), "final results after gating");
        Ok(deduped)
    }

    /// Hybrid retrieval: chunk vector search + fact vector search, merged with RRF.
    ///
    /// Runs chunk and fact searches in parallel, fuses the ranked result lists
    /// using Reciprocal Rank Fusion, then returns a unified `SearchResult` list
    /// sorted by the fused score.
    pub async fn retrieve_hybrid(
        &self,
        query: &str,
        user_id: &str,
        active_session_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, RetrieverError> {
        // ── Gate 1: Rule-based ──
        let signals = analyzer::analyze_query(query);

        if self.config.gating_enabled {
            match signals.decision {
                RetrievalDecision::Skip => {
                    debug!(query = %query, gate = "1-rules", "hybrid retrieval skipped");
                    return Ok(Vec::new());
                }
                RetrievalDecision::Force => {
                    debug!(query = %query, gate = "1-rules", "hybrid retrieval forced");
                }
                RetrievalDecision::Undecided => {
                    debug!(query = %query, gate = "1-rules", "undecided, proceeding to gate 2");
                }
            }
        }

        // Enhance query (expansion + HyDE)
        let enhanced = if let Some(ref enhancer) = self.enhancer {
            enhancer.enhance(query).await?
        } else {
            crate::enhancer::EnhancedQuery {
                original: query.to_string(),
                expanded_queries: Vec::new(),
                hyde_document: None,
            }
        };

        // Embed the primary query
        let primary_text = enhanced.hyde_document.as_deref().unwrap_or(query);
        let primary_vector = self.embeddings.embed(primary_text).await?;

        // ── Gate 2: Embedding routing ──
        let index_size = self.index.count(Some(user_id)).await.unwrap_or(0);
        if self.config.gating_enabled
            && signals.decision == RetrievalDecision::Undecided
            && index_size >= 100
        {
            if let Some(ref gate) = self.gate {
                if !gate.should_retrieve(&primary_vector) {
                    debug!(query = %query, gate = "2-embedding", "hybrid retrieval skipped by embedding routing");
                    return Ok(Vec::new());
                }
                debug!(query = %query, gate = "2-embedding", "hybrid retrieval approved by embedding routing");
            }
        }

        // ── Parallel search: chunks + facts ──
        let chunk_params = QueryParams {
            user_id: user_id.to_string(),
            top_k: self.config.similarity_top_k,
            session_id: signals.explicit_session.clone(),
            chunk_type: None,
            time_range: signals.temporal_range,
        };

        let chunk_future = self.index.search(&primary_vector, &chunk_params);
        let fact_future = self.index.search_facts(
            &primary_vector,
            user_id,
            self.config.similarity_top_k,
            true, // valid_only
        );

        let (chunk_results, fact_results) = tokio::join!(chunk_future, fact_future);
        let chunk_results = chunk_results?;
        let fact_results = fact_results?;

        debug!(
            chunks = chunk_results.len(),
            facts = fact_results.len(),
            "hybrid search raw results"
        );

        // Also run expanded queries and recency (same as retrieve)
        let mut extra_chunks = Vec::new();
        for expanded in &enhanced.expanded_queries {
            let exp_vector = self.embeddings.embed(expanded).await?;
            let exp_params = QueryParams {
                user_id: user_id.to_string(),
                top_k: self.config.similarity_top_k / 3,
                session_id: None,
                chunk_type: None,
                time_range: signals.temporal_range,
            };
            let exp_results = self.index.search(&exp_vector, &exp_params).await?;
            debug!(query = %expanded, count = exp_results.len(), "expansion search results");
            extra_chunks.extend(exp_results);
        }

        if let Some(session_id) = active_session_id {
            let recency_limit = if signals.is_recency_heavy {
                self.config.recency_window * 2
            } else {
                self.config.recency_window
            };
            let recency_results = self
                .index
                .get_recent(user_id, session_id, recency_limit)
                .await?;
            debug!(count = recency_results.len(), "recency window results");
            extra_chunks.extend(recency_results);
        }

        if let Some(ref explicit_sid) = signals.explicit_session {
            if active_session_id.map_or(true, |s| s != explicit_sid) {
                let session_params = QueryParams {
                    user_id: user_id.to_string(),
                    top_k: 20,
                    session_id: Some(explicit_sid.clone()),
                    chunk_type: None,
                    time_range: None,
                };
                let session_results = self
                    .index
                    .search(&primary_vector, &session_params)
                    .await?;
                debug!(count = session_results.len(), session = %explicit_sid, "explicit session results");
                extra_chunks.extend(session_results);
            }
        }

        // ── Build RRF input sets ──
        // Set 1: primary chunk results (already sorted by score from LanceDB)
        let chunk_set: Vec<(String, f32)> = chunk_results
            .iter()
            .chain(extra_chunks.iter())
            .map(|r| (r.chunk_id.clone(), r.score))
            .collect();

        // Set 2: fact results (prefixed to avoid ID collisions with chunks)
        let fact_set: Vec<(String, f32)> = fact_results
            .iter()
            .map(|f| (format!("fact_{}", f.fact_id), f.score))
            .collect();

        let fused = reciprocal_rank_fusion(&[chunk_set, fact_set], 60.0);

        // ── Build lookup maps for content ──
        let mut chunk_map: HashMap<String, SearchResult> = HashMap::new();
        for r in chunk_results.into_iter().chain(extra_chunks.into_iter()) {
            chunk_map.entry(r.chunk_id.clone()).or_insert(r);
        }

        let mut fact_map: HashMap<String, crate::index::FactSearchResult> = HashMap::new();
        for f in fact_results {
            fact_map
                .entry(format!("fact_{}", f.fact_id))
                .or_insert(f);
        }

        // ── Assemble final results with RRF scores ──
        let mut results = Vec::with_capacity(fused.len());
        for (id, rrf_score) in &fused {
            if let Some(mut sr) = chunk_map.remove(id) {
                sr.score = *rrf_score;
                results.push(sr);
            } else if let Some(fact) = fact_map.remove(id) {
                // Fact hit without a matching chunk — include as synthetic SearchResult
                results.push(SearchResult {
                    chunk_id: id.clone(),
                    session_id: fact.session_id.clone(),
                    chunk_type: crate::models::ChunkType::Knowledge,
                    role: None,
                    timestamp: fact.document_date,
                    content: fact.content.clone(),
                    score: *rrf_score,
                    arweave_tx_id: String::new(),
                    source_integration: None,
                    source_model: None,
                });
            }
        }

        // ── Gate 3: Post-retrieval quality check ──
        if self.config.gating_enabled && self.config.min_relevance_score > 0.0 {
            let best_score = results.first().map(|r| r.score).unwrap_or(0.0);
            if best_score < self.config.min_relevance_score {
                debug!(
                    best_score = format!("{:.4}", best_score),
                    threshold = format!("{:.4}", self.config.min_relevance_score),
                    gate = "3-quality",
                    "hybrid results below quality threshold, dropping all"
                );
                return Ok(Vec::new());
            }
        }

        debug!(total = results.len(), "final hybrid results after gating");
        Ok(results)
    }
}

/// Reciprocal Rank Fusion: merge multiple ranked result lists.
/// Each result is identified by a key (chunk_id or fact_id).
/// Returns merged scores sorted descending.
fn reciprocal_rank_fusion(
    result_sets: &[Vec<(String, f32)>], // Each set: Vec<(id, original_score)>
    k: f32,                              // RRF constant, typically 60.0
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    for results in result_sets {
        for (rank, (id, _original_score)) in results.iter().enumerate() {
            *scores.entry(id.clone()).or_default() += 1.0 / (k + rank as f32 + 1.0);
        }
    }
    let mut fused: Vec<(String, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

fn dedup_by_chunk_id(mut results: Vec<SearchResult>) -> Vec<SearchResult> {
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen = HashSet::new();
    results
        .into_iter()
        .filter(|r| seen.insert(r.chunk_id.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ChunkType;

    #[test]
    fn test_dedup_by_chunk_id() {
        let results = vec![
            SearchResult {
                chunk_id: "a".into(),
                session_id: "s1".into(),
                chunk_type: ChunkType::Conversation,
                role: None,
                timestamp: 100,
                content: "hello".into(),
                score: 0.9,
                arweave_tx_id: "tx1".into(),
                source_integration: None,
                source_model: None,
            },
            SearchResult {
                chunk_id: "a".into(),
                session_id: "s1".into(),
                chunk_type: ChunkType::Conversation,
                role: None,
                timestamp: 100,
                content: "hello".into(),
                score: 0.5,
                arweave_tx_id: "tx1".into(),
                source_integration: None,
                source_model: None,
            },
            SearchResult {
                chunk_id: "b".into(),
                session_id: "s1".into(),
                chunk_type: ChunkType::Conversation,
                role: None,
                timestamp: 200,
                content: "world".into(),
                score: 0.8,
                arweave_tx_id: "tx1".into(),
                source_integration: None,
                source_model: None,
            },
        ];
        let deduped = dedup_by_chunk_id(results);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].chunk_id, "a");
        assert_eq!(deduped[0].score, 0.9);
    }

    #[test]
    fn test_reciprocal_rank_fusion() {
        let k = 60.0;

        // Set 1: chunk results — A at rank 0, B at rank 1
        let chunks = vec![
            ("A".to_string(), 0.95),
            ("B".to_string(), 0.80),
        ];

        // Set 2: fact results — B at rank 0, C at rank 1
        let facts = vec![
            ("B".to_string(), 0.90),
            ("C".to_string(), 0.70),
        ];

        let fused = reciprocal_rank_fusion(&[chunks, facts], k);

        // B appears in both sets (rank 0 in facts, rank 1 in chunks) so it
        // should have the highest fused score.
        assert_eq!(fused[0].0, "B");

        // All three IDs must be present
        let ids: HashSet<&str> = fused.iter().map(|x| x.0.as_str()).collect();
        assert!(ids.contains("A"));
        assert!(ids.contains("B"));
        assert!(ids.contains("C"));
        assert_eq!(fused.len(), 3);

        // Verify exact RRF scores
        let score_b = 1.0 / (k + 1.0 + 1.0) + 1.0 / (k + 0.0 + 1.0); // rank 1 in set1 + rank 0 in set2
        let score_a = 1.0 / (k + 0.0 + 1.0); // rank 0 in set1 only
        let score_c = 1.0 / (k + 1.0 + 1.0); // rank 1 in set2 only

        let eps = 1e-6;
        assert!((fused[0].1 - score_b).abs() < eps, "B score mismatch");
        assert!((fused[1].1 - score_a).abs() < eps, "A score mismatch");
        assert!((fused[2].1 - score_c).abs() < eps, "C score mismatch");
    }

    #[test]
    fn test_rrf_empty_sets() {
        let fused = reciprocal_rank_fusion(&[], 60.0);
        assert!(fused.is_empty());

        // One empty, one non-empty
        let set = vec![("X".to_string(), 0.5)];
        let fused = reciprocal_rank_fusion(&[set, vec![]], 60.0);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].0, "X");
    }

    #[test]
    fn test_rrf_single_set() {
        let set = vec![
            ("A".to_string(), 0.9),
            ("B".to_string(), 0.8),
            ("C".to_string(), 0.7),
        ];
        let fused = reciprocal_rank_fusion(&[set], 60.0);

        // With a single set, RRF ordering should match the input ordering
        assert_eq!(fused.len(), 3);
        assert_eq!(fused[0].0, "A");
        assert_eq!(fused[1].0, "B");
        assert_eq!(fused[2].0, "C");
    }
}
