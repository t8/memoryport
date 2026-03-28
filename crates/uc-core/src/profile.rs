use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Maximum number of dynamic facts retained in the profile.
const MAX_DYNAMIC_FACTS: usize = 20;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A subject-predicate-object fact extracted from conversation content.
/// Defined here because no upstream `facts` module exists yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    /// Optional category hint (e.g. "editor", "language", "os").
    pub object_category: Option<String>,
    pub timestamp: i64,
}

/// Pre-computed user profile containing static and dynamic facts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    /// Rarely-changing facts: name, role, preferences, location, tools.
    pub static_facts: HashMap<String, String>,
    /// Frequently-changing facts: current projects, recent topics, active issues.
    pub dynamic_facts: Vec<DynamicFact>,
    /// Unix-millis timestamp of the last update.
    pub last_updated: i64,
}

/// A single dynamic (time-sensitive) fact in the user profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicFact {
    pub content: String,
    pub timestamp: i64,
    /// High-level category: "project", "topic", "issue", etc.
    pub category: String,
}

// ---------------------------------------------------------------------------
// ProfileManager
// ---------------------------------------------------------------------------

/// Manages loading, updating, and persisting a `UserProfile`.
pub struct ProfileManager {
    profile: UserProfile,
    file_path: PathBuf,
    dirty: bool,
}

impl ProfileManager {
    /// Load an existing profile from disk, or create an empty one.
    pub fn load(user_id: &str, data_dir: &Path) -> Self {
        let file_path = data_dir.join(format!("profile-{user_id}.json"));
        let profile = if file_path.exists() {
            match std::fs::read_to_string(&file_path) {
                Ok(contents) => match serde_json::from_str::<UserProfile>(&contents) {
                    Ok(p) => {
                        debug!(user_id, path = %file_path.display(), "loaded user profile");
                        p
                    }
                    Err(e) => {
                        warn!(error = %e, path = %file_path.display(), "corrupt profile, starting fresh");
                        new_empty_profile(user_id)
                    }
                },
                Err(e) => {
                    warn!(error = %e, path = %file_path.display(), "failed to read profile");
                    new_empty_profile(user_id)
                }
            }
        } else {
            debug!(user_id, "no existing profile, creating empty");
            new_empty_profile(user_id)
        };

        Self {
            profile,
            file_path,
            dirty: false,
        }
    }

    /// Update the profile with a batch of newly extracted facts.
    ///
    /// Each fact's predicate is mapped to either a static profile field or a
    /// dynamic fact entry.  Static facts are overwritten (latest wins); dynamic
    /// facts are appended with deduplication.
    pub fn ingest_facts(&mut self, facts: &[Fact]) {
        if facts.is_empty() {
            return;
        }

        let now = chrono::Utc::now().timestamp_millis();

        for fact in facts {
            match classify_predicate(&fact.predicate) {
                PredicateMapping::Static(key) => {
                    self.profile
                        .static_facts
                        .insert(key, fact.object.clone());
                    self.dirty = true;
                }
                PredicateMapping::Preference => {
                    let cat = fact
                        .object_category
                        .as_deref()
                        .unwrap_or("general");
                    let key = format!("pref:{cat}");
                    self.profile
                        .static_facts
                        .insert(key, fact.object.clone());
                    self.dirty = true;
                }
                PredicateMapping::Dynamic(category) => {
                    // Dedup: skip if an identical content string already exists.
                    let already_exists = self
                        .dynamic_facts()
                        .iter()
                        .any(|d| d.content == fact.object);
                    if !already_exists {
                        self.profile.dynamic_facts.push(DynamicFact {
                            content: fact.object.clone(),
                            timestamp: fact.timestamp,
                            category,
                        });
                        self.dirty = true;
                    }
                }
                PredicateMapping::Unknown => {
                    debug!(predicate = %fact.predicate, "unmapped predicate, skipping");
                }
            }
        }

        // Enforce the cap on dynamic facts — keep the most recent.
        if self.profile.dynamic_facts.len() > MAX_DYNAMIC_FACTS {
            self.profile
                .dynamic_facts
                .sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            let excess = self.profile.dynamic_facts.len() - MAX_DYNAMIC_FACTS;
            self.profile.dynamic_facts.drain(..excess);
            self.dirty = true;
        }

        if self.dirty {
            self.profile.last_updated = now;
        }
    }

    /// Return a reference to the current profile.
    pub fn profile(&self) -> &UserProfile {
        &self.profile
    }

