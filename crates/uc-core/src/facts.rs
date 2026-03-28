use chrono::Datelike;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: Uuid,
    /// The atomic fact as a sentence.
    pub content: String,
    /// Extracted entity (e.g., "user", "Project X").
    pub subject: String,
    /// Relation type (e.g., "lives_in", "prefers", "works_at").
    pub predicate: String,
    /// The value (e.g., "Austin", "Vim", "Google").
    pub object: String,
    /// FK to the chunks table.
    pub source_chunk_id: String,
    pub session_id: String,
    pub user_id: String,
    /// When the conversation happened (ms epoch).
    pub document_date: i64,
    /// When the fact became true (ms epoch, nullable).
    pub event_date: Option<i64>,
    /// `true` = current, `false` = superseded.
    pub valid: bool,
    /// Fact ID of the newer fact that replaced this one.
    pub superseded_by: Option<String>,
    /// 1.0 for explicit patterns, 0.7 for inferred.
    pub confidence: f32,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub facts: Vec<Fact>,
    pub entities: Vec<ExtractedEntity>,
}

#[derive(Debug, Clone)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: EntityType,
    /// Which fact it came from (fact content).
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Person,
    Place,
    Organization,
    Project,
    Tool,
    Concept,
    Unknown,
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Extract atomic facts from `text` using NLP-style regex-free patterns.
///
/// Each fact is a subject-predicate-object triple with optional temporal
/// metadata. The function splits text into sentences, matches each against
/// a library of first-person patterns, and returns the facts together with
/// any entities referenced by those facts.
pub fn extract_facts(
    text: &str,
    chunk_id: &str,
    session_id: &str,
    user_id: &str,
    document_date: i64,
) -> ExtractionResult {
    let sentences = split_sentences(text);
    let now = chrono::Utc::now().timestamp_millis();

    let mut facts: Vec<Fact> = Vec::new();
    let mut entities: Vec<ExtractedEntity> = Vec::new();

    for sentence in &sentences {
        let trimmed = sentence.trim();
        if trimmed.is_empty() || trimmed.len() < 5 {
            continue;
        }

        let extracted = match_patterns(trimmed);
        for (subject, predicate, object, confidence) in extracted {
            let event_date = extract_event_date(trimmed, document_date);
            let content = normalize_fact_sentence(&subject, &predicate, &object);

            let fact = Fact {
                id: Uuid::new_v4(),
                content: content.clone(),
                subject: subject.clone(),
                predicate: predicate.clone(),
                object: object.clone(),
                source_chunk_id: chunk_id.to_string(),
                session_id: session_id.to_string(),
                user_id: user_id.to_string(),
                document_date,
                event_date,
                valid: true,
                superseded_by: None,
                confidence,
                created_at: now,
            };

            // Derive entities from this fact.
            let ents = derive_entities(&fact);
            entities.extend(ents);

            facts.push(fact);
        }
    }

    // Deduplicate entities by (name, entity_type).
    entities.sort_by(|a, b| a.name.cmp(&b.name));
    entities.dedup_by(|a, b| {
        a.name.eq_ignore_ascii_case(&b.name) && a.entity_type == b.entity_type
    });

    ExtractionResult { facts, entities }
}

// ---------------------------------------------------------------------------
// Update-signal detection
// ---------------------------------------------------------------------------

/// Returns `true` if the text contains linguistic signals that a previously
/// stored fact may need updating (e.g., "I moved", "I switched", "actually").
pub fn is_update_signal(text: &str) -> bool {
    let lower = text.to_lowercase();
    let signals: &[&str] = &[
        "i moved",
        "i switched",
        "i changed",
        "actually,",
        "actually ",
        "correction:",
        "correction,",
        "i now ",
        "i'm now ",
        "im now ",
        "i no longer",
        "i don't anymore",
        "i stopped",
        "i quit",
        "i left ",
        "i used to ",
        "not anymore",
        "no longer ",
        "i've switched",
        "i've moved",
        "i've changed",
        "we migrated",
        "we switched",
        "we moved",
        "we changed",
        "update:",
        "fyi,",
        "fyi ",
    ];

    signals.iter().any(|s| lower.contains(s))
}

// ---------------------------------------------------------------------------
// Sentence splitter
// ---------------------------------------------------------------------------

/// Splits text into sentences on `.` `!` `?` and newline boundaries, keeping
/// non-empty trimmed fragments.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        match ch {
            '.' | '!' | '?' => {
                current.push(ch);
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    sentences.push(trimmed);
                }
                current.clear();
            }
            '\n' | '\r' => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    sentences.push(trimmed);
                }
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    sentences
}

// ---------------------------------------------------------------------------
// Pattern matching engine
// ---------------------------------------------------------------------------

/// Attempts all pattern families against `sentence` (case-insensitive).
/// Returns a vec of (subject, predicate, object, confidence) tuples.
fn match_patterns(sentence: &str) -> Vec<(String, String, String, f32)> {
    let mut results = Vec::new();
    let lower = sentence.to_lowercase();

    // Try each category. We collect at most one match per category to avoid
    // duplicates when a sentence triggers overlapping rules.
    if let Some(m) = match_preference(&lower, sentence) {
        results.push(m);
    }
    if let Some(m) = match_personal_info(&lower, sentence) {
        results.push(m);
    }
    if let Some(m) = match_projects_tools(&lower, sentence) {
        results.push(m);
    }
    if let Some(m) = match_temporal_statement(&lower, sentence) {
        results.push(m);
    }
    if let Some(m) = match_knowledge_update(&lower, sentence) {
        results.push(m);
    }

    results
}

