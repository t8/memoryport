use crate::index::Index;
use crate::models::{BatchPayload, Chunk};
use thiserror::Error;
use tracing::{debug, info, warn};
use uc_arweave::{ArweaveClient, TagFilter};
use uc_embeddings::EmbeddingProvider;

#[derive(Debug, Error)]
pub enum RebuildError {
    #[error("arweave error: {0}")]
    Arweave(#[from] uc_arweave::ArweaveError),
    #[error("index error: {0}")]
    Index(#[from] crate::index::IndexError),
    #[error("embedding error: {0}")]
    Embedding(#[from] uc_embeddings::EmbeddingError),
    #[error("failed to parse transaction data: {0}")]
    Parse(String),
}

/// Progress tracker for index rebuilds.
#[derive(Debug, Clone)]
pub struct RebuildProgress {
    pub transactions_found: usize,
    pub transactions_processed: usize,
    pub chunks_indexed: usize,
    pub errors: usize,
}

/// Rebuild the local LanceDB index from Arweave.
///
/// Queries ar.io GraphQL for all UnlimitedContext transactions belonging to the user,
/// fetches transaction data, parses chunks, computes embeddings, and inserts into the index.
pub async fn rebuild_index(
    arweave: &ArweaveClient,
    index: &Index,
    embeddings: &dyn EmbeddingProvider,
    user_id: &str,
) -> Result<RebuildProgress, RebuildError> {
    let mut progress = RebuildProgress {
        transactions_found: 0,
        transactions_processed: 0,
        chunks_indexed: 0,
        errors: 0,
    };

    // 1. Query all transactions for this user
    let tag_filters = vec![
        TagFilter::single("App-Name", "UnlimitedContext"),
        TagFilter::single("UC-User-Id", user_id),
    ];

    info!(user_id, "querying Arweave for transactions...");
    let edges = arweave.query_all_transactions(&tag_filters).await?;
    progress.transactions_found = edges.len();
    info!(count = edges.len(), "found transactions on Arweave");

    // 2. Process each transaction
    for edge in &edges {
        let tx_id = &edge.node.id;

        match process_transaction(arweave, index, embeddings, tx_id, user_id).await {
            Ok(chunk_count) => {
                progress.transactions_processed += 1;
                progress.chunks_indexed += chunk_count;
                debug!(tx_id, chunks = chunk_count, "processed transaction");
            }
            Err(e) => {
                progress.errors += 1;
                warn!(tx_id, error = %e, "failed to process transaction");
            }
        }
    }

    info!(
        transactions = progress.transactions_processed,
        chunks = progress.chunks_indexed,
        errors = progress.errors,
        "index rebuild complete"
    );

    Ok(progress)
}

async fn process_transaction(
    arweave: &ArweaveClient,
    index: &Index,
    embeddings: &dyn EmbeddingProvider,
    tx_id: &str,
    user_id: &str,
) -> Result<usize, RebuildError> {
    // Fetch raw transaction data
    let data = arweave.fetch_data(tx_id).await?;

    // Parse JSON payload
    let payload: BatchPayload = serde_json::from_slice(&data)
        .map_err(|e| RebuildError::Parse(format!("tx {tx_id}: {e}")))?;

    if payload.chunks.is_empty() {
        return Ok(0);
    }

    // Convert to Chunk structs
    let chunks: Vec<Chunk> = payload
        .chunks
        .iter()
        .map(|cp| {
            let id = cp.id.parse().unwrap_or_else(|_| uuid::Uuid::new_v4());
            Chunk {
                id,
                chunk_type: cp.chunk_type,
                session_id: cp.session_id.clone(),
                timestamp: cp.timestamp,
                role: cp.role,
                content: cp.content.clone(),
                metadata: cp.metadata.clone(),
            }
        })
        .collect();

    // Compute embeddings in batch
    let texts: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
    let vectors = embeddings.embed_batch(&texts).await?;

    // Build entries for index
    let entries: Vec<(Chunk, Vec<f32>, String, u32)> = chunks
        .into_iter()
        .zip(vectors.into_iter())
        .enumerate()
        .map(|(i, (chunk, vec))| (chunk, vec, tx_id.to_string(), i as u32))
        .collect();

    let count = entries.len();
    index.insert(&entries, user_id).await?;

    Ok(count)
}
