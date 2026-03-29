use crate::models::{Chunk, ChunkMetadata, ChunkType, Role};
use uuid::Uuid;

/// Configuration for the chunker.
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Target chunk size in characters (~4 chars per token).
    pub target_size: usize,
    /// Overlap between adjacent chunks in characters.
    pub overlap: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            target_size: 1500, // ~375 tokens
            overlap: 200,      // ~50 tokens
        }
    }
}

/// Split text into chunks using fixed-size-with-overlap, breaking at sentence boundaries.
pub fn chunk_text(
    text: &str,
    session_id: &str,
    chunk_type: ChunkType,
    role: Option<Role>,
    config: &ChunkerConfig,
    base_timestamp: i64,
) -> Vec<Chunk> {
    if text.is_empty() {
        return Vec::new();
    }

    // If text fits in a single chunk, return it directly
    if text.len() <= config.target_size {
        return vec![make_chunk(text, session_id, chunk_type, role, base_timestamp)];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = std::cmp::min(start + config.target_size, text.len());

        // Try to find a sentence boundary near the end
        let split_at = if end < text.len() {
            find_sentence_boundary(text, end, config.target_size / 4)
        } else {
            end
        };

        let chunk_text = &text[start..split_at];
        let ts = base_timestamp + chunks.len() as i64; // monotonically increasing
        chunks.push(make_chunk(chunk_text.trim(), session_id, chunk_type, role, ts));

        // Move start forward, accounting for overlap
        start = if split_at >= config.overlap {
            split_at - config.overlap
        } else {
            split_at
        };

        // Avoid infinite loop if we can't make progress
        if start >= text.len() || (split_at == start + config.overlap && split_at >= text.len()) {
            break;
        }
    }

    chunks
}

/// Split a multi-turn conversation into per-turn chunks.
/// Each (role, content) pair becomes one or more chunks.
pub fn chunk_conversation(
    turns: &[(Role, &str)],
    session_id: &str,
    config: &ChunkerConfig,
    base_timestamp: i64,
) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut ts = base_timestamp;

    for (role, content) in turns {
        let turn_chunks = chunk_text(
            content,
            session_id,
            ChunkType::Conversation,
            Some(*role),
            config,
            ts,
        );
        ts += turn_chunks.len() as i64;
        chunks.extend(turn_chunks);
    }

    chunks
}

/// Split a multi-turn conversation into round-level chunks.
/// Each user+assistant pair becomes a single chunk, preserving the Q&A context.
/// This improves embedding quality because the assistant's answer is embedded
/// alongside the question it answers (LongMemEval paper's #1 recommendation).
pub fn chunk_conversation_rounds(
    turns: &[(Role, &str)],
    session_id: &str,
    config: &ChunkerConfig,
    base_timestamp: i64,
) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut ts = base_timestamp;
    let mut i = 0;

    while i < turns.len() {
        let (role, content) = &turns[i];

        // Try to pair user+assistant as a round
        if *role == Role::User && i + 1 < turns.len() && turns[i + 1].0 == Role::Assistant {
            let round_text = format!(
                "User: {}\nAssistant: {}",
                content, turns[i + 1].1
            );
            let round_chunks = chunk_text(
                &round_text,
                session_id,
                ChunkType::Conversation,
                Some(Role::User), // Tag as user since the question drives retrieval
                config,
                ts,
            );
            ts += round_chunks.len() as i64;
            chunks.extend(round_chunks);
            i += 2; // Skip both turns
        } else {
            // Unpaired turn (e.g., system message, or trailing user turn)
            let turn_chunks = chunk_text(
                content,
                session_id,
                ChunkType::Conversation,
                Some(*role),
                config,
                ts,
            );
            ts += turn_chunks.len() as i64;
            chunks.extend(turn_chunks);
            i += 1;
        }
    }

    chunks
}

fn make_chunk(
    text: &str,
    session_id: &str,
    chunk_type: ChunkType,
    role: Option<Role>,
    timestamp: i64,
) -> Chunk {
    let token_count = estimate_tokens(text);
    Chunk {
        id: Uuid::new_v4(),
        chunk_type,
        session_id: session_id.to_string(),
        timestamp,
        role,
        content: text.to_string(),
        metadata: ChunkMetadata {
            token_count,
            language: None,
            source_integration: None,
            source_model: None,
            extra: Default::default(),
        },
    }
}

/// Rough token count estimate (~4 chars per token for English).
fn estimate_tokens(text: &str) -> u32 {
    (text.len() as f64 / 4.0).ceil() as u32
}

/// Find the nearest sentence boundary within a window around the target position.
/// Looks for '.', '!', '?', or '\n' followed by whitespace.
fn find_sentence_boundary(text: &str, target: usize, window: usize) -> usize {
    let search_start = target.saturating_sub(window);
    let search_end = std::cmp::min(target + window / 2, text.len());
    let search_region = &text[search_start..search_end];

    // Look backwards from target for sentence endings
    let mut best = target;
    for (i, c) in search_region.char_indices().rev() {
        let abs_pos = search_start + i + c.len_utf8();
        if abs_pos > target + window / 2 {
            continue;
        }
        if (c == '.' || c == '!' || c == '?' || c == '\n') && abs_pos <= target + window / 2 {
            // Check if followed by whitespace or end of string
            if abs_pos >= text.len() || text[abs_pos..].starts_with(char::is_whitespace) {
                best = abs_pos;
                break;
            }
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_text_single_chunk() {
        let chunks = chunk_text(
            "Hello, world!",
            "s1",
            ChunkType::Conversation,
            Some(Role::User),
            &ChunkerConfig::default(),
            1000,
        );
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "Hello, world!");
        assert_eq!(chunks[0].session_id, "s1");
    }

    #[test]
    fn test_long_text_multiple_chunks() {
        let text = "A".repeat(4000);
        let config = ChunkerConfig {
            target_size: 1000,
            overlap: 100,
        };
        let chunks = chunk_text(&text, "s1", ChunkType::Document, None, &config, 1000);
        assert!(chunks.len() > 1);
        // All content should be present
        for chunk in &chunks {
            assert!(!chunk.content.is_empty());
        }
    }

    #[test]
    fn test_conversation_chunking() {
        let turns = vec![
            (Role::User, "Hello, how are you?"),
            (Role::Assistant, "I'm doing well, thanks for asking!"),
            (Role::User, "Can you help me with a project?"),
        ];
        let chunks = chunk_conversation(&turns, "s1", &ChunkerConfig::default(), 1000);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].role, Some(Role::User));
        assert_eq!(chunks[1].role, Some(Role::Assistant));
        assert_eq!(chunks[2].role, Some(Role::User));
    }

    #[test]
    fn test_empty_text() {
        let chunks = chunk_text("", "s1", ChunkType::Document, None, &ChunkerConfig::default(), 1000);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_token_estimate() {
        assert_eq!(estimate_tokens("hello world"), 3); // 11 chars / 4 = 2.75 -> 3
    }
}