// ---------------------------------------------------------------------------
// 1. Preferences
// ---------------------------------------------------------------------------

fn match_preference(lower: &str, original: &str) -> Option<(String, String, String, f32)> {
    // "I prefer X (over/to Y)"
    if let Some(obj) = strip_after(lower, "i prefer ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "prefers".into(), extract_original(original, &obj), 1.0));
        }
    }

    // "my favorite X is Y"
    if let Some(rest) = strip_after(lower, "my favorite ") {
        if let Some(pos) = rest.find(" is ") {
            let category = rest[..pos].trim().to_string();
            let value = rest[pos + 4..].trim().to_string();
            let value = trim_trailing_clause(&value);
            if valid_object(&value) {
                let predicate = format!("favorite_{}", category.replace(' ', "_"));
                return Some(("user".into(), predicate, extract_original(original, &value), 1.0));
            }
        }
    }
    if let Some(rest) = strip_after(lower, "my favourite ") {
        if let Some(pos) = rest.find(" is ") {
            let category = rest[..pos].trim().to_string();
            let value = rest[pos + 4..].trim().to_string();
            let value = trim_trailing_clause(&value);
            if valid_object(&value) {
                let predicate = format!("favorite_{}", category.replace(' ', "_"));
                return Some(("user".into(), predicate, extract_original(original, &value), 1.0));
            }
        }
    }

    // "I like/love/enjoy X"
    for verb in &["like", "love", "enjoy"] {
        let prefix = format!("i {verb} ");
        if let Some(obj) = strip_after(lower, &prefix) {
            let obj = trim_trailing_clause(&obj);
            if valid_object(&obj) {
                return Some(("user".into(), format!("{verb}s"), extract_original(original, &obj), 1.0));
            }
        }
    }

    // "I use X"
    if let Some(obj) = strip_after(lower, "i use ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "uses".into(), extract_original(original, &obj), 1.0));
        }
    }

    // "I don't like X" / "I hate X" / "I dislike X"
    for (pattern, predicate) in &[
        ("i don't like ", "dislikes"),
        ("i dont like ", "dislikes"),
        ("i don't enjoy ", "dislikes"),
        ("i hate ", "dislikes"),
        ("i dislike ", "dislikes"),
        ("i can't stand ", "dislikes"),
    ] {
        if let Some(obj) = strip_after(lower, pattern) {
            let obj = trim_trailing_clause(&obj);
            if valid_object(&obj) {
                return Some(("user".into(), predicate.to_string(), extract_original(original, &obj), 1.0));
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// 2. Personal information
// ---------------------------------------------------------------------------

fn match_personal_info(lower: &str, original: &str) -> Option<(String, String, String, f32)> {
    // "my name is X"
    if let Some(obj) = strip_after(lower, "my name is ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "name_is".into(), extract_original(original, &obj), 1.0));
        }
    }

    // "I'm X at Y" / "I am X at Y" (role at org)
    for prefix in &["i'm a ", "i am a ", "i'm an ", "i am an "] {
        if let Some(rest) = strip_after(lower, prefix) {
            if let Some(at_pos) = find_word(rest.as_str(), " at ") {
                let role = rest[..at_pos].trim().to_string();
                let org = rest[at_pos + 4..].trim().to_string();
                let org = trim_trailing_clause(&org);
                if valid_object(&role) && valid_object(&org) {
                    return Some((
                        "user".into(),
                        "role_at".into(),
                        format!("{} at {}", extract_original(original, &role), extract_original(original, &org)),
                        1.0,
                    ));
                }
            }
        }
    }

    // "I'm a/an X" (role/title, but only if followed by a role-like word)
    for prefix in &["i'm a ", "i am a ", "i'm an ", "i am an "] {
        if let Some(rest) = strip_after(lower, prefix) {
            let obj = trim_trailing_clause(&rest);
            if valid_object(&obj) && looks_like_role(&obj) {
                return Some(("user".into(), "role_is".into(), extract_original(original, &obj), 0.7));
            }
        }
    }

    // "I live in X"
    if let Some(obj) = strip_after(lower, "i live in ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "lives_in".into(), extract_original(original, &obj), 1.0));
        }
    }

    // "I'm from X"
    for prefix in &["i'm from ", "i am from ", "im from "] {
        if let Some(obj) = strip_after(lower, prefix) {
            let obj = trim_trailing_clause(&obj);
            if valid_object(&obj) {
                return Some(("user".into(), "from".into(), extract_original(original, &obj), 1.0));
            }
        }
    }

    // "I work at/for X"
    for prefix in &["i work at ", "i work for "] {
        if let Some(obj) = strip_after(lower, prefix) {
            let obj = trim_trailing_clause(&obj);
            if valid_object(&obj) {
                return Some(("user".into(), "works_at".into(), extract_original(original, &obj), 1.0));
            }
        }
    }

    // "I speak X" (language)
    if let Some(obj) = strip_after(lower, "i speak ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "speaks".into(), extract_original(original, &obj), 0.7));
        }
    }

    // "I'm X years old" / "I am X years old"
    for prefix in &["i'm ", "i am ", "im "] {
        if let Some(rest) = strip_after(lower, prefix) {
            if rest.contains("years old") || rest.contains("year old") {
                let age_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !age_str.is_empty() {
                    return Some(("user".into(), "age_is".into(), age_str, 1.0));
                }
            }
        }
    }

    // "I have X" (when followed by concrete-ish nouns, low confidence)
    if let Some(obj) = strip_after(lower, "i have a ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "has".into(), extract_original(original, &obj), 0.7));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// 3. Projects & tools
// ---------------------------------------------------------------------------

fn match_projects_tools(lower: &str, original: &str) -> Option<(String, String, String, f32)> {
    // "I'm working on X" / "I am working on X"
    for prefix in &["i'm working on ", "i am working on ", "im working on "] {
        if let Some(obj) = strip_after(lower, prefix) {
            let obj = trim_trailing_clause(&obj);
            if valid_object(&obj) {
                return Some(("user".into(), "working_on".into(), extract_original(original, &obj), 1.0));
            }
        }
    }

    // "I'm building X" / "we're building X"
    for prefix in &["i'm building ", "i am building ", "we're building ", "we are building "] {
        if let Some(obj) = strip_after(lower, prefix) {
            let obj = trim_trailing_clause(&obj);
            if valid_object(&obj) {
                return Some(("user".into(), "building".into(), extract_original(original, &obj), 1.0));
            }
        }
    }

    // "the project is called X" / "the project is X"
    if let Some(obj) = strip_after(lower, "the project is called ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "project_name".into(), extract_original(original, &obj), 1.0));
        }
    }
    if let Some(obj) = strip_after(lower, "the project is ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "project_name".into(), extract_original(original, &obj), 0.7));
        }
    }

    // "we use X for Y"
    if let Some(rest) = strip_after(lower, "we use ") {
        if let Some(for_pos) = find_word(rest.as_str(), " for ") {
            let tool = rest[..for_pos].trim().to_string();
            let purpose = rest[for_pos + 5..].trim().to_string();
            let purpose = trim_trailing_clause(&purpose);
            if valid_object(&tool) && valid_object(&purpose) {
                return Some((
                    extract_original(original, &tool),
                    "used_for".into(),
                    extract_original(original, &purpose),
                    1.0,
                ));
            }
        }
        // Fallback: "we use X"
        let obj = trim_trailing_clause(&rest);
        if valid_object(&obj) {
            return Some(("user".into(), "team_uses".into(), extract_original(original, &obj), 0.7));
        }
    }

    // "our stack includes X"
    if let Some(obj) = strip_after(lower, "our stack includes ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "stack_includes".into(), extract_original(original, &obj), 1.0));
        }
    }

    // "our tech stack is X" / "our stack is X"
    for prefix in &["our tech stack is ", "our stack is "] {
        if let Some(obj) = strip_after(lower, prefix) {
            let obj = trim_trailing_clause(&obj);
            if valid_object(&obj) {
                return Some(("user".into(), "stack_includes".into(), extract_original(original, &obj), 0.7));
            }
        }
    }

    // "I'm learning X"
    for prefix in &["i'm learning ", "i am learning ", "im learning "] {
        if let Some(obj) = strip_after(lower, prefix) {
            let obj = trim_trailing_clause(&obj);
            if valid_object(&obj) {
                return Some(("user".into(), "learning".into(), extract_original(original, &obj), 1.0));
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// 4. Temporal statements
// ---------------------------------------------------------------------------

fn match_temporal_statement(lower: &str, original: &str) -> Option<(String, String, String, f32)> {
    // "I started X on Y" / "I started X in Y"
    for prefix in &["i started ", "i began "] {
        if let Some(rest) = strip_after(lower, prefix) {
            for delim in &[" on ", " in "] {
                if let Some(pos) = find_word(rest.as_str(), delim) {
                    let activity = rest[..pos].trim().to_string();
                    let _when = rest[pos + delim.len()..].trim().to_string();
                    if valid_object(&activity) {
                        return Some(("user".into(), "started".into(), extract_original(original, &activity), 1.0));
                    }
                }
            }
            // No date part: "I started learning Rust"
            let obj = trim_trailing_clause(&rest);
            if valid_object(&obj) {
                return Some(("user".into(), "started".into(), extract_original(original, &obj), 0.7));
            }
        }
    }

    // "since X" at start of clause
    if lower.starts_with("since ") || lower.contains(", since ") || lower.contains(" since ") {
        // This is a weak signal; mark as low confidence
        // We don't extract a triple here but the temporal date extractor
        // will pick up the date portion if present.
    }

    // "I moved to X"
    if let Some(obj) = strip_after(lower, "i moved to ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "moved_to".into(), extract_original(original, &obj), 1.0));
        }
    }

    // "I switched to X" / "I switched from X to Y"
    if let Some(rest) = strip_after(lower, "i switched to ") {
        let obj = trim_trailing_clause(&rest);
        if valid_object(&obj) {
            return Some(("user".into(), "switched_to".into(), extract_original(original, &obj), 1.0));
        }
    }
    if let Some(rest) = strip_after(lower, "i switched from ") {
        if let Some(to_pos) = find_word(rest.as_str(), " to ") {
            let _from = rest[..to_pos].trim().to_string();
            let to = rest[to_pos + 4..].trim().to_string();
            let to = trim_trailing_clause(&to);
            if valid_object(&to) {
                return Some(("user".into(), "switched_to".into(), extract_original(original, &to), 1.0));
            }
        }
    }

    // "I changed X to Y"
    if let Some(rest) = strip_after(lower, "i changed ") {
        if let Some(to_pos) = find_word(rest.as_str(), " to ") {
            let what = rest[..to_pos].trim().to_string();
            let new_val = rest[to_pos + 4..].trim().to_string();
            let new_val = trim_trailing_clause(&new_val);
            if valid_object(&what) && valid_object(&new_val) {
                return Some((
                    extract_original(original, &what),
                    "changed_to".into(),
                    extract_original(original, &new_val),
                    1.0,
                ));
            }
        }
    }

    // "I joined X"
    if let Some(obj) = strip_after(lower, "i joined ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "joined".into(), extract_original(original, &obj), 1.0));
        }
    }

    // "I left X"
    if let Some(obj) = strip_after(lower, "i left ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "left".into(), extract_original(original, &obj), 1.0));
        }
    }

    // "I graduated from X"
    if let Some(obj) = strip_after(lower, "i graduated from ") {
        let obj = trim_trailing_clause(&obj);
        if valid_object(&obj) {
            return Some(("user".into(), "graduated_from".into(), extract_original(original, &obj), 1.0));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// 5. Knowledge updates (contradiction signals)
// ---------------------------------------------------------------------------

fn match_knowledge_update(lower: &str, _original: &str) -> Option<(String, String, String, f32)> {
    // "actually, X" — the rest of the sentence is the corrected fact.
    // We extract the entire correction as the object; downstream
    // contradiction resolution can match it against existing facts.
    if let Some(rest) = strip_after(lower, "actually, ") {
        let obj = trim_trailing_clause(&rest);
        if obj.len() >= 5 {
            return Some(("user".into(), "corrects".into(), obj, 0.7));
        }
    }
    if let Some(rest) = strip_after(lower, "actually ") {
        // Only match when "actually" is at the very start (avoid mid-sentence).
        if lower.starts_with("actually ") {
            let obj = trim_trailing_clause(&rest);
            if obj.len() >= 5 {
                return Some(("user".into(), "corrects".into(), obj, 0.7));
            }
        }
    }

    // "correction: X"
    if let Some(rest) = strip_after(lower, "correction: ") {
        let obj = trim_trailing_clause(&rest);
        if obj.len() >= 5 {
            return Some(("user".into(), "corrects".into(), obj, 0.7));
        }
    }

    // "I now X" — signals a state change.
    if let Some(rest) = strip_after(lower, "i now ") {
        let obj = trim_trailing_clause(&rest);
        if valid_object(&obj) {
            return Some(("user".into(), "now".into(), obj, 0.7));
        }
    }

    // "I'm now X"
    for prefix in &["i'm now ", "im now ", "i am now "] {
        if let Some(rest) = strip_after(lower, prefix) {
            let obj = trim_trailing_clause(&rest);
            if valid_object(&obj) {
                return Some(("user".into(), "now_is".into(), obj, 0.7));
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Event-date extraction
// ---------------------------------------------------------------------------

/// Attempts to extract a temporal reference from text and convert it to an
/// epoch-millisecond timestamp. Returns `None` if no date-like pattern is
/// found.
pub fn extract_event_date(text: &str, reference_time_ms: i64) -> Option<i64> {
    let lower = text.to_lowercase();
    let day_ms: i64 = 86_400_000;
    let week_ms: i64 = 7 * day_ms;

    // --- ISO dates: YYYY-MM-DD ---
    // Look for a 4-digit year followed by -MM-DD.
    if let Some(date) = find_iso_date(&lower) {
        return Some(date);
    }

    // --- Named month + day: "January 15", "March 2024" ---
    if let Some(date) = find_named_date(&lower) {
        return Some(date);
    }

    // --- Relative patterns ---
    if lower.contains("yesterday") {
        return Some(reference_time_ms - day_ms);
    }
    if lower.contains("today") {
        return Some(reference_time_ms);
    }
    if lower.contains("last week") {
        return Some(reference_time_ms - week_ms);
    }
    if lower.contains("last month") {
        return Some(reference_time_ms - 30 * day_ms);
    }
    if lower.contains("last year") {
        return Some(reference_time_ms - 365 * day_ms);
    }

    // "N days/weeks/months ago"
    if let Some(ms) = parse_relative_ago(&lower, day_ms, week_ms, reference_time_ms) {
        return Some(ms);
    }

    // Named day of week: "last Tuesday"
    let days_of_week = [
        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
    ];
    for day in &days_of_week {
        if lower.contains(&format!("last {day}")) {
            // Approximate: 1-7 days ago
            return Some(reference_time_ms - 4 * day_ms);
        }
    }

    None
}

/// Searches for an ISO-8601 date (YYYY-MM-DD) and converts to epoch ms.
fn find_iso_date(text: &str) -> Option<i64> {
    // Walk through looking for a 4-digit year prefix.
    let bytes = text.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    for i in 0..=bytes.len().saturating_sub(10) {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
            && bytes[i + 4] == b'-'
            && bytes[i + 5].is_ascii_digit()
            && bytes[i + 6].is_ascii_digit()
            && bytes[i + 7] == b'-'
            && bytes[i + 8].is_ascii_digit()
            && bytes[i + 9].is_ascii_digit()
        {
            let fragment = &text[i..i + 10];
            if let Ok(nd) = chrono::NaiveDate::parse_from_str(fragment, "%Y-%m-%d") {
                let dt = nd.and_hms_opt(0, 0, 0)?;
                return Some(dt.and_utc().timestamp_millis());
            }
        }
    }
    None
}

/// Matches "January 15", "Jan 15", "March 2024", etc.
fn find_named_date(text: &str) -> Option<i64> {
    let months: &[(&[&str], u32)] = &[
        (&["january", "jan"], 1),
        (&["february", "feb"], 2),
        (&["march", "mar"], 3),
        (&["april", "apr"], 4),
        (&["may"], 5),
        (&["june", "jun"], 6),
        (&["july", "jul"], 7),
        (&["august", "aug"], 8),
        (&["september", "sep", "sept"], 9),
        (&["october", "oct"], 10),
        (&["november", "nov"], 11),
        (&["december", "dec"], 12),
    ];

    let words: Vec<&str> = text.split_whitespace().collect();

    for (i, word) in words.iter().enumerate() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
        for (names, month_num) in months {
            if names.iter().any(|n| *n == clean) {
                // Check the next word for a day or year.
                if i + 1 < words.len() {
                    let next = words[i + 1].trim_matches(|c: char| !c.is_ascii_digit());
                    if let Ok(num) = next.parse::<i32>() {
                        if num >= 1 && num <= 31 {
                            // "January 15" — assume current year.
                            let year = chrono::Utc::now().date_naive().year();
                            if let Some(nd) = chrono::NaiveDate::from_ymd_opt(year, *month_num, num as u32) {
                                let dt = nd.and_hms_opt(0, 0, 0)?;
                                return Some(dt.and_utc().timestamp_millis());
                            }
                        } else if num >= 2000 && num <= 2100 {
                            // "March 2024" — first of that month.
                            if let Some(nd) = chrono::NaiveDate::from_ymd_opt(num, *month_num, 1) {
                                let dt = nd.and_hms_opt(0, 0, 0)?;
                                return Some(dt.and_utc().timestamp_millis());
                            }
                        }
                    }
                }
                // Also check previous word for a day number: "15 January"
                if i > 0 {
                    let prev = words[i - 1].trim_matches(|c: char| !c.is_ascii_digit());
                    if let Ok(day) = prev.parse::<i32>() {
                        if day >= 1 && day <= 31 {
                            let year = chrono::Utc::now().date_naive().year();
                            if let Some(nd) = chrono::NaiveDate::from_ymd_opt(year, *month_num, day as u32) {
                                let dt = nd.and_hms_opt(0, 0, 0)?;
                                return Some(dt.and_utc().timestamp_millis());
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Parses "N days ago", "2 weeks ago", "3 months ago".
fn parse_relative_ago(text: &str, day_ms: i64, week_ms: i64, now_ms: i64) -> Option<i64> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        if *w == "ago" && i >= 2 {
            let unit_word = words[i - 1].trim_end_matches('s');
            let num_word = words[i - 2];

            let n: i64 = num_word.parse().ok().or_else(|| {
                match num_word {
                    "a" | "one" => Some(1),
                    "two" => Some(2),
                    "three" => Some(3),
                    "four" => Some(4),
                    "five" => Some(5),
                    "six" => Some(6),
                    "seven" => Some(7),
                    "eight" => Some(8),
                    "nine" => Some(9),
                    "ten" => Some(10),
                    "couple" => Some(2),
                    "few" => Some(3),
                    "several" => Some(5),
                    _ => None,
                }
            })?;

            let unit_ms = match unit_word {
                "day" => day_ms,
                "week" => week_ms,
                "month" => 30 * day_ms,
                "year" => 365 * day_ms,
                _ => return None,
            };

            return Some(now_ms - n * unit_ms);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Entity derivation
// ---------------------------------------------------------------------------

/// Derive entities from a fact based on its predicate and object.
fn derive_entities(fact: &Fact) -> Vec<ExtractedEntity> {
    let mut entities = Vec::new();
    let pred = fact.predicate.as_str();
    let obj = &fact.object;

    let (name, etype) = match pred {
        "name_is" => (obj.clone(), EntityType::Person),
        "lives_in" | "from" | "moved_to" => (obj.clone(), EntityType::Place),
        "works_at" | "joined" | "left" | "graduated_from" => (obj.clone(), EntityType::Organization),
        "working_on" | "building" | "project_name" => (obj.clone(), EntityType::Project),
        "uses" | "switched_to" | "stack_includes" | "team_uses" | "learning" => {
            (obj.clone(), EntityType::Tool)
        }
        "role_at" => {
            // "engineer at Google" — the org part is after " at ".
            if let Some(at_pos) = obj.find(" at ") {
                let org = obj[at_pos + 4..].trim().to_string();
                entities.push(ExtractedEntity {
                    name: org,
                    entity_type: EntityType::Organization,
                    source: fact.content.clone(),
                });
            }
            (obj.clone(), EntityType::Concept)
        }
        _ => (obj.clone(), EntityType::Unknown),
    };

    if !name.is_empty() {
        entities.push(ExtractedEntity {
            name,
            entity_type: etype,
            source: fact.content.clone(),
        });
    }

    entities
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// If `text` starts with `prefix` (case-insensitive match already done by
/// caller), return the remainder.
fn strip_after(lower: &str, prefix: &str) -> Option<String> {
    if lower.starts_with(prefix) {
        Some(lower[prefix.len()..].trim().to_string())
    } else {
        None
    }
}

/// Find a delimiter word that stands on its own (surrounded by spaces or at
/// boundaries). Returns byte offset into `text`.
fn find_word(text: &str, word: &str) -> Option<usize> {
    text.find(word)
}

/// Remove trailing clauses starting with common conjunctions/prepositions so
/// we don't swallow half the sentence as the object.
fn trim_trailing_clause(s: &str) -> String {
    let s = s.trim_end_matches(|c: char| c == '.' || c == '!' || c == '?' || c == ',');
    let cut_words = [
        " because ", " since ", " so ", " but ", " although ",
        " however ", " and i ", " and we ", " which ", " that i ",
        " when i ", " where i ", " over ", " for ", " last ",
        " before ", " after ", " during ", " until ", " from ",
    ];
    let lower = s.to_lowercase();
    let mut end = s.len();
    for cw in &cut_words {
        if let Some(pos) = lower.find(cw) {
            if pos > 0 && pos < end {
                end = pos;
            }
        }
    }
    s[..end].trim().to_string()
}

/// Given the lowercase object match, try to extract the original-case version
/// from `original`. Falls back to the lowercase version.
fn extract_original(original: &str, lower_obj: &str) -> String {
    let orig_lower = original.to_lowercase();
    if let Some(pos) = orig_lower.find(lower_obj) {
        original[pos..pos + lower_obj.len()].to_string()
    } else {
        lower_obj.to_string()
    }
}

/// Checks whether the extracted object is long enough to be meaningful and
/// not just a stop word.
fn valid_object(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.len() < 2 {
        return false;
    }
    // Reject if it's only stop words.
    let stop = [
        "a", "an", "the", "it", "this", "that", "is", "are", "was", "were",
        "be", "been", "being", "to", "of", "in", "on", "at", "by", "for",
        "with", "about", "as", "so", "or", "if", "my", "your", "our",
    ];
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.iter().all(|w| stop.contains(&w.to_lowercase().as_str())) {
        return false;
    }
    true
}

/// Heuristic: does the string look like a job title / role?
fn looks_like_role(s: &str) -> bool {
    let role_words = [
        "developer", "engineer", "designer", "manager", "director", "architect",
        "analyst", "scientist", "researcher", "consultant", "founder", "ceo",
        "cto", "cfo", "intern", "student", "teacher", "professor", "writer",
        "artist", "musician", "photographer", "freelancer", "contractor",
        "programmer", "admin", "administrator", "lead", "vp", "president",
        "specialist", "coordinator", "therapist", "doctor", "nurse", "lawyer",
        "chef", "pilot", "operator", "technician", "coach",
    ];
    let lower = s.to_lowercase();
    role_words.iter().any(|r| lower.contains(r))
}

/// Build a normalized fact sentence from components.
fn normalize_fact_sentence(subject: &str, predicate: &str, object: &str) -> String {
    format!("{} {} {}", subject, predicate.replace('_', " "), object)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helpers
    fn extract(text: &str) -> ExtractionResult {
        extract_facts(text, "chunk_1", "session_1", "user_1", 1_700_000_000_000)
    }

    fn first_fact(text: &str) -> Fact {
        let r = extract(text);
        assert!(
            !r.facts.is_empty(),
            "expected at least one fact from: {text}"
        );
        r.facts.into_iter().next().unwrap()
    }

    fn no_facts(text: &str) {
        let r = extract(text);
        assert!(
            r.facts.is_empty(),
            "expected no facts from: {text}, got: {:?}",
            r.facts.iter().map(|f| &f.content).collect::<Vec<_>>()
        );
    }

    // -----------------------------------------------------------------------
    // Preferences
    // -----------------------------------------------------------------------

    #[test]
    fn test_prefer() {
        let f = first_fact("I prefer Vim over Emacs.");
        assert_eq!(f.predicate, "prefers");
        assert_eq!(f.object, "Vim");
        assert_eq!(f.confidence, 1.0);
    }

    #[test]
    fn test_favorite() {
        let f = first_fact("My favorite language is Rust.");
        assert_eq!(f.predicate, "favorite_language");
        assert_eq!(f.object, "Rust");
    }

    #[test]
    fn test_favourite_british() {
        let f = first_fact("My favourite editor is Neovim.");
        assert_eq!(f.predicate, "favorite_editor");
        assert_eq!(f.object, "Neovim");
    }

    #[test]
    fn test_like() {
        let f = first_fact("I like hiking.");
        assert_eq!(f.predicate, "likes");
        assert_eq!(f.object, "hiking");
    }

    #[test]
    fn test_love() {
        let f = first_fact("I love coffee.");
        assert_eq!(f.predicate, "loves");
        assert_eq!(f.object, "coffee");
    }

    #[test]
    fn test_enjoy() {
        let f = first_fact("I enjoy reading science fiction.");
        assert_eq!(f.predicate, "enjoys");
        assert_eq!(f.object, "reading science fiction");
    }

    #[test]
    fn test_use() {
        let f = first_fact("I use TypeScript for most of my projects.");
        assert_eq!(f.predicate, "uses");
        assert_eq!(f.object, "TypeScript");
    }

    #[test]
    fn test_dont_like() {
        let f = first_fact("I don't like Java.");
        assert_eq!(f.predicate, "dislikes");
        assert_eq!(f.object, "Java");
    }

    #[test]
    fn test_hate() {
        let f = first_fact("I hate meetings.");
        assert_eq!(f.predicate, "dislikes");
        assert_eq!(f.object, "meetings");
    }

    // -----------------------------------------------------------------------
    // Personal info
    // -----------------------------------------------------------------------

    #[test]
    fn test_name_is() {
        let f = first_fact("My name is Alice.");
        assert_eq!(f.predicate, "name_is");
        assert_eq!(f.object, "Alice");
    }

    #[test]
    fn test_live_in() {
        let f = first_fact("I live in Austin.");
        assert_eq!(f.predicate, "lives_in");
        assert_eq!(f.object, "Austin");
    }

    #[test]
    fn test_from() {
        let f = first_fact("I'm from Portland.");
        assert_eq!(f.predicate, "from");
        assert_eq!(f.object, "Portland");
    }

    #[test]
    fn test_work_at() {
        let f = first_fact("I work at Google.");
        assert_eq!(f.predicate, "works_at");
        assert_eq!(f.object, "Google");
    }

    #[test]
    fn test_work_for() {
        let f = first_fact("I work for a startup called Memoryport.");
        assert_eq!(f.predicate, "works_at");
        // Should capture up to "a startup called Memoryport"
        assert!(f.object.contains("Memoryport"));
    }

    #[test]
    fn test_role_at_org() {
        let f = first_fact("I'm a software engineer at Anthropic.");
        assert_eq!(f.predicate, "role_at");
        assert!(f.object.contains("software engineer"));
        assert!(f.object.contains("Anthropic"));
    }

    #[test]
    fn test_role_only() {
        let f = first_fact("I'm a developer.");
        assert_eq!(f.predicate, "role_is");
        assert_eq!(f.object, "developer");
        assert_eq!(f.confidence, 0.7);
    }

    #[test]
    fn test_age() {
        let f = first_fact("I'm 30 years old.");
        assert_eq!(f.predicate, "age_is");
        assert_eq!(f.object, "30");
    }

    #[test]
    fn test_speak() {
        let f = first_fact("I speak Spanish.");
        assert_eq!(f.predicate, "speaks");
        assert_eq!(f.object, "Spanish");
    }

    // -----------------------------------------------------------------------
    // Projects & tools
    // -----------------------------------------------------------------------

    #[test]
    fn test_working_on() {
        let f = first_fact("I'm working on a CLI tool.");
        assert_eq!(f.predicate, "working_on");
        assert_eq!(f.object, "a CLI tool");
    }

    #[test]
    fn test_building() {
        let f = first_fact("We're building a memory system.");
        assert_eq!(f.predicate, "building");
        assert_eq!(f.object, "a memory system");
    }

    #[test]
    fn test_project_called() {
        let f = first_fact("The project is called Memoryport.");
        assert_eq!(f.predicate, "project_name");
        assert_eq!(f.object, "Memoryport");
    }

    #[test]
    fn test_we_use_for() {
        let f = first_fact("We use LanceDB for vector search.");
        assert_eq!(f.subject, "LanceDB");
        assert_eq!(f.predicate, "used_for");
        assert_eq!(f.object, "vector search");
    }

    #[test]
    fn test_stack_includes() {
        let f = first_fact("Our stack includes React and Tailwind.");
        assert_eq!(f.predicate, "stack_includes");
        assert!(f.object.contains("React"));
    }

    #[test]
    fn test_learning() {
        let f = first_fact("I'm learning Rust.");
        assert_eq!(f.predicate, "learning");
        assert_eq!(f.object, "Rust");
    }

    // -----------------------------------------------------------------------
    // Temporal statements
    // -----------------------------------------------------------------------

    #[test]
    fn test_started() {
        let f = first_fact("I started learning Rust in January.");
        assert_eq!(f.predicate, "started");
        assert_eq!(f.object, "learning Rust");
    }

    #[test]
    fn test_moved_to() {
        let f = first_fact("I moved to San Francisco.");
        assert_eq!(f.predicate, "moved_to");
        assert_eq!(f.object, "San Francisco");
    }

    #[test]
    fn test_switched_to() {
        let f = first_fact("I switched to NeoVim.");
        assert_eq!(f.predicate, "switched_to");
        assert_eq!(f.object, "NeoVim");
    }

    #[test]
    fn test_switched_from_to() {
        let f = first_fact("I switched from VS Code to NeoVim.");
        assert_eq!(f.predicate, "switched_to");
        assert_eq!(f.object, "NeoVim");
    }

    #[test]
    fn test_changed_to() {
        let f = first_fact("I changed my editor to Helix.");
        assert_eq!(f.subject, "my editor");
        assert_eq!(f.predicate, "changed_to");
        assert_eq!(f.object, "Helix");
    }

    #[test]
    fn test_joined() {
        let f = first_fact("I joined Anthropic.");
        assert_eq!(f.predicate, "joined");
        assert_eq!(f.object, "Anthropic");
    }

    #[test]
    fn test_left() {
        let f = first_fact("I left Meta last month.");
        assert_eq!(f.predicate, "left");
        assert_eq!(f.object, "Meta");
    }

    #[test]
    fn test_graduated_from() {
        let f = first_fact("I graduated from MIT.");
        assert_eq!(f.predicate, "graduated_from");
        assert_eq!(f.object, "MIT");
    }

    // -----------------------------------------------------------------------
    // Knowledge updates
    // -----------------------------------------------------------------------

    #[test]
    fn test_actually_correction() {
        let f = first_fact("Actually, I live in Denver now.");
        assert_eq!(f.predicate, "corrects");
        assert!(f.object.contains("denver"));
        assert_eq!(f.confidence, 0.7);
    }

    #[test]
    fn test_correction_prefix() {
        let f = first_fact("Correction: my name is Bob.");
        assert_eq!(f.predicate, "corrects");
        assert!(f.object.contains("my name is bob"));
    }

    #[test]
    fn test_i_now() {
        let f = first_fact("I now use Rust for everything.");
        assert_eq!(f.predicate, "now");
        assert!(f.object.contains("use rust"));
    }

    #[test]
    fn test_im_now() {
        let f = first_fact("I'm now a tech lead.");
        assert_eq!(f.predicate, "now_is");
        assert!(f.object.contains("tech lead"));
    }

    // -----------------------------------------------------------------------
    // is_update_signal
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_signal_moved() {
        assert!(is_update_signal("I moved to Denver."));
    }

    #[test]
    fn test_update_signal_switched() {
        assert!(is_update_signal("I switched to Neovim."));
    }

    #[test]
    fn test_update_signal_actually() {
        assert!(is_update_signal("Actually, I prefer Go now."));
    }

    #[test]
    fn test_update_signal_correction() {
        assert!(is_update_signal("Correction: it's Python 3.12."));
    }

    #[test]
    fn test_update_signal_im_now() {
        assert!(is_update_signal("I'm now a senior engineer."));
    }

    #[test]
    fn test_update_signal_no_longer() {
        assert!(is_update_signal("I no longer use Java."));
    }

    #[test]
    fn test_no_update_signal() {
        assert!(!is_update_signal("I like programming."));
    }

    // -----------------------------------------------------------------------
    // Event-date extraction
    // -----------------------------------------------------------------------

    #[test]
    fn test_iso_date() {
        let date = extract_event_date("I started on 2024-03-15.", 0);
        assert!(date.is_some());
        // 2024-03-15 00:00:00 UTC
        let nd = chrono::NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let expected = nd.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        assert_eq!(date.unwrap(), expected);
    }

    #[test]
    fn test_named_date_month_day() {
        let date = extract_event_date("I joined on January 15.", 0);
        assert!(date.is_some());
    }

    #[test]
    fn test_named_date_month_year() {
        let date = extract_event_date("I moved in March 2024.", 0);
        assert!(date.is_some());
        let nd = chrono::NaiveDate::from_ymd_opt(2024, 3, 1).unwrap();
        let expected = nd.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        assert_eq!(date.unwrap(), expected);
    }

    #[test]
    fn test_yesterday() {
        let now = 1_700_000_000_000i64;
        let date = extract_event_date("I did this yesterday.", now);
        assert_eq!(date, Some(now - 86_400_000));
    }

    #[test]
    fn test_days_ago() {
        let now = 1_700_000_000_000i64;
        let date = extract_event_date("about 3 days ago", now);
        assert_eq!(date, Some(now - 3 * 86_400_000));
    }

    #[test]
    fn test_weeks_ago() {
        let now = 1_700_000_000_000i64;
        let date = extract_event_date("two weeks ago I changed jobs", now);
        assert_eq!(date, Some(now - 2 * 7 * 86_400_000));
    }

    #[test]
    fn test_last_tuesday() {
        let now = 1_700_000_000_000i64;
        let date = extract_event_date("I met them last tuesday.", now);
        assert!(date.is_some());
        // Should be within ~7 days of now
        assert!(now - date.unwrap() <= 7 * 86_400_000);
    }

    #[test]
    fn test_no_date() {
        let date = extract_event_date("I like coffee.", 0);
        assert!(date.is_none());
    }

    // -----------------------------------------------------------------------
    // Entity derivation
    // -----------------------------------------------------------------------

    #[test]
    fn test_entities_from_lives_in() {
        let r = extract("I live in Austin.");
        let place = r.entities.iter().find(|e| e.entity_type == EntityType::Place);
        assert!(place.is_some());
        assert_eq!(place.unwrap().name, "Austin");
    }

    #[test]
    fn test_entities_from_works_at() {
        let r = extract("I work at Google.");
        let org = r.entities.iter().find(|e| e.entity_type == EntityType::Organization);
        assert!(org.is_some());
        assert_eq!(org.unwrap().name, "Google");
    }

    #[test]
    fn test_entities_from_project() {
        let r = extract("The project is called Memoryport.");
        let proj = r.entities.iter().find(|e| e.entity_type == EntityType::Project);
        assert!(proj.is_some());
        assert_eq!(proj.unwrap().name, "Memoryport");
    }

    #[test]
    fn test_entities_from_tool() {
        let r = extract("I use TypeScript for most projects.");
        let tool = r.entities.iter().find(|e| e.entity_type == EntityType::Tool);
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name, "TypeScript");
    }

    #[test]
    fn test_entities_from_role_at() {
        let r = extract("I'm a software engineer at Anthropic.");
        let org = r.entities.iter().find(|e| e.entity_type == EntityType::Organization);
        assert!(org.is_some());
        assert_eq!(org.unwrap().name, "Anthropic");
    }

    // -----------------------------------------------------------------------
    // Multi-sentence / edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_multiple_sentences() {
        let r = extract("I live in Austin. I work at Google. I prefer Vim.");
        assert_eq!(r.facts.len(), 3);
    }

    #[test]
    fn test_newline_separated() {
        let r = extract("I live in Austin\nI work at Google\nI prefer Vim");
        assert_eq!(r.facts.len(), 3);
    }

    #[test]
    fn test_empty_text() {
        let r = extract("");
        assert!(r.facts.is_empty());
        assert!(r.entities.is_empty());
    }

    #[test]
    fn test_no_match() {
        no_facts("The weather is nice today.");
    }

    #[test]
    fn test_short_garbage() {
        no_facts("hi");
    }

    #[test]
    fn test_case_insensitive() {
        let f = first_fact("MY NAME IS ALICE.");
        assert_eq!(f.predicate, "name_is");
        assert_eq!(f.object, "ALICE");
    }

    #[test]
    fn test_trailing_clause_trimmed() {
        let f = first_fact("I use Vim because it's fast.");
        assert_eq!(f.object, "Vim");
    }

    #[test]
    fn test_fact_metadata() {
        let f = first_fact("I live in Austin.");
        assert_eq!(f.source_chunk_id, "chunk_1");
        assert_eq!(f.session_id, "session_1");
        assert_eq!(f.user_id, "user_1");
        assert_eq!(f.document_date, 1_700_000_000_000);
        assert!(f.valid);
        assert!(f.superseded_by.is_none());
    }

    #[test]
    fn test_fact_ids_unique() {
        let r = extract("I live in Austin. I work at Google.");
        let ids: Vec<Uuid> = r.facts.iter().map(|f| f.id).collect();
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn test_original_case_preserved_in_object() {
        let f = first_fact("I work at OpenAI.");
        assert_eq!(f.object, "OpenAI");
    }

    #[test]
    fn test_dont_match_stopword_objects() {
        // "I like the" should not produce a fact with object "the".
        no_facts("I like the.");
    }

    #[test]
    fn test_event_date_populated() {
        let f = first_fact("I started the job on 2024-01-15.");
        assert!(f.event_date.is_some());
    }
}
