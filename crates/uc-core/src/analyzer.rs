use crate::models::{QuerySignals, RetrievalDecision};

/// Analyze a query string to extract retrieval signals and gating decision.
pub fn analyze_query(query: &str) -> QuerySignals {
    analyze_query_at(query, chrono::Utc::now().timestamp_millis())
}

/// Analyze with a specific reference timestamp (for testing / benchmarks).
pub fn analyze_query_at(query: &str, reference_time_ms: i64) -> QuerySignals {
    let lower = query.to_lowercase();

    let temporal_range = detect_temporal_range(&lower, reference_time_ms);
    let explicit_session = detect_explicit_session(&lower);
    let is_recency_heavy = detect_recency_signals(&lower);

    let decision = gate1_decide(
        &lower,
        temporal_range.is_some(),
        explicit_session.is_some(),
        is_recency_heavy,
    );

    QuerySignals {
        decision,
        temporal_range,
        explicit_session,
        is_recency_heavy,
    }
}

fn gate1_decide(
    query: &str,
    has_temporal: bool,
    has_session: bool,
    is_recency: bool,
) -> RetrievalDecision {
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
        "happened first",
        "happened before",
        "happened after",
        "how long",
        "how many days",
        "how many weeks",
        "how many months",
        "what order",
        "which came first",
        "which was first",
        "chronological",
        "timeline",
        "sequence of events",
        "when did i",
        "when was the",
        "what date",
        "what day",
    ];

    for pat in &force_patterns {
        if query.contains(pat) {
            return RetrievalDecision::Force;
        }
    }

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

    let word_count = trimmed.split_whitespace().count();
    if word_count < 4 && !trimmed.contains('?') {
        return RetrievalDecision::Skip;
    }

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
    let week_ms: i64 = 7 * day_ms;
    let month_ms: i64 = 30 * day_ms;

    // Exact relative time references
    if query.contains("yesterday") {
        return Some((now_ms - 2 * day_ms, now_ms - day_ms));
    }
    if query.contains("today") || query.contains("earlier today") {
        return Some((now_ms - day_ms, now_ms));
    }
    if query.contains("this morning") {
        return Some((now_ms - 12 * hour_ms, now_ms));
    }
    if query.contains("last hour") {
        return Some((now_ms - hour_ms, now_ms));
    }

    // "last N days/weeks/months"
    if let Some(range) = parse_last_n(query, "day", day_ms, now_ms) {
        return Some(range);
    }
    if let Some(range) = parse_last_n(query, "week", week_ms, now_ms) {
        return Some(range);
    }
    if let Some(range) = parse_last_n(query, "month", month_ms, now_ms) {
        return Some(range);
    }

    // "past N days/weeks/months"
    if let Some(range) = parse_past_n(query, "day", day_ms, now_ms) {
        return Some(range);
    }
    if let Some(range) = parse_past_n(query, "week", week_ms, now_ms) {
        return Some(range);
    }
    if let Some(range) = parse_past_n(query, "month", month_ms, now_ms) {
        return Some(range);
    }

    // Named relative periods
    if query.contains("last week") {
        return Some((now_ms - 2 * week_ms, now_ms - week_ms));
    }
    if query.contains("this week") {
        return Some((now_ms - week_ms, now_ms));
    }
    if query.contains("last month") {
        return Some((now_ms - 2 * month_ms, now_ms - month_ms));
    }
    if query.contains("this month") {
        return Some((now_ms - month_ms, now_ms));
    }
    if query.contains("last year") {
        return Some((now_ms - 365 * day_ms, now_ms));
    }

    // Day of week references ("last saturday", "last monday", etc.)
    let days_of_week = [
        ("monday", 0),
        ("tuesday", 1),
        ("wednesday", 2),
        ("thursday", 3),
        ("friday", 4),
        ("saturday", 5),
        ("sunday", 6),
    ];
    for (day_name, _target_dow) in &days_of_week {
        if query.contains(&format!("last {day_name}")) || query.contains(&format!("past {day_name}")) {
            // Approximate: last occurrence of this day = 1-7 days ago
            return Some((now_ms - 8 * day_ms, now_ms - 1 * day_ms));
        }
    }

    // "N days/weeks/months ago"
    if let Some(range) = parse_n_ago(query, "day", day_ms, now_ms) {
        return Some(range);
    }
    if let Some(range) = parse_n_ago(query, "week", week_ms, now_ms) {
        return Some(range);
    }
    if let Some(range) = parse_n_ago(query, "month", month_ms, now_ms) {
        return Some(range);
    }

    // Broad temporal signals that don't narrow to a specific range
    // but should still trigger retrieval (handled by Force in gate1)
    None
}

