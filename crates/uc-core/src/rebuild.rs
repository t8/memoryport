use crate::crypto::{self, EncryptedBatchKey, EncryptedPayload, MasterKey};
use crate::index::Index;
use crate::keystore::KeyStore;
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
    #[error("decryption error: {0}")]
    Crypto(#[from] crypto::CryptoError),
    #[error("encrypted batch but no master key available")]
    NoMasterKey,
    #[error("encrypted batch but no batch key found for tx {0}")]
    NoBatchKey(String),
}

/// Progress tracker for index rebuilds.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RebuildProgress {
    pub transactions_found: usize,
    pub transactions_processed: usize,
    pub chunks_indexed: usize,
    pub errors: usize,
}

/// Rebuild the local LanceDB index from Arweave.
pub async fn rebuild_index(
    arweave: &ArweaveClient,
    index: &Index,
    embeddings: &dyn EmbeddingProvider,
    user_id: &str,
    master_key: Option<&MasterKey>,
    keystore: Option<&KeyStore>,
) -> Result<RebuildProgress, RebuildError> {
    let mut progress = RebuildProgress {
        transactions_found: 0,
        transactions_processed: 0,
        chunks_indexed: 0,
        errors: 0,
    };

    let tag_filters = vec![
        TagFilter::single("App-Name", crate::tagger::APP_NAME),
        TagFilter::single("UC-User-Id", user_id),
    ];

    info!(user_id, "querying Arweave for transactions...");
    let mut edges = arweave.query_all_transactions(&tag_filters).await?;
    progress.transactions_found = edges.len();
    info!(count = edges.len(), "found transactions on Arweave");

    // Sort by timestamp for chronological reconstruction
    edges.sort_by(|a, b| {
        let ts_a = a.node.tags.iter().find(|t| t.name == "UC-Timestamp-Start").map(|t| &t.value);
        let ts_b = b.node.tags.iter().find(|t| t.name == "UC-Timestamp-Start").map(|t| &t.value);
        ts_a.cmp(&ts_b)
    });

    for edge in &edges {
        let tx_id = &edge.node.id;

        // Check if this transaction is encrypted (from tags)
        let is_encrypted = edge.node.tags.iter().any(|t| t.name == "UC-Encrypted" && t.value == "true");
        let batch_key_b64 = edge
            .node
            .tags
            .iter()
            .find(|t| t.name == "UC-Batch-Key")
            .map(|t| t.value.clone());

        match process_transaction(
            arweave, index, embeddings, tx_id, user_id,
            is_encrypted, batch_key_b64.as_deref(), master_key, keystore,
        )
        .await
        {
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
    is_encrypted: bool,
    batch_key_b64: Option<&str>,
    master_key: Option<&MasterKey>,
    keystore: Option<&KeyStore>,
) -> Result<usize, RebuildError> {
    // Fetch raw transaction data
    let data = arweave.fetch_data(tx_id).await?;

    // Decrypt if needed
    let json_bytes = if is_encrypted {
        let mk = master_key.ok_or(RebuildError::NoMasterKey)?;

        // Get encrypted batch key from tag or keystore
        let encrypted_batch_key = if let Some(b64) = batch_key_b64 {
            EncryptedBatchKey::from_base64(b64)?
        } else if let Some(ks) = keystore {
            ks.get(tx_id)
                .await
                .map_err(|e| RebuildError::Parse(e.to_string()))?
                .ok_or_else(|| RebuildError::NoBatchKey(tx_id.to_string()))?
        } else {
            return Err(RebuildError::NoBatchKey(tx_id.to_string()));
        };

        let batch_key = crypto::decrypt_batch_key(&encrypted_batch_key, mk)?;
        let encrypted_payload = EncryptedPayload::from_bytes(&data)?;
        crypto::decrypt_payload(&encrypted_payload, &batch_key)?
    } else {
        data
    };

    // Parse JSON payload
    let payload: BatchPayload = serde_json::from_slice(&json_bytes)
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

    let entries: Vec<(Chunk, Vec<f32>, String, u32)> = chunks
        .into_iter()
        .zip(vectors.into_iter())
        .enumerate()
        .map(|(i, (chunk, vec))| (chunk, vec, tx_id.to_string(), i as u32))
        .collect();

    let count = entries.len();
    index.insert(&entries, user_id).await?;

    // Store batch key in local keystore for future use
    if is_encrypted {
        if let (Some(b64), Some(ks)) = (batch_key_b64, keystore) {
            let ebk = EncryptedBatchKey::from_base64(b64)
                .map_err(|e| RebuildError::Parse(e.to_string()))?;
            ks.store(tx_id, &ebk, user_id)
                .await
                .map_err(|e| RebuildError::Parse(e.to_string()))?;
        }
    }

    Ok(count)
}
