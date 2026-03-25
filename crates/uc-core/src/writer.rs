use crate::crypto::{self, EncryptedBatchKey, MasterKey};
use crate::keystore::KeyStore;
use crate::models::{Batch, BatchPayload, UploadReceipt};
use crate::tagger;
use chrono::Utc;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info};
use uc_arweave::{ArweaveClient, ArweaveError, Tag};

#[derive(Debug, Error)]
pub enum WriterError {
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("arweave upload failed: {0}")]
    Upload(#[from] ArweaveError),
    #[error("tag validation failed: {0}")]
    Tags(#[from] tagger::TagError),
    #[error("encryption failed: {0}")]
    Crypto(#[from] crypto::CryptoError),
    #[error("keystore error: {0}")]
    KeyStore(#[from] crate::keystore::KeyStoreError),
}

/// Handles serializing batches and submitting them to Arweave.
pub struct Writer {
    arweave: Arc<ArweaveClient>,
    master_key: Option<MasterKey>,
    keystore: Option<Arc<KeyStore>>,
}

impl Writer {
    pub fn new_from_arc(arweave: Arc<ArweaveClient>) -> Self {
        Self {
            arweave,
            master_key: None,
            keystore: None,
        }
    }

    pub fn with_encryption(mut self, master_key: MasterKey, keystore: Arc<KeyStore>) -> Self {
        self.master_key = Some(master_key);
        self.keystore = Some(keystore);
        self
    }

    /// Serialize a batch to JSON, optionally encrypt, and upload to Arweave.
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

        // 2. Generate tags
        let mut tags = tagger::generate_batch_tags(batch, &batch.user_id);
        tagger::validate_tag_budget(&tags)?;

        // 3. Optionally encrypt
        let upload_bytes = if let Some(ref master_key) = self.master_key {
            let batch_key = crypto::generate_batch_key();
            let encrypted = crypto::encrypt_payload(&json_bytes, &batch_key)?;
            let encrypted_batch_key = crypto::encrypt_batch_key(&batch_key, master_key)?;

            // Add encryption tags
            tags.push(Tag::new("UC-Encrypted", "true"));
            tags.push(Tag::new("UC-Batch-Key", encrypted_batch_key.to_base64()));

            debug!(
                batch_id = %batch.id,
                "batch encrypted with AES-256-GCM"
            );

            // We'll store the key after upload (need tx_id)
            encrypted.to_bytes()
        } else {
            json_bytes
        };

        // 4. Upload to Arweave
        let response = self.arweave.upload(&upload_bytes, &tags).await?;

        info!(
            batch_id = %batch.id,
            tx_id = %response.id,
            encrypted = self.master_key.is_some(),
            "batch uploaded to Arweave"
        );

        // 5. Store encrypted batch key in keystore (if encrypted)
        if self.master_key.is_some() {
            if let Some(ref keystore) = self.keystore {
                // Find the encrypted key from tags
                if let Some(key_tag) = tags.iter().find(|t| t.name == "UC-Batch-Key") {
                    let ebk = EncryptedBatchKey::from_base64(&key_tag.value)?;
                    keystore.store(&response.id, &ebk, &batch.user_id).await?;
                }
            }
        }

        Ok(UploadReceipt {
            tx_id: response.id,
            timestamp: Utc::now(),
        })
    }
}