/// Parse "last N days/weeks/months" or "last two days", "last three weeks", etc.
fn parse_last_n(query: &str, unit: &str, unit_ms: i64, now_ms: i64) -> Option<(i64, i64)> {
    // "last N <unit>s" or "last N <unit>"
    let _patterns = [
        format!("last {} {}", "{}", unit),
        format!("last {} {}s", "{}", unit),
    ];

    // Try numeric
    for word in query.split_whitespace() {
        if let Ok(n) = word.parse::<i64>() {
            // Check if "last" appears before and unit appears after
            if let Some(pos) = query.find(word) {
                let before = &query[..pos];
                let after = &query[pos + word.len()..];
                if before.contains("last") && (after.contains(unit) || after.contains(&format!("{unit}s"))) {
                    return Some((now_ms - n * unit_ms, now_ms));
                }
            }
        }
    }

    // Try word numbers
    let word_nums = [
        ("two", 2), ("three", 3), ("four", 4), ("five", 5),
        ("six", 6), ("seven", 7), ("eight", 8), ("nine", 9), ("ten", 10),
        ("couple", 2), ("few", 3), ("several", 5),
    ];
    for (word, n) in &word_nums {
        let pat1 = format!("last {word} {unit}");
        let pat2 = format!("last {word} {unit}s");
        if query.contains(&pat1) || query.contains(&pat2) {
            return Some((now_ms - (*n as i64) * unit_ms, now_ms));
        }
    }

    None
}

/// Parse "past N days/weeks/months" or "past two months", etc.
fn parse_past_n(query: &str, unit: &str, unit_ms: i64, now_ms: i64) -> Option<(i64, i64)> {
    // Try numeric
    for word in query.split_whitespace() {
        if let Ok(n) = word.parse::<i64>() {
            if let Some(pos) = query.find(word) {
                let before = &query[..pos];
                let after = &query[pos + word.len()..];
                if before.contains("past") && (after.contains(unit) || after.contains(&format!("{unit}s"))) {
                    return Some((now_ms - n * unit_ms, now_ms));
                }
            }
        }
    }

    let word_nums = [
        ("two", 2), ("three", 3), ("four", 4), ("five", 5),
        ("six", 6), ("seven", 7), ("eight", 8), ("nine", 9), ("ten", 10),
        ("couple", 2), ("few", 3), ("several", 5),
    ];
    for (word, n) in &word_nums {
        let pat1 = format!("past {word} {unit}");
        let pat2 = format!("past {word} {unit}s");
        if query.contains(&pat1) || query.contains(&pat2) {
            return Some((now_ms - (*n as i64) * unit_ms, now_ms));
        }
    }

    None
}

