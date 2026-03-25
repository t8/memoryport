use crate::models::QuerySignals;

/// Analyze a query string to extract retrieval signals.
pub fn analyze_query(query: &str) -> QuerySignals {
    let lower = query.to_lowercase();
    let now = chrono::Utc::now();
    let now_ms = now.timestamp_millis();

    let temporal_range = detect_temporal_range(&lower, now_ms);
    let explicit_session = detect_explicit_session(&lower);
    let is_recency_heavy = detect_recency_signals(&lower);

    QuerySignals {
        temporal_range,
        explicit_session,
        is_recency_heavy,
    }
}

fn detect_temporal_range(query: &str, now_ms: i64) -> Option<(i64, i64)> {
    let day_ms: i64 = 86_400_000;
    let hour_ms: i64 = 3_600_000;

    if query.contains("yesterday") {
        let start = now_ms - 2 * day_ms;
        let end = now_ms - day_ms;
        return Some((start, end));
    }
    if query.contains("last week") {
        let start = now_ms - 7 * day_ms;
        return Some((start, now_ms));
    }
    if query.contains("last month") {
        let start = now_ms - 30 * day_ms;
        return Some((start, now_ms));
    }
    if query.contains("today") || query.contains("earlier today") {
        let start = now_ms - day_ms;
        return Some((start, now_ms));
    }
    if query.contains("last hour") {
        let start = now_ms - hour_ms;
        return Some((start, now_ms));
    }
    if query.contains("this morning") {
        let start = now_ms - 12 * hour_ms;
        return Some((start, now_ms));
    }

    None
}

fn detect_explicit_session(query: &str) -> Option<String> {
    // Look for patterns like "session abc123" or "session_abc123"
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
        "just now",
        "recent",
        "recently",
        "latest",
        "last thing",
        "a moment ago",
        "just said",
        "just told",
        "just asked",
        "what did i just",
        "what we just",
    ];
    signals.iter().any(|s| query.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporal_yesterday() {
        let signals = analyze_query("What did we discuss yesterday?");
        assert!(signals.temporal_range.is_some());
    }

    #[test]
    fn test_temporal_last_week() {
        let signals = analyze_query("Show me conversations from last week");
        assert!(signals.temporal_range.is_some());
        let (start, end) = signals.temporal_range.unwrap();
        assert!(end > start);
    }

    #[test]
    fn test_no_temporal() {
        let signals = analyze_query("How does Arweave pricing work?");
        assert!(signals.temporal_range.is_none());
    }

    #[test]
    fn test_explicit_session() {
        let signals = analyze_query("Show me session abc123");
        assert_eq!(signals.explicit_session, Some("abc123".to_string()));
    }

    #[test]
    fn test_recency_signals() {
        let signals = analyze_query("What did I just ask about?");
        assert!(signals.is_recency_heavy);
    }

    #[test]
    fn test_no_recency() {
        let signals = analyze_query("Explain the architecture of this system");
        assert!(!signals.is_recency_heavy);
    }
}