    /// Format the profile as a compact text block suitable for context injection.
    ///
    /// Targets roughly 200 tokens (≈800 characters).  Static facts first, then
    /// the most recent dynamic facts.
    pub fn format_for_injection(&self) -> String {
        let p = &self.profile;
        if p.static_facts.is_empty() && p.dynamic_facts.is_empty() {
            return String::new();
        }

        let mut lines: Vec<String> = Vec::with_capacity(16);
        lines.push("User Profile:".to_string());

        // Ordered keys we want to emit first (in display order).
        let ordered_keys: &[(&str, &str)] = &[
            ("name", "Name"),
            ("location", "Location"),
            ("organization", "Organization"),
            ("role", "Role"),
        ];

        for &(key, label) in ordered_keys {
            if let Some(val) = p.static_facts.get(key) {
                lines.push(format!("- {label}: {val}"));
            }
        }

        // Collect preferences (pref:* keys), sorted for determinism.
        let mut prefs: Vec<&str> = p
            .static_facts
            .iter()
            .filter(|(k, _)| k.starts_with("pref:"))
            .map(|(_, v)| v.as_str())
            .collect();
        prefs.sort();
        if !prefs.is_empty() {
            lines.push(format!("- Preferences: {}", prefs.join(", ")));
        }

        // Emit remaining static keys not yet printed (excluding pref:* and the
        // ordered keys above).
        let known: std::collections::HashSet<&str> = ordered_keys
            .iter()
            .map(|(k, _)| *k)
            .collect();
        let mut extra_static: Vec<(&String, &String)> = p
            .static_facts
            .iter()
            .filter(|(k, _)| !known.contains(k.as_str()) && !k.starts_with("pref:"))
            .collect();
        extra_static.sort_by_key(|(k, _)| (*k).clone());
        for (key, val) in extra_static {
            let label = titlecase(key);
            lines.push(format!("- {label}: {val}"));
        }

        // Dynamic facts — most recent first, capped to avoid blowing the budget.
        if !p.dynamic_facts.is_empty() {
            lines.push("Current context:".to_string());
            let mut sorted: Vec<&DynamicFact> = p.dynamic_facts.iter().collect();
            sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

            // Show at most 5 to stay within ~200 tokens.
            for fact in sorted.iter().take(5) {
                let label = category_label(&fact.category);
                lines.push(format!("- {label}: {}", fact.content));
            }
        }

        lines.join("\n")
    }

    /// Persist the profile to disk if it has been modified.
    pub fn save(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.profile)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&self.file_path, json)?;
        self.dirty = false;
        debug!(path = %self.file_path.display(), "profile saved");
        Ok(())
    }

    /// Remove dynamic facts older than `max_age_ms` milliseconds.
    pub fn trim_dynamic(&mut self, max_age_ms: i64) {
        let cutoff = chrono::Utc::now().timestamp_millis() - max_age_ms;
        let before = self.profile.dynamic_facts.len();
        self.profile
            .dynamic_facts
            .retain(|f| f.timestamp >= cutoff);
        let removed = before - self.profile.dynamic_facts.len();
        if removed > 0 {
            self.dirty = true;
            debug!(removed, "trimmed stale dynamic facts");
        }
    }

    // -- private helpers -----------------------------------------------------

    fn dynamic_facts(&self) -> &[DynamicFact] {
        &self.profile.dynamic_facts
    }
}

// ---------------------------------------------------------------------------
// Predicate classification
// ---------------------------------------------------------------------------

enum PredicateMapping {
    /// Maps directly to `static_facts[key]`.
    Static(String),
    /// Maps to `static_facts["pref:{object_category}"]`.
    Preference,
    /// Appended to `dynamic_facts` with the given category.
    Dynamic(String),
    /// Unrecognised predicate — ignored.
    Unknown,
}

