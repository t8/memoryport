use crate::types::{SignedDataItem, Tag};
use crate::wallet::Wallet;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as B64Engine};
use sha2::{Digest, Sha256, Sha384};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("signing failed: {0}")]
    Signing(#[from] crate::wallet::WalletError),
    #[error("tag budget exceeded: {total} bytes (max {max})")]
    TagBudgetExceeded { total: usize, max: usize },
}

/// ANS-104 signature type for Arweave RSA-4096.
const SIG_TYPE_ARWEAVE: u16 = 1;
/// Signature length for Arweave RSA-4096.
const SIG_LENGTH: usize = 512;
/// Public key (owner) length for Arweave RSA-4096.
const OWNER_LENGTH: usize = 512;

/// Build and sign an ANS-104 data item.
pub fn create_data_item(
    wallet: &Wallet,
    data: &[u8],
    tags: &[Tag],
    target: Option<&[u8; 32]>,
    anchor: Option<&[u8; 32]>,
) -> Result<SignedDataItem, TransactionError> {
    // Compute the deep hash message to sign
    let message = compute_deep_hash(
        &wallet.owner_bytes,
        target.map(|t| t.as_slice()),
        anchor.map(|a| a.as_slice()),
        tags,
        data,
    );

    // Sign the message
    let signature = wallet.sign(&message)?;

    // Compute data item ID = base64url(SHA-256(signature))
    let id = {
        let hash = Sha256::digest(&signature);
        URL_SAFE_NO_PAD.encode(hash)
    };

    // Serialize the binary data item
    let bytes = serialize_data_item(&signature, &wallet.owner_bytes, target, anchor, tags, data);

    Ok(SignedDataItem {
        id,
        bytes,
        owner_address: wallet.address.clone(),
    })
}

/// Serialize an ANS-104 data item to its binary format.
fn serialize_data_item(
    signature: &[u8],
    owner: &[u8],
    target: Option<&[u8; 32]>,
    anchor: Option<&[u8; 32]>,
    tags: &[Tag],
    data: &[u8],
) -> Vec<u8> {
    let mut buf = Vec::new();

    // Signature type (2 bytes, little-endian)
    buf.extend_from_slice(&SIG_TYPE_ARWEAVE.to_le_bytes());

    // Signature (512 bytes for Arweave)
    assert_eq!(signature.len(), SIG_LENGTH);
    buf.extend_from_slice(signature);

    // Owner / public key (512 bytes for Arweave)
    // Pad if necessary
    if owner.len() < OWNER_LENGTH {
        let padding = OWNER_LENGTH - owner.len();
        buf.extend(std::iter::repeat(0u8).take(padding));
    }
    buf.extend_from_slice(&owner[..std::cmp::min(owner.len(), OWNER_LENGTH)]);

    // Target
    match target {
        Some(t) => {
            buf.push(1);
            buf.extend_from_slice(t);
        }
        None => buf.push(0),
    }

    // Anchor
    match anchor {
        Some(a) => {
            buf.push(1);
            buf.extend_from_slice(a);
        }
        None => buf.push(0),
    }

    // Tags
    let encoded_tags = encode_avro_tags(tags);
    let num_tags = tags.len() as u64;
    buf.extend_from_slice(&num_tags.to_le_bytes());
    buf.extend_from_slice(&(encoded_tags.len() as u64).to_le_bytes());
    buf.extend_from_slice(&encoded_tags);

    // Data
    buf.extend_from_slice(data);

    buf
}

/// Compute the deep hash for signing an ANS-104 data item.
///
/// Structure: ["dataitem", "1", owner, target, anchor, tags_array, data]
fn compute_deep_hash(
    owner: &[u8],
    target: Option<&[u8]>,
    anchor: Option<&[u8]>,
    tags: &[Tag],
    data: &[u8],
) -> Vec<u8> {
    let target_bytes = target.unwrap_or(&[]);
    let anchor_bytes = anchor.unwrap_or(&[]);

    // The deep hash uses the raw avro-encoded tag bytes, not structured tags.
    // This matches the arbundles reference implementation which passes rawTags.
    let encoded_tags = encode_avro_tags(tags);

    let root = DeepHashItem::List(vec![
        DeepHashItem::Blob(b"dataitem".to_vec()),
        DeepHashItem::Blob(b"1".to_vec()),
        DeepHashItem::Blob(b"1".to_vec()), // signature type as string
        DeepHashItem::Blob(owner.to_vec()),
        DeepHashItem::Blob(target_bytes.to_vec()),
        DeepHashItem::Blob(anchor_bytes.to_vec()),
        DeepHashItem::Blob(encoded_tags),
        DeepHashItem::Blob(data.to_vec()),
    ]);

    deep_hash(&root).to_vec()
}

