use crate::crypto::EncryptedBatchKey;
use chrono::Utc;
use rusqlite::OptionalExtension;
use std::path::Path;
use tokio::sync::Mutex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KeyStoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Local store for encrypted batch keys, keyed by Arweave transaction ID.
pub struct KeyStore {
    conn: Mutex<rusqlite::Connection>,
}

/// Info about a stored batch key.
#[derive(Debug, Clone)]
pub struct BatchKeyInfo {
    pub tx_id: String,
    pub user_id: String,
    pub created_at: String,
    pub destroyed: bool,
}

impl KeyStore {
    pub fn open(path: &Path) -> Result<Self, KeyStoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS batch_keys (
                tx_id TEXT PRIMARY KEY,
                encrypted_batch_key BLOB NOT NULL,
                user_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                destroyed INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_batch_keys_user ON batch_keys(user_id);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Store an encrypted batch key for a transaction.
    pub async fn store(
        &self,
        tx_id: &str,
        encrypted_key: &EncryptedBatchKey,
        user_id: &str,
    ) -> Result<(), KeyStoreError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO batch_keys (tx_id, encrypted_batch_key, user_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![tx_id, encrypted_key.0, user_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Retrieve the encrypted batch key for a transaction.
    pub async fn get(&self, tx_id: &str) -> Result<Option<EncryptedBatchKey>, KeyStoreError> {
        let conn = self.conn.lock().await;
        let result: Option<Vec<u8>> = conn
            .query_row(
                "SELECT encrypted_batch_key FROM batch_keys WHERE tx_id = ?1 AND destroyed = 0",
                rusqlite::params![tx_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result.map(EncryptedBatchKey))
    }

    /// Mark a batch key as destroyed (logical deletion).
    /// The ciphertext on Arweave becomes permanently unreadable.
    pub async fn destroy(&self, tx_id: &str) -> Result<bool, KeyStoreError> {
        let conn = self.conn.lock().await;
        let rows = conn.execute(
            "UPDATE batch_keys SET destroyed = 1, encrypted_batch_key = X'' WHERE tx_id = ?1",
            rusqlite::params![tx_id],
        )?;
        Ok(rows > 0)
    }

    /// List all batch key records for a user.
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<BatchKeyInfo>, KeyStoreError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT tx_id, user_id, created_at, destroyed FROM batch_keys WHERE user_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![user_id], |row| {
            Ok(BatchKeyInfo {
                tx_id: row.get(0)?,
                user_id: row.get(1)?,
                created_at: row.get(2)?,
                destroyed: row.get::<_, i32>(3)? != 0,
            })
        })?;
        let mut infos = Vec::new();
        for row in rows {
            infos.push(row?);
        }
        Ok(infos)
    }

    /// Check if the store is reachable.
    pub async fn ping(&self) -> Result<(), KeyStoreError> {
        let conn = self.conn.lock().await;
        conn.execute_batch("SELECT 1")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn temp_keystore() -> KeyStore {
        let tmp = NamedTempFile::new().unwrap();
        KeyStore::open(tmp.path()).unwrap()
    }

    #[tokio::test]
    async fn test_store_and_get() {
        let ks = temp_keystore();
        let key = EncryptedBatchKey(vec![1, 2, 3, 4]);
        ks.store("tx_abc", &key, "user_1").await.unwrap();

        let retrieved = ks.get("tx_abc").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().0, vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_destroy() {
        let ks = temp_keystore();
        let key = EncryptedBatchKey(vec![5, 6, 7, 8]);
        ks.store("tx_def", &key, "user_1").await.unwrap();

        let destroyed = ks.destroy("tx_def").await.unwrap();
        assert!(destroyed);

        // Should not be retrievable after destruction
        let retrieved = ks.get("tx_def").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_list_for_user() {
        let ks = temp_keystore();
        ks.store("tx_1", &EncryptedBatchKey(vec![1]), "user_a").await.unwrap();
        ks.store("tx_2", &EncryptedBatchKey(vec![2]), "user_a").await.unwrap();
        ks.store("tx_3", &EncryptedBatchKey(vec![3]), "user_b").await.unwrap();

        let user_a = ks.list_for_user("user_a").await.unwrap();
        assert_eq!(user_a.len(), 2);

        let user_b = ks.list_for_user("user_b").await.unwrap();
        assert_eq!(user_b.len(), 1);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let ks = temp_keystore();
        let result = ks.get("no_such_tx").await.unwrap();
        assert!(result.is_none());
    }
}
