use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::auth::AuthenticatedUser;

pub struct UserDb {
    conn: Mutex<Connection>,
}

impl UserDb {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                email TEXT,
                created_at TEXT NOT NULL,
                active INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL REFERENCES users(id),
                key_hash TEXT NOT NULL,
                key_prefix TEXT NOT NULL,
                created_at TEXT NOT NULL,
                revoked INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);
            CREATE INDEX IF NOT EXISTS idx_api_keys_user ON api_keys(user_id);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create a new user and return (user_id, plaintext_api_key).
    pub async fn create_user(&self, email: Option<&str>) -> Result<(String, String), String> {
        let user_id = Uuid::new_v4().to_string();
        let key_id = Uuid::new_v4().to_string();
        let api_key = generate_api_key();
        let key_hash = hash_api_key(&api_key);
        let key_prefix = &api_key[..11.min(api_key.len())]; // "uc_" + 8 hex chars
        let now = Utc::now().to_rfc3339();

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO users (id, email, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![user_id, email, now],
        )
        .map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO api_keys (id, user_id, key_hash, key_prefix, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![key_id, user_id, key_hash, key_prefix, now],
        )
        .map_err(|e| e.to_string())?;

        Ok((user_id, api_key))
    }

    /// Look up a user by API key hash.
    pub async fn lookup_by_key_hash(&self, key_hash: &str) -> Result<Option<AuthenticatedUser>, String> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT ak.id, ak.user_id FROM api_keys ak
                 JOIN users u ON ak.user_id = u.id
                 WHERE ak.key_hash = ?1 AND ak.revoked = 0 AND u.active = 1",
            )
            .map_err(|e| e.to_string())?;

        let result = stmt
            .query_row(rusqlite::params![key_hash], |row| {
                Ok(AuthenticatedUser {
                    key_id: row.get(0)?,
                    user_id: row.get(1)?,
                })
            })
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(result)
    }

    /// Deactivate a user.
    pub async fn delete_user(&self, user_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().await;
        let rows = conn
            .execute(
                "UPDATE users SET active = 0 WHERE id = ?1",
                rusqlite::params![user_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(rows > 0)
    }

    /// Check if the database is reachable.
    pub async fn ping(&self) -> Result<(), String> {
        let conn = self.conn.lock().await;
        conn.execute_batch("SELECT 1").map_err(|e| e.to_string())
    }
}

fn generate_api_key() -> String {
    let mut bytes = [0u8; 16];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("uc_{}", hex::encode(bytes))
}

pub fn hash_api_key(key: &str) -> String {
    let hash = Sha256::digest(key.as_bytes());
    hex::encode(hash)
}

use rusqlite::OptionalExtension;