fn classify_predicate(predicate: &str) -> PredicateMapping {
    match predicate {
        "name_is" => PredicateMapping::Static("name".to_string()),
        "lives_in" | "based_in" | "from" => {
            PredicateMapping::Static("location".to_string())
        }
        "works_at" | "works_for" => {
            PredicateMapping::Static("organization".to_string())
        }
        "role_is" | "is_a" => PredicateMapping::Static("role".to_string()),
        "prefers" | "uses" | "likes" => PredicateMapping::Preference,
        "working_on" | "building" | "developing" => {
            PredicateMapping::Dynamic("project".to_string())
        }
        "interested_in" | "studying" | "researching" => {
            PredicateMapping::Dynamic("topic".to_string())
        }
        "debugging" | "fixing" | "investigating" => {
            PredicateMapping::Dynamic("issue".to_string())
        }
        _ => PredicateMapping::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn new_empty_profile(user_id: &str) -> UserProfile {
    UserProfile {
        user_id: user_id.to_string(),
        ..Default::default()
    }
}

/// Capitalise the first letter of a string for display.
fn titlecase(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Map a dynamic-fact category to a human-readable label.
fn category_label(cat: &str) -> &str {
    match cat {
        "project" => "Working on",
        "topic" => "Recent topic",
        "issue" => "Active issue",
        _ => "Note",
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fact(predicate: &str, object: &str) -> Fact {
        Fact {
            subject: "user".to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            object_category: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    fn make_fact_with_category(
        predicate: &str,
        object: &str,
        category: &str,
    ) -> Fact {
        Fact {
            subject: "user".to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            object_category: Some(category.to_string()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    fn make_fact_at(predicate: &str, object: &str, ts: i64) -> Fact {
        Fact {
            subject: "user".to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            object_category: None,
            timestamp: ts,
        }
    }

    // -----------------------------------------------------------------------
    // Predicate mapping
    // -----------------------------------------------------------------------

    #[test]
    fn test_static_fact_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[
            make_fact("name_is", "Tate Berenbaum"),
            make_fact("lives_in", "Austin, Texas"),
            make_fact("works_at", "Not Community Labs"),
            make_fact("role_is", "Founder"),
        ]);

        let p = mgr.profile();
        assert_eq!(p.static_facts.get("name").unwrap(), "Tate Berenbaum");
        assert_eq!(p.static_facts.get("location").unwrap(), "Austin, Texas");
        assert_eq!(
            p.static_facts.get("organization").unwrap(),
            "Not Community Labs"
        );
        assert_eq!(p.static_facts.get("role").unwrap(), "Founder");
    }

    #[test]
    fn test_static_fact_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[make_fact("lives_in", "San Francisco")]);
        assert_eq!(
            mgr.profile().static_facts.get("location").unwrap(),
            "San Francisco"
        );

        mgr.ingest_facts(&[make_fact("based_in", "Austin, Texas")]);
        assert_eq!(
            mgr.profile().static_facts.get("location").unwrap(),
            "Austin, Texas"
        );
    }

    #[test]
    fn test_preference_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[
            make_fact_with_category("prefers", "Rust", "language"),
            make_fact_with_category("uses", "Vim", "editor"),
            make_fact_with_category("likes", "macOS", "os"),
        ]);

        let p = mgr.profile();
        assert_eq!(p.static_facts.get("pref:language").unwrap(), "Rust");
        assert_eq!(p.static_facts.get("pref:editor").unwrap(), "Vim");
        assert_eq!(p.static_facts.get("pref:os").unwrap(), "macOS");
    }

    #[test]
    fn test_preference_without_category() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[make_fact("prefers", "dark mode")]);
        assert_eq!(
            mgr.profile().static_facts.get("pref:general").unwrap(),
            "dark mode"
        );
    }

    // -----------------------------------------------------------------------
    // Dynamic facts
    // -----------------------------------------------------------------------

    #[test]
    fn test_dynamic_fact_ingestion() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[
            make_fact("working_on", "Memoryport"),
            make_fact("interested_in", "LongMemEval benchmark"),
        ]);

        assert_eq!(mgr.profile().dynamic_facts.len(), 2);
        assert_eq!(mgr.profile().dynamic_facts[0].content, "Memoryport");
        assert_eq!(mgr.profile().dynamic_facts[0].category, "project");
        assert_eq!(
            mgr.profile().dynamic_facts[1].content,
            "LongMemEval benchmark"
        );
        assert_eq!(mgr.profile().dynamic_facts[1].category, "topic");
    }

    #[test]
    fn test_dynamic_fact_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[make_fact("working_on", "Memoryport")]);
        mgr.ingest_facts(&[make_fact("working_on", "Memoryport")]);

        assert_eq!(mgr.profile().dynamic_facts.len(), 1);
    }

    #[test]
    fn test_dynamic_fact_cap() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        // Insert 25 distinct facts, each with a unique timestamp.
        let facts: Vec<Fact> = (0..25)
            .map(|i| make_fact_at("working_on", &format!("project-{i}"), 1000 + i))
            .collect();
        mgr.ingest_facts(&facts);

        assert_eq!(mgr.profile().dynamic_facts.len(), MAX_DYNAMIC_FACTS);
        // Oldest should have been dropped; newest retained.
        assert_eq!(
            mgr.profile().dynamic_facts.last().unwrap().content,
            "project-24"
        );
    }

    // -----------------------------------------------------------------------
    // format_for_injection
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_for_injection_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = ProfileManager::load("u1", dir.path());
        assert!(mgr.format_for_injection().is_empty());
    }

    #[test]
    fn test_format_for_injection_full() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[
            make_fact("name_is", "Tate Berenbaum"),
            make_fact("lives_in", "Austin, Texas"),
            make_fact("works_at", "Not Community Labs"),
            make_fact("role_is", "Founder"),
            make_fact_with_category("prefers", "Rust", "language"),
            make_fact_with_category("uses", "Vim", "editor"),
            make_fact_with_category("likes", "macOS", "os"),
            make_fact("working_on", "Memoryport"),
            make_fact("interested_in", "LongMemEval benchmark"),
        ]);

        let output = mgr.format_for_injection();
        assert!(output.contains("User Profile:"));
        assert!(output.contains("- Name: Tate Berenbaum"));
        assert!(output.contains("- Location: Austin, Texas"));
        assert!(output.contains("- Organization: Not Community Labs"));
        assert!(output.contains("- Role: Founder"));
        assert!(output.contains("- Preferences:"));
        assert!(output.contains("Rust"));
        assert!(output.contains("Vim"));
        assert!(output.contains("macOS"));
        assert!(output.contains("Current context:"));
        assert!(output.contains("Working on: Memoryport"));
        assert!(output.contains("Recent topic: LongMemEval benchmark"));
    }

    // -----------------------------------------------------------------------
    // Save / load round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();

        {
            let mut mgr = ProfileManager::load("u1", dir.path());
            mgr.ingest_facts(&[
                make_fact("name_is", "Alice"),
                make_fact("working_on", "Widget"),
            ]);
            mgr.save().unwrap();
        }

        // Load again from the same directory.
        let mgr2 = ProfileManager::load("u1", dir.path());
        assert_eq!(
            mgr2.profile().static_facts.get("name").unwrap(),
            "Alice"
        );
        assert_eq!(mgr2.profile().dynamic_facts.len(), 1);
        assert_eq!(mgr2.profile().dynamic_facts[0].content, "Widget");
    }

    #[test]
    fn test_save_not_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        // No changes — save should be a no-op.
        mgr.save().unwrap();
        assert!(!mgr.file_path.exists());
    }

    // -----------------------------------------------------------------------
    // trim_dynamic
    // -----------------------------------------------------------------------

    #[test]
    fn test_trim_dynamic() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        let now = chrono::Utc::now().timestamp_millis();
        let old_ts = now - 100_000; // 100 seconds ago
        let recent_ts = now - 1_000; // 1 second ago

        mgr.ingest_facts(&[
            make_fact_at("working_on", "old-project", old_ts),
            make_fact_at("working_on", "new-project", recent_ts),
        ]);
        assert_eq!(mgr.profile().dynamic_facts.len(), 2);

        // Trim anything older than 50 seconds.
        mgr.trim_dynamic(50_000);
        assert_eq!(mgr.profile().dynamic_facts.len(), 1);
        assert_eq!(
            mgr.profile().dynamic_facts[0].content,
            "new-project"
        );
    }

    #[test]
    fn test_trim_dynamic_nothing_to_trim() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[make_fact("working_on", "project")]);
        let before = mgr.profile().dynamic_facts.len();
        mgr.trim_dynamic(999_999_999);
        assert_eq!(mgr.profile().dynamic_facts.len(), before);
    }

    // -----------------------------------------------------------------------
    // Unknown predicates
    // -----------------------------------------------------------------------

    #[test]
    fn test_unknown_predicate_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[make_fact("random_pred", "value")]);
        assert!(mgr.profile().static_facts.is_empty());
        assert!(mgr.profile().dynamic_facts.is_empty());
    }

    // -----------------------------------------------------------------------
    // Alternate predicate synonyms
    // -----------------------------------------------------------------------

    #[test]
    fn test_predicate_synonyms() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = ProfileManager::load("u1", dir.path());

        mgr.ingest_facts(&[
            make_fact("from", "New York"),
            make_fact("works_for", "Acme Corp"),
            make_fact("is_a", "Engineer"),
            make_fact("building", "Rocket"),
            make_fact("debugging", "Memory leak"),
            make_fact("researching", "Vector DBs"),
        ]);

        let p = mgr.profile();
        assert_eq!(p.static_facts.get("location").unwrap(), "New York");
        assert_eq!(p.static_facts.get("organization").unwrap(), "Acme Corp");
        assert_eq!(p.static_facts.get("role").unwrap(), "Engineer");
        assert_eq!(p.dynamic_facts.len(), 3);

        let categories: Vec<&str> =
            p.dynamic_facts.iter().map(|f| f.category.as_str()).collect();
        assert!(categories.contains(&"project"));
        assert!(categories.contains(&"issue"));
        assert!(categories.contains(&"topic"));
    }
}
