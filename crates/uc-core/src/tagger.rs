use crate::models::Batch;
use uc_arweave::Tag;
use thiserror::Error;

pub const APP_NAME: &str = "Memoryport";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SCHEMA_VERSION: &str = "1";
const MAX_TAG_BYTES: usize = 2048;

#[derive(Debug, Error)]
pub enum TagError {
    #[error("tag budget exceeded: {total} bytes (max {MAX_TAG_BYTES})")]
    BudgetExceeded { total: usize },
}

/// Generate Arweave transaction tags for a batch of chunks.
pub fn generate_batch_tags(batch: &Batch, user_id: &str) -> Vec<Tag> {
    let mut tags = vec![
        Tag::new("Content-Type", "application/json"),
        Tag::new("App-Name", APP_NAME),
        Tag::new("App-Version", APP_VERSION),
        Tag::new("UC-Schema-Version", SCHEMA_VERSION),
        Tag::new("UC-User-Id", user_id),
        Tag::new("UC-Chunk-Count", batch.chunks.len().to_string()),
    ];

    // Add chunk type if all chunks share the same type
    if let Some(ct) = batch.dominant_chunk_type() {
        tags.push(Tag::new("UC-Chunk-Type", ct.as_str()));
    }

    // Add session ID if all chunks share the same session
    if let Some(sid) = batch.session_id_if_uniform() {
        tags.push(Tag::new("UC-Session-Id", sid));
    }

    // Add timestamp range
    if let Some((start, end)) = batch.timestamp_range() {
        tags.push(Tag::new("UC-Timestamp-Start", start.to_string()));
        tags.push(Tag::new("UC-Timestamp-End", end.to_string()));
    }

    tags
}

/// Validate that total tag bytes fit within Arweave's tag budget.
pub fn validate_tag_budget(tags: &[Tag]) -> Result<(), TagError> {
    let total: usize = tags.iter().map(|t| t.byte_size()).sum();
    if total > MAX_TAG_BYTES {
        return Err(TagError::BudgetExceeded { total });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Chunk, ChunkMetadata, ChunkType, Role};
    use uuid::Uuid;

    fn test_chunk(session_id: &str, ts: i64) -> Chunk {
        Chunk {
            id: Uuid::new_v4(),
            chunk_type: ChunkType::Conversation,
            session_id: session_id.to_string(),
            timestamp: ts,
            role: Some(Role::User),
            content: "test".into(),
            metadata: ChunkMetadata::default(),
        }
    }

    #[test]
    fn test_generate_tags() {
        let batch = Batch::new(vec![
            test_chunk("s1", 1000),
            test_chunk("s1", 2000),
        ], "user_123");
        let tags = generate_batch_tags(&batch, "user_123");

        let tag_names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        assert!(tag_names.contains(&"App-Name"));
        assert!(tag_names.contains(&"UC-User-Id"));
        assert!(tag_names.contains(&"UC-Session-Id"));
        assert!(tag_names.contains(&"UC-Timestamp-Start"));
        assert!(tag_names.contains(&"UC-Chunk-Count"));

        let user_tag = tags.iter().find(|t| t.name == "UC-User-Id").unwrap();
        assert_eq!(user_tag.value, "user_123");
    }

    #[test]
    fn test_tag_budget_validation() {
        let small_tags = vec![Tag::new("a", "b")];
        assert!(validate_tag_budget(&small_tags).is_ok());

        // Create tags that exceed the budget
        let big_tags: Vec<Tag> = (0..100)
            .map(|i| Tag::new(format!("tag-{i}"), "x".repeat(100)))
            .collect();
        assert!(validate_tag_budget(&big_tags).is_err());
    }
}
