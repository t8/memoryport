use crate::models::SearchResult;
use async_trait::async_trait;
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Error)]
pub enum RerankerError {
    #[error("reranking failed: {0}")]
    Failed(String),
    #[error("API error: {0}")]
    Api(String),
}

/// Trait for reranking search results.
#[async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(
        &self,
        query: &str,
        candidates: Vec<SearchResult>,
        active_session_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, RerankerError>;
}

/// Heuristic-based reranker using recency decay, session affinity, and diversity.
pub struct HeuristicReranker {
    /// Half-life for recency decay in milliseconds.
    pub recency_half_life_ms: i64,
    /// Boost factor for chunks from the active session.
    pub session_affinity_boost: f32,
    /// Whether to apply MMR diversity penalty.
    pub diversity_enabled: bool,
    /// Lambda for MMR diversity (0 = max diversity, 1 = no diversity).
    pub diversity_lambda: f32,
}

impl Default for HeuristicReranker {
    fn default() -> Self {
        Self {
            recency_half_life_ms: 24 * 3600 * 1000, // 1 day
            session_affinity_boost: 1.2,
            diversity_enabled: true,
            diversity_lambda: 0.7,
        }
    }
}

#[async_trait]
impl Reranker for HeuristicReranker {
    async fn rerank(
        &self,
        _query: &str,
        mut candidates: Vec<SearchResult>,
        active_session_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, RerankerError> {
        if candidates.is_empty() {
            return Ok(candidates);
        }

        let now_ms = chrono::Utc::now().timestamp_millis();

        // Score each candidate
        for result in &mut candidates {
            let mut score = result.score;

            // Recency boost: exponential decay
            let age_ms = (now_ms - result.timestamp).max(0) as f64;
            let half_life = self.recency_half_life_ms as f64;
            let recency_factor = (0.5_f64).powf(age_ms / half_life) as f32;
            score *= 0.7 + 0.3 * recency_factor; // blend: 70% base + 30% recency

            // Session affinity boost
            if let Some(active_sid) = active_session_id {
                if result.session_id == active_sid {
                    score *= self.session_affinity_boost;
                }
            }

            result.score = score;
        }

        // Sort by adjusted score
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Apply MMR diversity if enabled
        if self.diversity_enabled && candidates.len() > 1 {
            candidates = mmr_reorder(candidates, self.diversity_lambda);
        }

        debug!(count = candidates.len(), "heuristic reranking complete");

        Ok(candidates)
    }
}

/// Maximal Marginal Relevance reordering for diversity.
/// Greedily selects candidates that balance relevance and diversity.
fn mmr_reorder(candidates: Vec<SearchResult>, lambda: f32) -> Vec<SearchResult> {
    let n = candidates.len();
    let mut selected: Vec<SearchResult> = Vec::with_capacity(n);
    let mut remaining: Vec<SearchResult> = candidates;

    // Always pick the top result first
    if let Some(first) = remaining.first().cloned() {
        selected.push(first);
        remaining.remove(0);
    }

    while !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_mmr = f32::NEG_INFINITY;

        for (i, candidate) in remaining.iter().enumerate() {
            // Relevance component
            let relevance = candidate.score;

            // Diversity component: max similarity to any already-selected result
            let max_sim = selected
                .iter()
                .map(|s| content_similarity(&candidate.content, &s.content))
                .fold(0.0_f32, f32::max);

            let mmr = lambda * relevance - (1.0 - lambda) * max_sim;

            if mmr > best_mmr {
                best_mmr = mmr;
                best_idx = i;
            }
        }

        selected.push(remaining.remove(best_idx));
    }

    selected
}

/// Simple content similarity based on character-level Jaccard of words.
fn content_similarity(a: &str, b: &str) -> f32 {
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();

    if words_a.is_empty() && words_b.is_empty() {
        return 1.0;
    }

    let intersection = words_a.intersection(&words_b).count() as f32;
    let union = words_a.union(&words_b).count() as f32;

    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// Stub reranker that calls an external API (e.g., Cohere Rerank).
/// Currently a passthrough — implement when API integration is needed.
pub struct ApiReranker {
    #[allow(dead_code)]
    api_base: String,
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    model: String,
}

impl ApiReranker {
    pub fn new(api_base: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_base: api_base.into(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }
}

#[async_trait]
impl Reranker for ApiReranker {
    async fn rerank(
        &self,
        _query: &str,
        candidates: Vec<SearchResult>,
        _active_session_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, RerankerError> {
        // TODO: implement API call to Cohere Rerank or similar
        // For now, passthrough
        debug!(count = candidates.len(), "API reranker passthrough (not yet implemented)");
        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ChunkType;

    fn make_result(chunk_id: &str, score: f32, timestamp: i64, session: &str, content: &str) -> SearchResult {
        SearchResult {
            chunk_id: chunk_id.into(),
            session_id: session.into(),
            chunk_type: ChunkType::Conversation,
            role: None,
            timestamp,
            content: content.into(),
            score,
            arweave_tx_id: "tx".into(),
        }
    }

    #[tokio::test]
    async fn test_heuristic_reranker_session_boost() {
        let reranker = HeuristicReranker::default();
        let now = chrono::Utc::now().timestamp_millis();

        let candidates = vec![
            make_result("a", 0.8, now, "other", "hello world"),
            make_result("b", 0.75, now, "active", "hello there"),
        ];

        let results = reranker.rerank("hello", candidates, Some("active")).await.unwrap();
        // "b" should get a session boost and potentially outrank "a"
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_heuristic_reranker_recency() {
        let reranker = HeuristicReranker {
            recency_half_life_ms: 3600_000, // 1 hour
            ..Default::default()
        };
        let now = chrono::Utc::now().timestamp_millis();

        let candidates = vec![
            make_result("old", 0.9, now - 7 * 86400_000, "s1", "old content"),
            make_result("new", 0.85, now - 60_000, "s1", "new content"),
        ];

        let results = reranker.rerank("test", candidates, None).await.unwrap();
        // New result should rank higher due to recency boost
        assert_eq!(results[0].chunk_id, "new");
    }

    #[test]
    fn test_content_similarity() {
        let sim = content_similarity("hello world foo", "hello world bar");
        assert!(sim > 0.3 && sim < 0.8);

        let sim_same = content_similarity("hello world", "hello world");
        assert!((sim_same - 1.0).abs() < 0.001);

        let sim_diff = content_similarity("hello", "goodbye");
        assert!(sim_diff < 0.1);
    }
}
