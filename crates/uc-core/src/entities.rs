use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// EntityType
// ---------------------------------------------------------------------------

/// Classification of a named entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Person,
    Place,
    Org,
    Project,
    Tool,
    Concept,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Place => "place",
            Self::Org => "org",
            Self::Project => "project",
            Self::Tool => "tool",
            Self::Concept => "concept",
        }
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for EntityType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "person" => Ok(Self::Person),
            "place" => Ok(Self::Place),
            "org" | "organization" => Ok(Self::Org),
            "project" => Ok(Self::Project),
            "tool" => Ok(Self::Tool),
            "concept" => Ok(Self::Concept),
            _ => Err(format!("unknown entity type: {s}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Entity
// ---------------------------------------------------------------------------

/// A named entity extracted from conversations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Uuid,
    /// Canonical display name.
    pub name: String,
    /// Alternative names and spellings that resolved to this entity.
    pub aliases: Vec<String>,
    pub entity_type: EntityType,
    /// Rolling summary of known facts about this entity.
    pub summary: String,
    pub user_id: String,
    /// Timestamp (ms) when this entity was first observed.
    pub first_seen: i64,
    /// Timestamp (ms) of the most recent observation.
    pub last_seen: i64,
    /// Number of facts linked to this entity.
    pub fact_count: u32,
    pub created_at: i64,
}

// ---------------------------------------------------------------------------
// Jaro-Winkler similarity
// ---------------------------------------------------------------------------

/// Compute the Jaro similarity between two strings (case-insensitive).
fn jaro(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().flat_map(|c| c.to_lowercase()).collect();
    let b: Vec<char> = b.chars().flat_map(|c| c.to_lowercase()).collect();

    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 && b_len == 0 {
        return 1.0;
    }
    if a_len == 0 || b_len == 0 {
        return 0.0;
    }

    let match_window = (a_len.max(b_len) / 2).saturating_sub(1);

    let mut a_matched = vec![false; a_len];
    let mut b_matched = vec![false; b_len];

    let mut matches: f32 = 0.0;

    // Find matching characters within the window.
    for i in 0..a_len {
        let lo = i.saturating_sub(match_window);
        let hi = (i + match_window + 1).min(b_len);
        for j in lo..hi {
            if !b_matched[j] && a[i] == b[j] {
                a_matched[i] = true;
                b_matched[j] = true;
                matches += 1.0;
                break;
            }
        }
    }

    if matches == 0.0 {
        return 0.0;
    }

    // Count transpositions.
    let mut transpositions: f32 = 0.0;
    let mut k = 0usize;
    for i in 0..a_len {
        if !a_matched[i] {
            continue;
        }
        while !b_matched[k] {
            k += 1;
        }
        if a[i] != b[k] {
            transpositions += 1.0;
        }
        k += 1;
    }

    (matches / a_len as f32
        + matches / b_len as f32
        + (matches - transpositions / 2.0) / matches)
        / 3.0
}

/// Compute the Jaro-Winkler similarity between two strings.
///
/// Returns a value in `[0.0, 1.0]` where 1.0 means identical (case-insensitive).
/// The Winkler modification boosts the score for strings that share a common
/// prefix (up to 4 characters), making it well-suited for name matching.
pub fn name_similarity(a: &str, b: &str) -> f32 {
    let jaro_score = jaro(a, b);

    // Winkler prefix bonus (up to 4 characters).
    let a_lower: Vec<char> = a.chars().flat_map(|c| c.to_lowercase()).collect();
    let b_lower: Vec<char> = b.chars().flat_map(|c| c.to_lowercase()).collect();

    let prefix_len = a_lower
        .iter()
        .zip(b_lower.iter())
        .take(4)
        .take_while(|(x, y)| x == y)
        .count();

    // Standard Winkler scaling factor p = 0.1.
    jaro_score + prefix_len as f32 * 0.1 * (1.0 - jaro_score)
}

// ---------------------------------------------------------------------------
// Name normalization
// ---------------------------------------------------------------------------

/// Normalize a name for comparison: trim whitespace, collapse interior runs of
/// whitespace to a single space.
fn normalize_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// EntityRegistry
// ---------------------------------------------------------------------------

/// In-memory registry of named entities with deduplication.
///
/// Entities are deduplicated using a three-tier strategy:
///   1. Exact case-insensitive match against canonical names and aliases.
///   2. Fuzzy Jaro-Winkler match above a configurable threshold (default 0.85).
///   3. Create a new entity if neither tier matches.
pub struct EntityRegistry {
    entities: Vec<Entity>,
    user_id: String,
}

impl EntityRegistry {
    /// Create an empty registry for the given user.
    pub fn new(user_id: &str) -> Self {
        Self {
            entities: Vec::new(),
            user_id: user_id.to_string(),
        }
    }

    /// Find or create an entity, returning its ID.
    ///
    /// The resolution strategy is:
    ///   1. **Exact match** (case-insensitive) against all canonical names and
    ///      aliases.  If found, update `last_seen` and return.
    ///   2. **Fuzzy match** using Jaro-Winkler similarity.  If the best score
    ///      exceeds 0.85 *and* the entity type matches, merge the new name as
    ///      an alias, update `last_seen`, and return.
    ///   3. **Create** a brand-new entity.
    pub fn resolve(&mut self, name: &str, entity_type: EntityType, timestamp: i64) -> Uuid {
        let normalized = normalize_name(name);
        if normalized.is_empty() {
            // Degenerate input — still create an entity, but with the raw name.
            return self.create_entity(name.to_string(), entity_type, timestamp);
        }

        // Tier 1: exact case-insensitive match.
        if let Some(entity) = self.find_exact_mut(&normalized) {
            entity.last_seen = timestamp;
            return entity.id;
        }

        // Tier 2: fuzzy match.
        let (best_idx, best_score) = self.best_fuzzy_match(&normalized);
        if best_score > 0.85 {
            if let Some(entity) = self.entities.get_mut(best_idx) {
                if entity.entity_type == entity_type {
                    // Add as alias if not already present.
                    let lower = normalized.to_lowercase();
                    let already = entity.name.to_lowercase() == lower
                        || entity.aliases.iter().any(|a| a.to_lowercase() == lower);
                    if !already {
                        entity.aliases.push(normalized);
                    }
                    entity.last_seen = timestamp;
                    return entity.id;
                }
            }
        }

        // Tier 3: new entity.
        self.create_entity(normalized, entity_type, timestamp)
    }

    /// Get an entity by ID.
    pub fn get(&self, id: &Uuid) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == *id)
    }

    /// Search entities by fuzzy name match, returning up to `limit` results
    /// ordered by descending similarity.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Entity> {
        let q = normalize_name(query);
        if q.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(usize, f32)> = self
            .entities
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let mut best = name_similarity(&q, &e.name);
                for alias in &e.aliases {
                    let s = name_similarity(&q, alias);
                    if s > best {
                        best = s;
                    }
                }
                (i, best)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(limit)
            .filter(|(_, score)| *score > 0.5)
            .map(|(i, _)| &self.entities[i])
            .collect()
    }

    /// Return a slice of all registered entities.
    pub fn all(&self) -> &[Entity] {
        &self.entities
    }

    /// Update the summary for a given entity.
    pub fn update_summary(&mut self, id: &Uuid, summary: &str) {
        if let Some(e) = self.entities.iter_mut().find(|e| e.id == *id) {
            e.summary = summary.to_string();
        }
    }

    /// Increment the linked fact count for an entity.
    pub fn increment_fact_count(&mut self, id: &Uuid) {
        if let Some(e) = self.entities.iter_mut().find(|e| e.id == *id) {
            e.fact_count += 1;
        }
    }

    /// Serialize the registry to JSON for persistence.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.entities).unwrap_or_else(|_| "[]".to_string())
    }

    /// Deserialize a registry from JSON.
    pub fn from_json(json: &str, user_id: &str) -> Self {
        let entities: Vec<Entity> =
            serde_json::from_str(json).unwrap_or_default();
        Self {
            entities,
            user_id: user_id.to_string(),
        }
    }

    // -- private helpers ----------------------------------------------------

    /// Case-insensitive exact match against canonical names and aliases.
    fn find_exact_mut(&mut self, normalized: &str) -> Option<&mut Entity> {
        let lower = normalized.to_lowercase();
        self.entities.iter_mut().find(|e| {
            e.name.to_lowercase() == lower
                || e.aliases.iter().any(|a| a.to_lowercase() == lower)
        })
    }

    /// Find the entity with the highest Jaro-Winkler similarity to `name`.
    /// Returns `(index, score)`. If the registry is empty, returns `(0, 0.0)`.
    fn best_fuzzy_match(&self, name: &str) -> (usize, f32) {
        let mut best_idx = 0;
        let mut best_score: f32 = 0.0;

        for (i, entity) in self.entities.iter().enumerate() {
            let s = name_similarity(name, &entity.name);
            if s > best_score {
                best_score = s;
                best_idx = i;
            }
            for alias in &entity.aliases {
                let s = name_similarity(name, alias);
                if s > best_score {
                    best_score = s;
                    best_idx = i;
                }
            }
        }

        (best_idx, best_score)
    }

    /// Create a new entity, insert it, and return its ID.
    fn create_entity(
        &mut self,
        name: String,
        entity_type: EntityType,
        timestamp: i64,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.entities.push(Entity {
            id,
            name,
            aliases: Vec::new(),
            entity_type,
            summary: String::new(),
            user_id: self.user_id.clone(),
            first_seen: timestamp,
            last_seen: timestamp,
            fact_count: 0,
            created_at: timestamp,
        });
        id
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Jaro-Winkler similarity -------------------------------------------

    #[test]
    fn similarity_identical() {
        assert!((name_similarity("hello", "hello") - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn similarity_case_insensitive() {
        assert!((name_similarity("Hello", "hello") - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn similarity_empty_strings() {
        assert!((name_similarity("", "") - 1.0).abs() < f32::EPSILON);
        assert!(name_similarity("abc", "").abs() < f32::EPSILON);
        assert!(name_similarity("", "abc").abs() < f32::EPSILON);
    }

    #[test]
    fn similarity_known_pairs() {
        // Jaro-Winkler should give high scores for these near-identical names.
        let s = name_similarity("JavaScript", "Javascript");
        assert!(s > 0.95, "JavaScript vs Javascript = {s}");

        let s = name_similarity("Tate", "tate");
        assert!((s - 1.0).abs() < f32::EPSILON, "Tate vs tate = {s}");

        // Different but somewhat related strings.
        let s = name_similarity("Martha", "Marhta");
        assert!(s > 0.9, "Martha vs Marhta = {s}");

        // Completely different strings should score low.
        let s = name_similarity("Rust", "Python");
        assert!(s < 0.6, "Rust vs Python = {s}");
    }

    #[test]
    fn similarity_prefix_bonus() {
        // Strings that share a prefix should score higher than those that don't.
        let with_prefix = name_similarity("JohnSmith", "JohnSmyth");
        let without_prefix = name_similarity("SmithJohn", "SmythJohn");
        assert!(
            with_prefix >= without_prefix,
            "prefix bonus: {with_prefix} vs {without_prefix}"
        );
    }

    #[test]
    fn similarity_single_char() {
        let s = name_similarity("a", "a");
        assert!((s - 1.0).abs() < f32::EPSILON);

        let s = name_similarity("a", "b");
        assert!(s < 0.9, "a vs b = {s}");
    }

    // -- EntityRegistry: exact dedup ----------------------------------------

    #[test]
    fn resolve_exact_dedup_case_insensitive() {
        let mut reg = EntityRegistry::new("user1");

        let id1 = reg.resolve("Tate", EntityType::Person, 1000);
        let id2 = reg.resolve("tate", EntityType::Person, 2000);

        assert_eq!(id1, id2, "case-insensitive exact match should dedup");
        assert_eq!(reg.all().len(), 1);
        assert_eq!(reg.get(&id1).unwrap().last_seen, 2000);
    }

    #[test]
    fn resolve_exact_dedup_via_alias() {
        let mut reg = EntityRegistry::new("user1");

        let id1 = reg.resolve("JavaScript", EntityType::Tool, 1000);
        // Manually add an alias.
        reg.entities[0].aliases.push("JS".to_string());

        let id2 = reg.resolve("js", EntityType::Tool, 2000);
        assert_eq!(id1, id2, "alias match should dedup");
        assert_eq!(reg.all().len(), 1);
    }

    // -- EntityRegistry: fuzzy dedup ----------------------------------------

    #[test]
    fn resolve_fuzzy_dedup() {
        let mut reg = EntityRegistry::new("user1");

        // Use names that differ by more than just casing so they bypass the
        // exact-match tier and exercise the Jaro-Winkler fuzzy path.
        let id1 = reg.resolve("Kubernetes", EntityType::Tool, 1000);
        let id2 = reg.resolve("Kubernets", EntityType::Tool, 2000); // typo

        assert_eq!(id1, id2, "fuzzy match should dedup");
        assert_eq!(reg.all().len(), 1);

        // The variant should be recorded as an alias.
        let entity = reg.get(&id1).unwrap();
        assert!(
            entity.aliases.contains(&"Kubernets".to_string()),
            "alias should be added: {:?}",
            entity.aliases,
        );
    }

    #[test]
    fn resolve_fuzzy_requires_type_match() {
        let mut reg = EntityRegistry::new("user1");

        // Names that are fuzzy-similar but assigned to different entity types
        // should NOT merge. Use names that differ beyond casing so they bypass
        // exact match and hit the fuzzy tier.
        let id_a = reg.resolve("Kubernetes", EntityType::Tool, 1000);
        let id_b = reg.resolve("Kubernets", EntityType::Concept, 2000);
        assert_ne!(
            id_a, id_b,
            "fuzzy match should not merge across different types"
        );
        assert_eq!(reg.all().len(), 2);
    }

    // -- EntityRegistry: no false merges ------------------------------------

    #[test]
    fn resolve_no_merge_different_names() {
        let mut reg = EntityRegistry::new("user1");

        let id1 = reg.resolve("Rust", EntityType::Tool, 1000);
        let id2 = reg.resolve("Python", EntityType::Tool, 2000);

        assert_ne!(id1, id2, "different names should create separate entities");
        assert_eq!(reg.all().len(), 2);
    }

    // -- EntityRegistry: alias accumulation ---------------------------------

    #[test]
    fn alias_accumulation() {
        let mut reg = EntityRegistry::new("user1");

        let id = reg.resolve("New York City", EntityType::Place, 1000);
        reg.entities[0].aliases.push("NYC".to_string());

        let id2 = reg.resolve("nyc", EntityType::Place, 2000);
        assert_eq!(id, id2);

        // Adding the same alias again should not duplicate.
        let id3 = reg.resolve("NYC", EntityType::Place, 3000);
        assert_eq!(id, id3);

        let entity = reg.get(&id).unwrap();
        let nyc_count = entity
            .aliases
            .iter()
            .filter(|a| a.to_lowercase() == "nyc")
            .count();
        assert_eq!(nyc_count, 1, "alias should not be duplicated");
    }

    // -- EntityRegistry: search ---------------------------------------------

    #[test]
    fn search_returns_ordered_results() {
        let mut reg = EntityRegistry::new("user1");

        reg.resolve("Tate Berenbaum", EntityType::Person, 1000);
        reg.resolve("Arweave", EntityType::Project, 2000);
        reg.resolve("Rust", EntityType::Tool, 3000);

        let results = reg.search("Tate", 10);
        assert!(!results.is_empty(), "search should find results");
        assert_eq!(
            results[0].name, "Tate Berenbaum",
            "best match should be first"
        );
    }

    #[test]
    fn search_empty_query() {
        let mut reg = EntityRegistry::new("user1");
        reg.resolve("Rust", EntityType::Tool, 1000);

        let results = reg.search("", 10);
        assert!(results.is_empty(), "empty query should return nothing");
    }

    // -- EntityRegistry: update/increment -----------------------------------

    #[test]
    fn update_summary_and_fact_count() {
        let mut reg = EntityRegistry::new("user1");

        let id = reg.resolve("LanceDB", EntityType::Tool, 1000);
        assert_eq!(reg.get(&id).unwrap().fact_count, 0);
        assert!(reg.get(&id).unwrap().summary.is_empty());

        reg.update_summary(&id, "A vector database built on Lance.");
        reg.increment_fact_count(&id);
        reg.increment_fact_count(&id);

        let entity = reg.get(&id).unwrap();
        assert_eq!(entity.summary, "A vector database built on Lance.");
        assert_eq!(entity.fact_count, 2);
    }

    // -- Serialization roundtrip -------------------------------------------

    #[test]
    fn json_roundtrip() {
        let mut reg = EntityRegistry::new("user1");

        let id1 = reg.resolve("Tate", EntityType::Person, 1000);
        let id2 = reg.resolve("Arweave", EntityType::Project, 2000);
        reg.update_summary(&id1, "The user.");
        reg.increment_fact_count(&id2);

        let json = reg.to_json();
        let restored = EntityRegistry::from_json(&json, "user1");

        assert_eq!(restored.all().len(), 2);
        assert_eq!(restored.get(&id1).unwrap().name, "Tate");
        assert_eq!(restored.get(&id1).unwrap().summary, "The user.");
        assert_eq!(restored.get(&id2).unwrap().fact_count, 1);
    }

    #[test]
    fn from_json_handles_invalid_input() {
        let reg = EntityRegistry::from_json("not valid json", "user1");
        assert!(reg.all().is_empty());
    }

    // -- Name normalization -------------------------------------------------

    #[test]
    fn normalize_trims_and_collapses_whitespace() {
        let mut reg = EntityRegistry::new("user1");

        let id1 = reg.resolve("  Tate  Berenbaum  ", EntityType::Person, 1000);
        let id2 = reg.resolve("Tate Berenbaum", EntityType::Person, 2000);

        assert_eq!(id1, id2, "whitespace normalization should dedup");
        assert_eq!(reg.all().len(), 1);
        assert_eq!(reg.get(&id1).unwrap().name, "Tate Berenbaum");
    }
}
