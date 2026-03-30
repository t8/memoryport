use crate::index::Index;
use crate::models::SessionSummary;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use uc_embeddings::EmbeddingProvider;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("index error: {0}")]
    Index(#[from] crate::index::IndexError),
    #[error("embedding error: {0}")]
    Embedding(#[from] uc_embeddings::EmbeddingError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub chunk_count: usize,
    pub first_timestamp: i64,
    pub last_timestamp: i64,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub weight: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphCache {
    fingerprint: String,
    data: GraphData,
}

fn cache_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".memoryport")
        .join("graph-cache.json")
}

fn load_cache(fingerprint: &str) -> Option<GraphData> {
    let content = std::fs::read_to_string(cache_path()).ok()?;
    let cache: GraphCache = serde_json::from_str(&content).ok()?;
    if cache.fingerprint == fingerprint {
        Some(cache.data)
    } else {
        None
    }
}

fn save_cache(fingerprint: &str, data: &GraphData) {
    let cache = GraphCache {
        fingerprint: fingerprint.to_string(),
        data: data.clone(),
    };
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = std::fs::write(cache_path(), json);
    }
}

/// Compute a session-level graph for visualization.
/// Caches results on disk — only recomputes when sessions/chunks change.
pub async fn compute_session_graph(
    index: &Index,
    embeddings: &dyn EmbeddingProvider,
    user_id: &str,
    similarity_threshold: f32,
) -> Result<GraphData, GraphError> {
    // Get all sessions, sorted by most recent first, capped for performance
    let mut sessions = index.list_sessions(user_id).await?;

    if sessions.is_empty() {
        return Ok(GraphData {
            nodes: Vec::new(),
            edges: Vec::new(),
        });
    }

    // Sort by most recent and limit to 100 sessions to keep graph computation fast
    sessions.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
    let session_count = sessions.len();
    sessions.truncate(100);
    if session_count > 100 {
        tracing::info!(total = session_count, showing = 100, "capped graph to 100 most recent sessions");
    }

    // Check cache — fingerprint is session count + total chunks
    let total_chunks: usize = sessions.iter().map(|s| s.chunk_count).sum();
    let fingerprint = format!("{}:{}:{}", user_id, sessions.len(), total_chunks);

    if let Some(cached) = load_cache(&fingerprint) {
        tracing::debug!(sessions = sessions.len(), chunks = total_chunks, "returning cached graph");
        return Ok(cached);
    }

    tracing::info!(sessions = sessions.len(), chunks = total_chunks, "computing graph (cache miss)");

    // Compute mean embedding per session
    let mut session_embeddings: Vec<(SessionSummary, Vec<f32>)> = Vec::new();

    for (i, session) in sessions.iter().enumerate() {
        if i % 20 == 0 {
            tracing::debug!(progress = i, total = sessions.len(), "computing graph embeddings");
        }

        let chunks = match index.get_all_for_session(user_id, &session.session_id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(session = %session.session_id, error = %e, "skipping session");
                continue;
            }
        };

        if chunks.is_empty() {
            continue;
        }

        // Concatenate first 20 chunks and embed as a single document
        let combined: String = chunks.iter().take(20).map(|c| c.content.as_str()).collect::<Vec<_>>().join(" ");
        let truncated = if combined.len() > 2000 { &combined[..2000] } else { &combined };

        match embeddings.embed(truncated).await {
            Ok(emb) => session_embeddings.push((session.clone(), emb)),
            Err(e) => {
                tracing::debug!(session = %session.session_id, error = %e, "embedding failed, skipping");
                continue;
            }
        }
    }

    // Build nodes with simple 2D layout (PCA-like: use first two components)
    let nodes: Vec<GraphNode> = session_embeddings
        .iter()
        .enumerate()
        .map(|(_i, (session, emb))| {
            // Simple 2D projection: take two embedding dimensions and scale
            let x = if emb.len() > 0 { emb[0] * 500.0 } else { 0.0 };
            let y = if emb.len() > 1 { emb[1] * 500.0 } else { 0.0 };

            GraphNode {
                id: session.session_id.clone(),
                label: session.session_id.clone(),
                chunk_count: session.chunk_count,
                first_timestamp: session.first_timestamp,
                last_timestamp: session.last_timestamp,
                x,
                y,
            }
        })
        .collect();

    // Build edges: pairwise cosine similarity above threshold
    let mut edges = Vec::new();
    for i in 0..session_embeddings.len() {
        for j in (i + 1)..session_embeddings.len() {
            let sim = cosine_similarity(&session_embeddings[i].1, &session_embeddings[j].1);
            if sim > similarity_threshold {
                edges.push(GraphEdge {
                    source: session_embeddings[i].0.session_id.clone(),
                    target: session_embeddings[j].0.session_id.clone(),
                    weight: sim,
                });
            }
        }
    }

    let result = GraphData { nodes, edges };
    save_cache(&fingerprint, &result);
    Ok(result)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
