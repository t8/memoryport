use crate::models::{AssembledContext, ChunkType, SearchResult};
use chrono::{TimeZone, Utc};
use std::collections::BTreeMap;

/// Assemble ranked search results into structured XML context for LLM injection.
pub fn assemble_context(
    results: &[SearchResult],
    max_tokens: u32,
) -> AssembledContext {
    if results.is_empty() {
        return AssembledContext {
            formatted: String::new(),
            token_count: 0,
            chunks_included: 0,
        };
    }

    let mut budget = max_tokens;
    let mut included_results: Vec<&SearchResult> = Vec::new();

    // Greedily fill from ranked results until budget exhausted
    for result in results {
        let chunk_tokens = estimate_tokens(&result.content);
        if chunk_tokens <= budget {
            included_results.push(result);
            budget -= chunk_tokens;
        } else if budget > 50 {
            // Include truncated version if we have enough budget
            included_results.push(result);
            break;
        } else {
            break;
        }
    }

    let formatted = format_xml(&included_results, max_tokens);
    let token_count = estimate_tokens(&formatted);

    AssembledContext {
        formatted,
        token_count,
        chunks_included: included_results.len(),
    }
}

fn format_xml(results: &[&SearchResult], max_tokens: u32) -> String {
    let mut out = String::from("<unlimited_context>\n");

    // Group by session for conversation chunks, keep others flat
    let mut sessions: BTreeMap<&str, Vec<&SearchResult>> = BTreeMap::new();
    let mut documents: Vec<&SearchResult> = Vec::new();
    let mut knowledge: Vec<&SearchResult> = Vec::new();

    for r in results {
        match r.chunk_type {
            ChunkType::Conversation => {
                sessions.entry(&r.session_id).or_default().push(r);
            }
            ChunkType::Document => documents.push(r),
            ChunkType::Knowledge => knowledge.push(r),
        }
    }

    // Format sessions
    for (session_id, mut turns) in sessions {
        turns.sort_by_key(|t| t.timestamp);
        let date = format_timestamp(turns.first().map(|t| t.timestamp).unwrap_or(0));
        out.push_str(&format!("  <session id=\"{session_id}\" date=\"{date}\">\n"));
        for turn in &turns {
            let time = format_time(turn.timestamp);
            let role = turn.role.map(|r| r.as_str()).unwrap_or("unknown");
            let content = truncate_content(&turn.content, max_tokens as usize / results.len().max(1));
            out.push_str(&format!("    <turn role=\"{role}\" time=\"{time}\">{content}</turn>\n"));
        }
        out.push_str("  </session>\n");
    }

    // Format documents
    for doc in &documents {
        let stored = format_timestamp(doc.timestamp);
        let content = truncate_content(&doc.content, max_tokens as usize / results.len().max(1));
        out.push_str(&format!("  <document stored=\"{stored}\">{content}</document>\n"));
    }

    // Format knowledge
    for k in &knowledge {
        let stored = format_timestamp(k.timestamp);
        let content = truncate_content(&k.content, max_tokens as usize / results.len().max(1));
        out.push_str(&format!("  <knowledge stored=\"{stored}\">{content}</knowledge>\n"));
    }

    out.push_str("</unlimited_context>");
    out
}

fn format_timestamp(ts_ms: i64) -> String {
    Utc.timestamp_millis_opt(ts_ms)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn format_time(ts_ms: i64) -> String {
    Utc.timestamp_millis_opt(ts_ms)
        .single()
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| "??:??".into())
}

fn truncate_content(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        return s;
    }
    let mut end = max_chars;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn estimate_tokens(text: &str) -> u32 {
    (text.len() as f64 / 4.0).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChunkType, Role};

    fn make_result(chunk_type: ChunkType, session: &str, role: Option<Role>, ts: i64, content: &str) -> SearchResult {
        SearchResult {
            chunk_id: "c1".into(),
            session_id: session.into(),
            chunk_type,
            role,
            timestamp: ts,
            content: content.into(),
            score: 0.9,
            arweave_tx_id: "tx".into(),
        }
    }

    #[test]
    fn test_assemble_empty() {
        let ctx = assemble_context(&[], 1000);
        assert_eq!(ctx.chunks_included, 0);
        assert!(ctx.formatted.is_empty());
    }

    #[test]
    fn test_assemble_conversation() {
        let results = vec![
            make_result(ChunkType::Conversation, "s1", Some(Role::User), 1711324800000, "Hello"),
            make_result(ChunkType::Conversation, "s1", Some(Role::Assistant), 1711324860000, "Hi there"),
        ];
        let ctx = assemble_context(&results, 5000);
        assert!(ctx.formatted.contains("<unlimited_context>"));
        assert!(ctx.formatted.contains("<session id=\"s1\""));
        assert!(ctx.formatted.contains("role=\"user\""));
        assert!(ctx.formatted.contains("role=\"assistant\""));
        assert!(ctx.formatted.contains("</unlimited_context>"));
        assert_eq!(ctx.chunks_included, 2);
    }

    #[test]
    fn test_assemble_mixed_types() {
        let results = vec![
            make_result(ChunkType::Conversation, "s1", Some(Role::User), 1000, "Hello"),
            make_result(ChunkType::Document, "s1", None, 2000, "Some document content"),
            make_result(ChunkType::Knowledge, "s1", None, 3000, "A knowledge fact"),
        ];
        let ctx = assemble_context(&results, 5000);
        assert!(ctx.formatted.contains("<session"));
        assert!(ctx.formatted.contains("<document"));
        assert!(ctx.formatted.contains("<knowledge"));
    }

    #[test]
    fn test_token_budget() {
        let long_content = "x".repeat(10000);
        let results = vec![
            make_result(ChunkType::Conversation, "s1", Some(Role::User), 1000, &long_content),
        ];
        // Very small budget
        let ctx = assemble_context(&results, 100);
        assert!(ctx.chunks_included <= 1);
    }
}
