use crate::analyzer;
use crate::config::RetrievalConfig;
use crate::enhancer::QueryEnhancer;
use crate::gate::RetrievalGate;
use crate::index::Index;
use crate::models::{QueryParams, RetrievalDecision, SearchResult};
use std::collections::HashSet;
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
            },
        ];
        let deduped = dedup_by_chunk_id(results);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].chunk_id, "a");
        assert_eq!(deduped[0].score, 0.9);
    }
}