enum DeepHashItem {
    Blob(Vec<u8>),
    List(Vec<DeepHashItem>),
}

/// Recursive deep hash using SHA-384.
fn deep_hash(item: &DeepHashItem) -> [u8; 48] {
    match item {
        DeepHashItem::Blob(data) => {
            let tag = {
                let mut h = Sha384::new();
                h.update(b"blob");
                h.update(data.len().to_string().as_bytes());
                h.finalize()
            };
            let data_hash = Sha384::digest(data);
            let mut h = Sha384::new();
            h.update(tag);
            h.update(data_hash);
            h.finalize().into()
        }
        DeepHashItem::List(items) => {
            let tag = {
                let mut h = Sha384::new();
                h.update(b"list");
                h.update(items.len().to_string().as_bytes());
                h.finalize()
            };
            let mut acc: [u8; 48] = tag.into();
            for child in items {
                let child_hash = deep_hash(child);
                let mut h = Sha384::new();
                h.update(acc);
                h.update(child_hash);
                acc = h.finalize().into();
            }
            acc
        }
    }
}

/// Encode tags using Apache Avro binary format for ANS-104.
fn encode_avro_tags(tags: &[Tag]) -> Vec<u8> {
    if tags.is_empty() {
        return vec![0]; // empty array terminator
    }

    let mut buf = Vec::new();

    // Write the count as a zigzag-encoded varint
    write_avro_long(&mut buf, tags.len() as i64);

    for tag in tags {
        // Write name bytes
        let name_bytes = tag.name.as_bytes();
        write_avro_long(&mut buf, name_bytes.len() as i64);
        buf.extend_from_slice(name_bytes);

        // Write value bytes
        let value_bytes = tag.value.as_bytes();
        write_avro_long(&mut buf, value_bytes.len() as i64);
        buf.extend_from_slice(value_bytes);
    }

    // Array terminator
    buf.push(0);

    buf
}

/// Write a long value as a zigzag-encoded variable-length integer.
fn write_avro_long(buf: &mut Vec<u8>, n: i64) {
    let mut v = ((n << 1) ^ (n >> 63)) as u64;
    loop {
        if v & !0x7F == 0 {
            buf.push(v as u8);
            break;
        }
        buf.push((v as u8 & 0x7F) | 0x80);
        v >>= 7;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avro_zigzag_encoding() {
        let mut buf = Vec::new();
        write_avro_long(&mut buf, 0);
        assert_eq!(buf, vec![0]);

        buf.clear();
        write_avro_long(&mut buf, 1);
        assert_eq!(buf, vec![2]);

        buf.clear();
        write_avro_long(&mut buf, -1);
        assert_eq!(buf, vec![1]);

        buf.clear();
        write_avro_long(&mut buf, 64);
        assert_eq!(buf, vec![0x80, 0x01]);
    }

    #[test]
    fn test_encode_avro_tags() {
        let tags = vec![
            Tag::new("Content-Type", "application/json"),
            Tag::new("App-Name", "UnlimitedContext"),
        ];
        let encoded = encode_avro_tags(&tags);
        // Should start with count (2 = zigzag 4) and end with 0 terminator
        assert_eq!(encoded[0], 4); // zigzag(2) = 4
        assert_eq!(*encoded.last().unwrap(), 0);
    }

    #[test]
    fn test_encode_empty_tags() {
        let encoded = encode_avro_tags(&[]);
        assert_eq!(encoded, vec![0]);
    }

    #[test]
    fn test_deep_hash_blob() {
        // Basic sanity check: deep hash of a blob produces 48 bytes
        let item = DeepHashItem::Blob(b"hello".to_vec());
        let hash = deep_hash(&item);
        assert_eq!(hash.len(), 48);
    }

    #[test]
    fn test_serialize_data_item_structure() {
        let sig = vec![0u8; SIG_LENGTH];
        let owner = vec![0u8; OWNER_LENGTH];
        let tags = vec![Tag::new("App-Name", "Test")];
        let data = b"hello world";

        let bytes = serialize_data_item(&sig, &owner, None, None, &tags, data);

        // Check structure:
        // 2 (sig type) + 512 (sig) + 512 (owner) + 1 (no target) + 1 (no anchor)
        // + 8 (num tags) + 8 (tag bytes len) + tag_bytes + data
        assert!(bytes.len() > 2 + SIG_LENGTH + OWNER_LENGTH + 2 + 16);

        // Signature type should be 1 (little-endian)
        assert_eq!(bytes[0], 1);
        assert_eq!(bytes[1], 0);
    }
}