/// Parse "N days/weeks/months ago"
fn parse_n_ago(query: &str, unit: &str, unit_ms: i64, now_ms: i64) -> Option<(i64, i64)> {
    let ago_pat1 = format!("{unit}s ago");
    let ago_pat2 = format!("{unit} ago");

    if query.contains(&ago_pat1) || query.contains(&ago_pat2) || query.contains(&format!("{unit}s ago")) || query.contains(&format!("{unit} ago")) {
        // Find the number before "days ago"
        let words: Vec<&str> = query.split_whitespace().collect();
        for (i, w) in words.iter().enumerate() {
            let w_clean = w.trim_end_matches(|c: char| !c.is_alphanumeric());
            if (w_clean == "ago") && i >= 2 {
                let unit_word = words[i - 1].trim_end_matches(|c: char| !c.is_alphanumeric());
                if unit_word.starts_with(unit) {
                    let num_word = words[i - 2];
                    if let Ok(n) = num_word.parse::<i64>() {
                        let center = now_ms - n * unit_ms;
                        return Some((center - unit_ms, center + unit_ms));
                    }
                    let word_nums = [
                        ("two", 2i64), ("three", 3), ("four", 4), ("five", 5),
                        ("six", 6), ("seven", 7), ("eight", 8), ("a", 1),
                    ];
                    for (word, n) in &word_nums {
                        if num_word == *word {
                            let center = now_ms - n * unit_ms;
                            return Some((center - unit_ms, center + unit_ms));
                        }
                    }
                }
            }
        }
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
        "most recent",
    ];
    signals.iter().any(|s| query.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Gate 1 tests
    #[test]
    fn test_skip_greeting() {
        let s = analyze_query("hello");
        assert_eq!(s.decision, RetrievalDecision::Skip);
    }

    #[test]
    fn test_skip_command() {
        let s = analyze_query("fix the typo on line 5");
        assert_eq!(s.decision, RetrievalDecision::Skip);
    }

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
    fn test_force_temporal_ordering() {
        let s = analyze_query("which event happened first?");
        assert_eq!(s.decision, RetrievalDecision::Force);
    }

    #[test]
    fn test_force_how_many_days() {
        let s = analyze_query("how many days between the two events?");
        assert_eq!(s.decision, RetrievalDecision::Force);
    }

    // Temporal range tests
    #[test]
    fn test_temporal_yesterday() {
        let s = analyze_query("What did we discuss yesterday?");
        assert!(s.temporal_range.is_some());
    }

    #[test]
    fn test_temporal_last_week() {
        let s = analyze_query("Show me conversations from last week");
        assert!(s.temporal_range.is_some());
    }

    #[test]
    fn test_temporal_past_two_months() {
        let now = chrono::Utc::now().timestamp_millis();
        let s = analyze_query_at("events in the past two months", now);
        assert!(s.temporal_range.is_some());
        let (start, end) = s.temporal_range.unwrap();
        let two_months_ms = 60 * 86_400_000i64;
        assert!(end - start >= two_months_ms - 86_400_000); // ~60 days ± 1 day
    }

    #[test]
    fn test_temporal_last_3_days() {
        let now = chrono::Utc::now().timestamp_millis();
        let s = analyze_query_at("what happened in the last 3 days?", now);
        assert!(s.temporal_range.is_some());
        let (start, end) = s.temporal_range.unwrap();
        let three_days_ms = 3 * 86_400_000i64;
        assert!((end - start - three_days_ms).abs() < 86_400_000);
    }

    #[test]
    fn test_temporal_last_saturday() {
        let s = analyze_query("who did I go with last saturday?");
        assert!(s.temporal_range.is_some());
    }

    #[test]
    fn test_temporal_two_weeks_ago() {
        let s = analyze_query("what was I doing two weeks ago?");
        assert!(s.temporal_range.is_some());
    }

    #[test]
    fn test_temporal_few_months() {
        let s = analyze_query("in the past few months what concerts did I attend?");
        assert!(s.temporal_range.is_some());
    }

    #[test]
    fn test_no_temporal() {
        let s = analyze_query("How does Arweave pricing work?");
        assert!(s.temporal_range.is_none());
    }

    #[test]
    fn test_undecided_question() {
        let s = analyze_query("How does Arweave pricing work?");
        assert_eq!(s.decision, RetrievalDecision::Undecided);
    }

    #[test]
    fn test_explicit_session() {
        let s = analyze_query("Show me session abc123");
        assert_eq!(s.explicit_session, Some("abc123".to_string()));
    }
}
