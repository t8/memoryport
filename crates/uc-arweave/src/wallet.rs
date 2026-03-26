use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as B64Engine};
use rsa::pss::BlindedSigningKey;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use rsa::traits::{PrivateKeyParts, PublicKeyParts};
use rsa::{BigUint, RsaPrivateKey};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("failed to read wallet file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse wallet JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid RSA key component '{field}': {reason}")]
    InvalidKey { field: &'static str, reason: String },
    #[error("RSA error: {0}")]
    Rsa(#[from] rsa::Error),
}

/// An Arweave JWK wallet file format.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct JwkWallet {
    kty: String,
    n: String,
    e: String,
    d: String,
    p: String,
    q: String,
    dp: String,
    dq: String,
    qi: String,
}

/// A loaded Arweave wallet with signing capability.
#[derive(Clone)]
pub struct Wallet {
    private_key: RsaPrivateKey,
    /// The raw bytes of the public key modulus (n).
    pub owner_bytes: Vec<u8>,
    /// The Arweave address: base64url(SHA-256(owner_bytes)).
    pub address: String,
}

impl std::fmt::Debug for Wallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wallet")
            .field("address", &self.address)
            .finish()
    }
}

impl Wallet {
    /// Generate a new RSA-4096 Arweave wallet.
    pub fn generate() -> Result<Self, WalletError> {
        let mut rng = rand::thread_rng();
        let private_key = RsaPrivateKey::new(&mut rng, 4096)?;

        let owner_bytes = private_key.n().to_bytes_be();
        let address = {
            let hash = Sha256::digest(&owner_bytes);
            URL_SAFE_NO_PAD.encode(hash)
        };

        Ok(Self {
            private_key,
            owner_bytes,
            address,
        })
    }

    /// Load a wallet from a JWK JSON file.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, WalletError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Load a wallet from a JWK JSON string.
    pub fn from_json(json: &str) -> Result<Self, WalletError> {
        let jwk: JwkWallet = serde_json::from_str(json)?;

        let n = decode_b64url_bigint(&jwk.n, "n")?;
        let e = decode_b64url_bigint(&jwk.e, "e")?;
        let d = decode_b64url_bigint(&jwk.d, "d")?;
        let p = decode_b64url_bigint(&jwk.p, "p")?;
        let q = decode_b64url_bigint(&jwk.q, "q")?;

        let private_key = RsaPrivateKey::from_components(n, e, d, vec![p, q])?;

        let owner_bytes = decode_b64url_bytes(&jwk.n, "n")?;
        let address = {
            let hash = Sha256::digest(&owner_bytes);
            URL_SAFE_NO_PAD.encode(hash)
        };

        Ok(Self {
            private_key,
            owner_bytes,
            address,
        })
    }

    /// Serialize this wallet to JWK JSON format.
    pub fn to_json(&self) -> Result<String, WalletError> {
        let key = &self.private_key;
        let primes = key.primes();
        let p = &primes[0];
        let q = &primes[1];

        let one = BigUint::from(1u32);
        let dp = key.d() % (p - &one);
        let dq = key.d() % (q - &one);

        // qi = q^(-1) mod p — modular inverse
        let qi = mod_inverse(q, p).ok_or_else(|| WalletError::InvalidKey {
            field: "qi",
            reason: "failed to compute modular inverse".into(),
        })?;

        let jwk = JwkWallet {
            kty: "RSA".into(),
            n: encode_b64url_bytes(&key.n().to_bytes_be()),
            e: encode_b64url_bytes(&key.e().to_bytes_be()),
            d: encode_b64url_bytes(&key.d().to_bytes_be()),
            p: encode_b64url_bytes(&p.to_bytes_be()),
            q: encode_b64url_bytes(&q.to_bytes_be()),
            dp: encode_b64url_bytes(&dp.to_bytes_be()),
            dq: encode_b64url_bytes(&dq.to_bytes_be()),
            qi: encode_b64url_bytes(&qi.to_bytes_be()),
        };

        Ok(serde_json::to_string_pretty(&jwk)?)
    }

    /// Save this wallet to a JWK JSON file.
    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), WalletError> {
        let json = self.to_json()?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Sign a message using RSA-PSS with SHA-256.
    /// Returns the raw signature bytes (512 bytes for 4096-bit key).
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, WalletError> {
        let signing_key = BlindedSigningKey::<Sha256>::new(self.private_key.clone());
        let mut rng = rand::thread_rng();
        let signature = signing_key.sign_with_rng(&mut rng, message);
        Ok(signature.to_vec())
    }

    /// Get the public key modulus bytes (the "owner" field in data items).
    pub fn owner_bytes(&self) -> &[u8] {
        &self.owner_bytes
    }
}

fn encode_b64url_bytes(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn decode_b64url_bytes(s: &str, field: &'static str) -> Result<Vec<u8>, WalletError> {
    URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| WalletError::InvalidKey {
            field,
            reason: e.to_string(),
        })
}

fn decode_b64url_bigint(s: &str, field: &'static str) -> Result<BigUint, WalletError> {
    let bytes = decode_b64url_bytes(s, field)?;
    Ok(BigUint::from_bytes_be(&bytes))
}

/// Compute modular inverse: a^(-1) mod m using Fermat's little theorem.
/// For prime m: a^(-1) = a^(m-2) mod m.
fn mod_inverse(a: &BigUint, m: &BigUint) -> Option<BigUint> {
    let one = BigUint::from(1u32);
    let two = BigUint::from(2u32);
    if m <= &one {
        return None;
    }
    let exp = m - &two;
    Some(a.modpow(&exp, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_wallet() {
        let wallet = Wallet::generate().expect("wallet generation failed");
        assert!(!wallet.address.is_empty());
        assert!(!wallet.owner_bytes.is_empty());

        // Verify signing works
        let msg = b"hello arweave";
        let sig = wallet.sign(msg).expect("signing failed");
        assert_eq!(sig.len(), 512); // RSA-4096 signature
    }

    #[test]
    fn test_wallet_json_roundtrip() {
        let wallet = Wallet::generate().expect("generation failed");
        let json = wallet.to_json().expect("to_json failed");
        let loaded = Wallet::from_json(&json).expect("from_json failed");

        assert_eq!(wallet.address, loaded.address);
        assert_eq!(wallet.owner_bytes, loaded.owner_bytes);

        // Verify loaded wallet can sign
        let msg = b"roundtrip test";
        let sig = loaded.sign(msg).expect("signing failed");
        assert_eq!(sig.len(), 512);
    }

    #[test]
    fn test_wallet_file_roundtrip() {
        let wallet = Wallet::generate().expect("generation failed");
        let dir = std::env::temp_dir().join("memoryport_test_wallet");
        let path = dir.join("test_wallet.json");
        std::fs::create_dir_all(&dir).ok();

        wallet.save_to_file(&path).expect("save failed");
        let loaded = Wallet::from_file(&path).expect("load failed");

        assert_eq!(wallet.address, loaded.address);
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }
}
