use crate::models::{QuerySignals, RetrievalDecision};

/// Analyze a query string to extract retrieval signals and gating decision.
pub fn analyze_query(query: &str) -> QuerySignals {
    let lower = query.to_lowercase();
    let now_ms = chrono::Utc::now().timestamp_millis();

    let temporal_range = detect_temporal_range(&lower, now_ms);
    let explicit_session = detect_explicit_session(&lower);
    let is_recency_heavy = detect_recency_signals(&lower);

    // Gate 1: rule-based decision
    let decision = gate1_decide(&lower, temporal_range.is_some(), explicit_session.is_some(), is_recency_heavy);

    QuerySignals {
        decision,
        temporal_range,
        explicit_session,
        is_recency_heavy,
    }
}

fn gate1_decide(query: &str, has_temporal: bool, has_session: bool, is_recency: bool) -> RetrievalDecision {
    // Force retrieval for explicit memory/context signals
    if has_temporal || has_session || is_recency {
        return RetrievalDecision::Force;
    }

    let force_patterns = [
        "remember when",
        "remember that",
        "we discussed",
        "we talked about",
        "you told me",
        "you said",
        "you mentioned",
        "i mentioned",
        "i told you",
        "from our conversation",
        "from earlier",
        "what did we",
        "what did i",
        "do you recall",
        "as we discussed",
        "based on what",
        "referring to",
        "context from",
        "previous conversation",
        "prior conversation",
        "earlier conversation",
        "in our last",
    ];

    for pat in &force_patterns {
        if query.contains(pat) {
            return RetrievalDecision::Force;
        }
    }

    // Skip retrieval for greetings and simple interactions
    let skip_exact = [
        "hi", "hey", "hello", "yo", "sup",
        "thanks", "thank you", "thx", "ty",
        "bye", "goodbye", "see you",
        "ok", "okay", "sure", "yes", "no", "yep", "nope",
        "got it", "sounds good", "makes sense",
        "lgtm", "nice", "cool", "great",
    ];

    let trimmed = query.trim();
    for pat in &skip_exact {
        if trimmed == *pat || trimmed == format!("{pat}!") || trimmed == format!("{pat}.") {
            return RetrievalDecision::Skip;
        }
    }

    // Skip for command-like inputs
    if trimmed.starts_with('/') || trimmed.starts_with('!') {
        return RetrievalDecision::Skip;
    }

    let command_prefixes = [
        "fix ", "run ", "build ", "test ", "commit ",
        "deploy ", "install ", "update ", "delete ",
        "create ", "rename ", "move ", "copy ",
        "git ", "npm ", "cargo ", "make ",
    ];

    for prefix in &command_prefixes {
        if trimmed.starts_with(prefix) {
            return RetrievalDecision::Skip;
        }
    }

    // Skip very short queries with no question/memory signals
    let word_count = trimmed.split_whitespace().count();
    if word_count < 4 && !trimmed.contains('?') {
        return RetrievalDecision::Skip;
    }

    // Skip code-only inputs (backtick-wrapped or file paths)
    if (trimmed.starts_with('`') && trimmed.ends_with('`'))
        || (trimmed.starts_with("```") && trimmed.ends_with("```"))
        || trimmed.starts_with("./")
        || trimmed.starts_with("~/")
        || (trimmed.contains('/') && !trimmed.contains(' ') && trimmed.len() > 3)
    {
        return RetrievalDecision::Skip;
    }

    RetrievalDecision::Undecided
}

fn detect_temporal_range(query: &str, now_ms: i64) -> Option<(i64, i64)> {
    let day_ms: i64 = 86_400_000;
    let hour_ms: i64 = 3_600_000;

    if query.contains("yesterday") {
        return Some((now_ms - 2 * day_ms, now_ms - day_ms));
    }
    if query.contains("last week") {
        return Some((now_ms - 7 * day_ms, now_ms));
    }
    if query.contains("last month") {
        return Some((now_ms - 30 * day_ms, now_ms));
    }
    if query.contains("today") || query.contains("earlier today") {
        return Some((now_ms - day_ms, now_ms));
    }
    if query.contains("last hour") {
        return Some((now_ms - hour_ms, now_ms));
    }
    if query.contains("this morning") {
        return Some((now_ms - 12 * hour_ms, now_ms));
    }
    None
}

fn detect_explicit_session(query: &str) -> Option<String> {
    let patterns = ["session ", "session_"];
    for pat in patterns {
        if let Some(pos) = query.find(pat) {
            let rest = &query[pos + pat.len()..];
            let session_id: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if !session_id.is_empty() {
                return Some(session_id);
            }
        }
    }
    None
}

fn detect_recency_signals(query: &str) -> bool {
    let signals = [
        "just now", "recent", "recently", "latest",
        "last thing", "a moment ago",
        "just said", "just told", "just asked",
        "what did i just", "what we just",
    ];
    signals.iter().any(|s| query.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Gate 1: Skip tests
    #[test]
    fn test_skip_greeting() {
        let s = analyze_query("hello");
        assert_eq!(s.decision, RetrievalDecision::Skip);
    }

    #[test]
    fn test_skip_thanks() {
        let s = analyze_query("thanks!");
        assert_eq!(s.decision, RetrievalDecision::Skip);
    }

    #[test]
    fn test_skip_command() {
        let s = analyze_query("fix the typo on line 5");
        assert_eq!(s.decision, RetrievalDecision::Skip);
    }

    #[test]
    fn test_skip_short() {
        let s = analyze_query("do it");
        assert_eq!(s.decision, RetrievalDecision::Skip);
    }

    #[test]
    fn test_skip_slash_command() {
        let s = analyze_query("/commit");
        assert_eq!(s.decision, RetrievalDecision::Skip);
    }

    #[test]
    fn test_skip_code_path() {
        let s = analyze_query("src/main.rs");
        assert_eq!(s.decision, RetrievalDecision::Skip);
    }

    // Gate 1: Force tests
    #[test]
    fn test_force_memory_reference() {
        let s = analyze_query("what did we discuss about the auth system?");
        assert_eq!(s.decision, RetrievalDecision::Force);
    }

    #[test]
    fn test_force_temporal() {
        let s = analyze_query("what happened yesterday?");
        assert_eq!(s.decision, RetrievalDecision::Force);
    }

    #[test]
    fn test_force_recency() {
        let s = analyze_query("what did I just ask about?");
        assert_eq!(s.decision, RetrievalDecision::Force);
    }

    #[test]
    fn test_force_you_told_me() {
        let s = analyze_query("you told me something about Arweave pricing");
        assert_eq!(s.decision, RetrievalDecision::Force);
    }

    // Gate 1: Undecided tests
    #[test]
    fn test_undecided_question() {
        let s = analyze_query("How does Arweave pricing work?");
        assert_eq!(s.decision, RetrievalDecision::Undecided);
    }

    #[test]
    fn test_undecided_explain() {
        let s = analyze_query("Explain the architecture of this system in detail");
        assert_eq!(s.decision, RetrievalDecision::Undecided);
    }

    // Existing signal tests
    #[test]
    fn test_temporal_yesterday() {
        let s = analyze_query("What did we discuss yesterday?");
        assert!(s.temporal_range.is_some());
        assert_eq!(s.decision, RetrievalDecision::Force);
    }

    #[test]
    fn test_explicit_session() {
        let s = analyze_query("Show me session abc123");
        assert_eq!(s.explicit_session, Some("abc123".to_string()));
    }
}
