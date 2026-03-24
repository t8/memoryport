use crate::models::{Batch, BatchPayload, UploadReceipt};
use crate::tagger;
use chrono::Utc;
use thiserror::Error;
use tracing::{debug, info};
use uc_arweave::{ArweaveClient, ArweaveError};

#[derive(Debug, Error)]
pub enum WriterError {
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("arweave upload failed: {0}")]
    Upload(#[from] ArweaveError),
    #[error("tag validation failed: {0}")]
    Tags(#[from] tagger::TagError),
}

/// Handles serializing batches and submitting them to Arweave.
pub struct Writer {
    arweave: ArweaveClient,
    user_id: String,
}

impl Writer {
    pub fn new(arweave: ArweaveClient, user_id: String) -> Self {
        Self { arweave, user_id }
    }

    /// Serialize a batch to JSON and upload to Arweave via Turbo.
    pub async fn write_batch(&self, batch: &Batch) -> Result<UploadReceipt, WriterError> {
        // 1. Serialize batch to JSON
        let payload = BatchPayload::from(batch);
        let json_bytes = serde_json::to_vec(&payload)?;

        debug!(
            batch_id = %batch.id,
            chunks = batch.chunks.len(),
            bytes = json_bytes.len(),
            "serialized batch for upload"
        );

        // 2. Generate and validate tags
        let tags = tagger::generate_batch_tags(batch, &self.user_id);
        tagger::validate_tag_budget(&tags)?;

        // 3. Upload to Arweave
        let response = self.arweave.upload(&json_bytes, &tags).await?;

        info!(
            batch_id = %batch.id,
            tx_id = %response.id,
            "batch uploaded to Arweave"
        );

        Ok(UploadReceipt {
            tx_id: response.id,
            timestamp: Utc::now(),
        })
    }
}
