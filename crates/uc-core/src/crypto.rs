use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as B64, Engine as B64Engine};
use rand::RngCore;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption failed: {0}")]
    Encrypt(String),
    #[error("decryption failed: {0}")]
    Decrypt(String),
    #[error("key derivation failed: {0}")]
    KeyDerivation(String),
    #[error("invalid key material")]
    InvalidKey,
}

/// A 256-bit master key derived from a passphrase.
#[derive(Clone)]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(***)")
    }
}

/// A 256-bit per-batch encryption key.
pub struct BatchKey([u8; 32]);

impl BatchKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// An encrypted batch key (batch key wrapped with master key).
#[derive(Debug, Clone)]
pub struct EncryptedBatchKey(pub Vec<u8>);

impl EncryptedBatchKey {
    pub fn to_base64(&self) -> String {
        B64.encode(&self.0)
    }

    pub fn from_base64(s: &str) -> Result<Self, CryptoError> {
        let bytes = B64.decode(s).map_err(|_| CryptoError::InvalidKey)?;
        Ok(Self(bytes))
    }
}

/// Encrypted payload: nonce + ciphertext.
pub struct EncryptedPayload {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

impl EncryptedPayload {
    /// Serialize to bytes: 12-byte nonce || ciphertext
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.ciphertext.len());
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    /// Deserialize from bytes: first 12 bytes are nonce, rest is ciphertext.
    pub fn from_bytes(data: &[u8]) -> Result<Self, CryptoError> {
        if data.len() < 13 {
            return Err(CryptoError::Decrypt("data too short".into()));
        }
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&data[..12]);
        Ok(Self {
            nonce,
            ciphertext: data[12..].to_vec(),
        })
    }
}

/// Derive a master key from a passphrase using Argon2id.
pub fn derive_master_key(passphrase: &str, salt: &[u8]) -> Result<MasterKey, CryptoError> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| CryptoError::KeyDerivation(e.to_string()))?;
    Ok(MasterKey(key))
}

/// Generate a random salt for key derivation.
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Generate a random batch key.
pub fn generate_batch_key() -> BatchKey {
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    BatchKey(key)
}

/// Encrypt a plaintext payload with a batch key using AES-256-GCM.
pub fn encrypt_payload(plaintext: &[u8], batch_key: &BatchKey) -> Result<EncryptedPayload, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(batch_key.as_bytes())
        .map_err(|e| CryptoError::Encrypt(e.to_string()))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| CryptoError::Encrypt(e.to_string()))?;

    Ok(EncryptedPayload {
        nonce: nonce_bytes,
        ciphertext,
    })
}

/// Decrypt an encrypted payload with a batch key.
pub fn decrypt_payload(encrypted: &EncryptedPayload, batch_key: &BatchKey) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(batch_key.as_bytes())
        .map_err(|e| CryptoError::Decrypt(e.to_string()))?;

    let nonce = Nonce::from_slice(&encrypted.nonce);

    cipher
        .decrypt(nonce, encrypted.ciphertext.as_slice())
        .map_err(|e| CryptoError::Decrypt(e.to_string()))
}

/// Encrypt a batch key with the master key (key wrapping).
pub fn encrypt_batch_key(batch_key: &BatchKey, master_key: &MasterKey) -> Result<EncryptedBatchKey, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(master_key.as_bytes())
        .map_err(|e| CryptoError::Encrypt(e.to_string()))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, batch_key.as_bytes().as_slice())
        .map_err(|e| CryptoError::Encrypt(e.to_string()))?;

    // Store as nonce || ciphertext
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);

    Ok(EncryptedBatchKey(out))
}

/// Decrypt a batch key with the master key.
pub fn decrypt_batch_key(encrypted: &EncryptedBatchKey, master_key: &MasterKey) -> Result<BatchKey, CryptoError> {
    if encrypted.0.len() < 13 {
        return Err(CryptoError::InvalidKey);
    }

    let nonce = Nonce::from_slice(&encrypted.0[..12]);
    let ciphertext = &encrypted.0[12..];

    let cipher = Aes256Gcm::new_from_slice(master_key.as_bytes())
        .map_err(|e| CryptoError::Decrypt(e.to_string()))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| CryptoError::Decrypt(e.to_string()))?;

    if plaintext.len() != 32 {
        return Err(CryptoError::InvalidKey);
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&plaintext);
    Ok(BatchKey(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_encrypt_decrypt_roundtrip() {
        let batch_key = generate_batch_key();
        let plaintext = b"hello world, this is a secret message!";

        let encrypted = encrypt_payload(plaintext, &batch_key).unwrap();
        let decrypted = decrypt_payload(&encrypted, &batch_key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_payload_serialization_roundtrip() {
        let batch_key = generate_batch_key();
        let plaintext = b"test data for serialization";

        let encrypted = encrypt_payload(plaintext, &batch_key).unwrap();
        let bytes = encrypted.to_bytes();
        let restored = EncryptedPayload::from_bytes(&bytes).unwrap();
        let decrypted = decrypt_payload(&restored, &batch_key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_batch_key_wrap_unwrap() {
        let master_key = derive_master_key("test-passphrase", b"test-salt-16byt").unwrap();
        let batch_key = generate_batch_key();

        let encrypted = encrypt_batch_key(&batch_key, &master_key).unwrap();
        let decrypted = decrypt_batch_key(&encrypted, &master_key).unwrap();

        assert_eq!(batch_key.as_bytes(), decrypted.as_bytes());
    }

    #[test]
    fn test_encrypted_batch_key_base64_roundtrip() {
        let master_key = derive_master_key("passphrase", b"salt-must-be-16!").unwrap();
        let batch_key = generate_batch_key();

        let encrypted = encrypt_batch_key(&batch_key, &master_key).unwrap();
        let b64 = encrypted.to_base64();
        let restored = EncryptedBatchKey::from_base64(&b64).unwrap();

        let decrypted = decrypt_batch_key(&restored, &master_key).unwrap();
        assert_eq!(batch_key.as_bytes(), decrypted.as_bytes());
    }

    #[test]
    fn test_derive_master_key_deterministic() {
        let k1 = derive_master_key("same-pass", b"same-salt-16byte").unwrap();
        let k2 = derive_master_key("same-pass", b"same-salt-16byte").unwrap();
        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn test_wrong_key_fails() {
        let batch_key = generate_batch_key();
        let wrong_key = generate_batch_key();
        let plaintext = b"secret";

        let encrypted = encrypt_payload(plaintext, &batch_key).unwrap();
        assert!(decrypt_payload(&encrypted, &wrong_key).is_err());
    }
}
