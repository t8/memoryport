use crate::index::Index;
use crate::models::SessionSummary;
use serde::Serialize;
use thiserror::Error;
use uc_embeddings::EmbeddingProvider;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("index error: {0}")]
    Index(#[from] crate::index::IndexError),
    #[error("embedding error: {0}")]
    Embedding(#[from] uc_embeddings::EmbeddingError),
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub chunk_count: usize,
    pub first_timestamp: i64,
    pub last_timestamp: i64,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub weight: f32,
}

/// Compute a session-level graph for visualization.
/// Nodes = sessions, edges = cosine similarity between session mean embeddings.
pub async fn compute_session_graph(
    index: &Index,
    embeddings: &dyn EmbeddingProvider,
    user_id: &str,
    similarity_threshold: f32,
) -> Result<GraphData, GraphError> {
    // Get all sessions
    let sessions = index.list_sessions(user_id).await?;

    if sessions.is_empty() {
        return Ok(GraphData {
            nodes: Vec::new(),
            edges: Vec::new(),
        });
    }

    // Compute mean embedding per session
    let mut session_embeddings: Vec<(SessionSummary, Vec<f32>)> = Vec::new();

    for session in &sessions {
        let chunks = index
            .get_all_for_session(user_id, &session.session_id)
            .await?;

        if chunks.is_empty() {
            continue;
        }

        // Get text from all chunks, compute mean embedding
        let texts: Vec<&str> = chunks
            .iter()
            .take(20) // limit to avoid huge batch embeds
            .map(|c| c.content.as_str())
            .collect();

        // Concatenate texts and embed as a single document (cheaper than batch + average)
        let combined = texts.join(" ");
        let truncated = if combined.len() > 2000 {
            &combined[..2000]
        } else {
            &combined
        };

        match embeddings.embed(truncated).await {
            Ok(emb) => session_embeddings.push((session.clone(), emb)),
            Err(_) => continue,
        }
    }

    // Build nodes with simple 2D layout (PCA-like: use first two components)
    let nodes: Vec<GraphNode> = session_embeddings
        .iter()
        .enumerate()
        .map(|(i, (session, emb))| {
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

    Ok(GraphData { nodes, edges })
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
