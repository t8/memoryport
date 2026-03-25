use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChunkType {
    Conversation,
    Document,
    Knowledge,
}

impl ChunkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Document => "document",
            Self::Knowledge => "knowledge",
        }
    }
}

impl std::fmt::Display for ChunkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ChunkType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "conversation" => Ok(Self::Conversation),
            "document" => Ok(Self::Document),
            "knowledge" => Ok(Self::Knowledge),
            _ => Err(format!("unknown chunk type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            _ => Err(format!("unknown role: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: Uuid,
    pub chunk_type: ChunkType,
    pub session_id: String,
    pub timestamp: i64,
    pub role: Option<Role>,
    pub content: String,
    pub metadata: ChunkMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub token_count: u32,
    #[serde(default)]
    pub language: Option<String>,
    /// Where this chunk was stored from: "mcp", "proxy", "cli", "api"
    #[serde(default)]
    pub source_integration: Option<String>,
    /// Which LLM model was active: "claude-opus-4-20250514", "gpt-4o", etc.
    #[serde(default)]
    pub source_model: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    pub id: Uuid,
    pub schema_version: u32,
    pub user_id: String,
    pub chunks: Vec<Chunk>,
}

impl Batch {
    pub fn new(chunks: Vec<Chunk>, user_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            schema_version: 1,
            user_id: user_id.into(),
            chunks,
        }
    }

    pub fn timestamp_range(&self) -> Option<(i64, i64)> {
        if self.chunks.is_empty() {
            return None;
        }
        let min = self.chunks.iter().map(|c| c.timestamp).min().unwrap();
        let max = self.chunks.iter().map(|c| c.timestamp).max().unwrap();
        Some((min, max))
    }

    pub fn dominant_chunk_type(&self) -> Option<ChunkType> {
        self.chunks.first().map(|c| c.chunk_type)
    }

    pub fn session_id_if_uniform(&self) -> Option<&str> {
        let first = self.chunks.first()?;
        if self.chunks.iter().all(|c| c.session_id == first.session_id) {
            Some(&first.session_id)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct StoreParams {
    pub user_id: String,
    pub session_id: String,
    pub chunk_type: ChunkType,
    pub role: Option<Role>,
    pub source_integration: Option<String>,
    pub source_model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct QueryParams {
    pub user_id: String,
    pub top_k: usize,
    pub session_id: Option<String>,
    pub chunk_type: Option<ChunkType>,
    pub time_range: Option<(i64, i64)>,
}

impl Default for QueryParams {
    fn default() -> Self {
        Self {
            user_id: "default".into(),
            top_k: 10,
            session_id: None,
            chunk_type: None,
            time_range: None,
        }
    }
}

/// Whether the gating system thinks retrieval is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalDecision {
    /// Gate 1 determined retrieval should be skipped (greeting, command, etc.)
    Skip,
    /// Gate 1 determined retrieval is definitely needed (memory reference, temporal, etc.)
    Force,
    /// Gate 1 couldn't decide — pass to Gate 2 (embedding routing)
    Undecided,
}

impl Default for RetrievalDecision {
    fn default() -> Self {
        Self::Undecided
    }
}

/// Signals extracted from a user query by the analyzer.
#[derive(Debug, Clone, Default)]
pub struct QuerySignals {
    pub decision: RetrievalDecision,
    pub temporal_range: Option<(i64, i64)>,
    pub explicit_session: Option<String>,
    pub is_recency_heavy: bool,
}

/// Summary info for a stored session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub chunk_count: usize,
    pub first_timestamp: i64,
    pub last_timestamp: i64,
}

/// Assembled context ready for LLM injection.
#[derive(Debug, Clone)]
pub struct AssembledContext {
    pub formatted: String,
    pub token_count: u32,
    pub chunks_included: usize,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub chunk_id: String,
    pub session_id: String,
    pub chunk_type: ChunkType,
    pub role: Option<Role>,
    pub timestamp: i64,
    pub content: String,
    pub score: f32,
    pub arweave_tx_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadReceipt {
    pub tx_id: String,
    pub timestamp: DateTime<Utc>,
}

/// Payload format for Arweave transaction data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPayload {
    pub schema_version: u32,
    pub batch_id: String,
    pub chunks: Vec<ChunkPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkPayload {
    pub id: String,
    #[serde(rename = "type")]
    pub chunk_type: ChunkType,
    pub session_id: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub content: String,
    pub metadata: ChunkMetadata,
}

impl From<&Batch> for BatchPayload {
    fn from(batch: &Batch) -> Self {
        Self {
            schema_version: batch.schema_version,
            batch_id: batch.id.to_string(),
            chunks: batch
                .chunks
                .iter()
                .map(|c| ChunkPayload {
                    id: c.id.to_string(),
                    chunk_type: c.chunk_type,
                    session_id: c.session_id.clone(),
                    timestamp: c.timestamp,
                    role: c.role,
                    content: c.content.clone(),
                    metadata: c.metadata.clone(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_type_roundtrip() {
        for ct in [ChunkType::Conversation, ChunkType::Document, ChunkType::Knowledge] {
            let s = ct.as_str();
            let parsed: ChunkType = s.parse().unwrap();
            assert_eq!(ct, parsed);
        }
    }

    #[test]
    fn test_role_roundtrip() {
        for r in [Role::User, Role::Assistant, Role::System] {
            let s = r.as_str();
            let parsed: Role = s.parse().unwrap();
            assert_eq!(r, parsed);
        }
    }

    #[test]
    fn test_batch_timestamp_range() {
        let batch = Batch::new(vec![
            Chunk {
                id: Uuid::new_v4(),
                chunk_type: ChunkType::Conversation,
                session_id: "s1".into(),
                timestamp: 100,
                role: Some(Role::User),
                content: "hello".into(),
                metadata: ChunkMetadata::default(),
            },
            Chunk {
                id: Uuid::new_v4(),
                chunk_type: ChunkType::Conversation,
                session_id: "s1".into(),
                timestamp: 200,
                role: Some(Role::Assistant),
                content: "hi".into(),
                metadata: ChunkMetadata::default(),
            },
        ], "user_123");
        assert_eq!(batch.timestamp_range(), Some((100, 200)));
        assert_eq!(batch.session_id_if_uniform(), Some("s1"));
    }
}
