use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as B64Engine};
use rsa::pss::BlindedSigningKey;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
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
#[derive(Debug, serde::Deserialize)]
struct JwkWallet {
    n: String,
    e: String,
    d: String,
    p: String,
    q: String,
    #[allow(dead_code)]
    dp: String,
    #[allow(dead_code)]
    dq: String,
    #[allow(dead_code)]
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
