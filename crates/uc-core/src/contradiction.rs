//! Contradiction detection and resolution.
//!
//! When a new fact is stored, check if it contradicts an existing fact
//! (same subject + similar predicate, different object). If so, mark
//! the old fact as superseded.

use crate::facts::Fact;

/// Predicates that represent the same relation (grouped for contradiction detection).
const PREDICATE_GROUPS: &[&[&str]] = &[
    &["lives_in", "based_in", "from", "moved_to"],
    &["works_at", "works_for", "joined"],
    &["role_is", "is_a"],
    &["prefers", "uses", "likes", "switched_to"],
    &["name_is"],
    &["age_is"],
    &["speaks"],
    &["working_on", "building", "developing"],
    &["learning", "studying"],
];

/// Check if two predicates belong to the same group (and thus could contradict).
pub fn predicates_conflict(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    for group in PREDICATE_GROUPS {
        if group.contains(&a) && group.contains(&b) {
            return true;
        }
    }
    false
}

/// A detected contradiction between a new fact and an existing fact.
#[derive(Debug, Clone)]
pub struct Contradiction {
    /// The existing fact that is now superseded.
    pub old_fact_id: String,
    /// The new fact that supersedes it.
    pub new_fact_id: String,
    /// Why we think it's a contradiction.
    pub reason: String,
}

/// Find contradictions between new facts and existing facts.
///
/// For each new fact, search existing facts with the same subject and a
/// conflicting predicate. If the objects differ, it's a contradiction.
pub fn detect_contradictions(
    new_facts: &[Fact],
    existing_facts: &[Fact],
) -> Vec<Contradiction> {
    let mut contradictions = Vec::new();

    for new_fact in new_facts {
        for existing in existing_facts {
            // Skip if not valid (already superseded)
            if !existing.valid {
                continue;
            }

            // Same user
            if new_fact.user_id != existing.user_id {
                continue;
            }

            // Same subject (case-insensitive)
            if !subjects_match(&new_fact.subject, &existing.subject) {
                continue;
            }

            // Conflicting predicate
            if !predicates_conflict(&new_fact.predicate, &existing.predicate) {
                continue;
            }

            // Different object = contradiction
            let new_obj = new_fact.object.trim().to_lowercase();
            let old_obj = existing.object.trim().to_lowercase();
            if new_obj != old_obj && !new_obj.is_empty() && !old_obj.is_empty() {
                contradictions.push(Contradiction {
                    old_fact_id: existing.id.to_string(),
                    new_fact_id: new_fact.id.to_string(),
                    reason: format!(
                        "{} {} changed from '{}' to '{}'",
                        new_fact.subject, new_fact.predicate, existing.object, new_fact.object
                    ),
                });
            }
        }
    }

    contradictions
}

/// Check if two subject strings refer to the same entity.
fn subjects_match(a: &str, b: &str) -> bool {
    let a_lower = a.trim().to_lowercase();
    let b_lower = b.trim().to_lowercase();

    if a_lower == b_lower {
        return true;
    }

    // "I" / "user" / "me" all refer to the same person
    let self_refs = ["i", "user", "me", "my", "myself"];
    if self_refs.contains(&a_lower.as_str()) && self_refs.contains(&b_lower.as_str()) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_fact(subject: &str, predicate: &str, object: &str, valid: bool) -> Fact {
        Fact {
            id: Uuid::new_v4(),
            content: format!("{} {} {}", subject, predicate, object),
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            source_chunk_id: "chunk1".to_string(),
            session_id: "s1".to_string(),
            user_id: "user1".to_string(),
            document_date: 1000,
            event_date: None,
            valid,
            superseded_by: None,
            confidence: 1.0,
            created_at: 1000,
        }
    }

    #[test]
    fn test_detects_location_change() {
        let existing = vec![make_fact("I", "lives_in", "New York", true)];
        let new_facts = vec![make_fact("I", "moved_to", "London", true)];
        let contradictions = detect_contradictions(&new_facts, &existing);
        assert_eq!(contradictions.len(), 1);
        assert!(contradictions[0].reason.contains("New York"));
        assert!(contradictions[0].reason.contains("London"));
    }

    #[test]
    fn test_detects_job_change() {
        let existing = vec![make_fact("user", "works_at", "Google", true)];
        let new_facts = vec![make_fact("I", "joined", "Anthropic", true)];
        let contradictions = detect_contradictions(&new_facts, &existing);
        assert_eq!(contradictions.len(), 1);
    }

    #[test]
    fn test_no_contradiction_different_subject() {
        let existing = vec![make_fact("Alice", "lives_in", "New York", true)];
        let new_facts = vec![make_fact("Bob", "lives_in", "London", true)];
        let contradictions = detect_contradictions(&new_facts, &existing);
        assert_eq!(contradictions.len(), 0);
    }

    #[test]
    fn test_no_contradiction_different_predicate_group() {
        let existing = vec![make_fact("I", "lives_in", "New York", true)];
        let new_facts = vec![make_fact("I", "works_at", "Google", true)];
        let contradictions = detect_contradictions(&new_facts, &existing);
        assert_eq!(contradictions.len(), 0);
    }

    #[test]
    fn test_skips_already_superseded() {
        let existing = vec![make_fact("I", "lives_in", "New York", false)]; // already superseded
        let new_facts = vec![make_fact("I", "lives_in", "London", true)];
        let contradictions = detect_contradictions(&new_facts, &existing);
        assert_eq!(contradictions.len(), 0);
    }

    #[test]
    fn test_same_object_no_contradiction() {
        let existing = vec![make_fact("I", "lives_in", "Austin", true)];
        let new_facts = vec![make_fact("I", "lives_in", "Austin", true)];
        let contradictions = detect_contradictions(&new_facts, &existing);
        assert_eq!(contradictions.len(), 0);
    }

    #[test]
    fn test_self_reference_equivalence() {
        assert!(subjects_match("I", "user"));
        assert!(subjects_match("me", "I"));
        assert!(subjects_match("myself", "user"));
        assert!(!subjects_match("I", "Alice"));
    }

    #[test]
    fn test_predicate_groups() {
        assert!(predicates_conflict("lives_in", "moved_to"));
        assert!(predicates_conflict("works_at", "joined"));
        assert!(predicates_conflict("prefers", "switched_to"));
        assert!(!predicates_conflict("lives_in", "works_at"));
        assert!(predicates_conflict("uses", "uses")); // same predicate
    }

    #[test]
    fn test_preference_update() {
        let existing = vec![make_fact("I", "uses", "Vim", true)];
        let new_facts = vec![make_fact("I", "switched_to", "Neovim", true)];
        let contradictions = detect_contradictions(&new_facts, &existing);
        assert_eq!(contradictions.len(), 1);
    }
}
