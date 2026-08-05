//! Bounded anti-claim wording checks.
//!
//! Evidence reports must keep their claims bounded. These fragment lists and
//! pure checks are the single authority for the anti-claim wording that the
//! evidence readiness shells enforce.

/// Anti-claim fragments that assertion-readiness reports must contain.
pub const REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS: [&str; 5] = [
    "A high exercised count only says the committed run observed cataloged SDK assertions",
    "Local harness coverage is not snapshot replay evidence",
    "Zero ordinary assertion blockers applies only to accepted v2 assertion evidence",
    "Legacy bare-array assertion artifacts are diagnostic-only",
    "Operator/product readiness still requires separate replay, minimization/reproduction, workload onboarding, and triage evidence",
];

/// Overclaim fragments that assertion-readiness reports must never contain.
pub const FORBIDDEN_ASSERTION_OVERCLAIM_FRAGMENTS: [&str; 5] = [
    "product parity is established",
    "full antithesis-style product replacement",
    "assertion density proves replay",
    "assertion coverage proves replay",
    "zero assertion blockers proves product parity",
];

/// Return every required fragment missing from the text, in list order.
pub fn missing_required_fragments<'a>(text: &str, required: &[&'a str]) -> Vec<&'a str> {
    required
        .iter()
        .copied()
        .filter(|fragment| !text.contains(fragment))
        .collect()
}

/// Return the first forbidden fragment present in the text, if any.
pub fn find_forbidden_fragment<'a>(text: &str, forbidden: &[&'a str]) -> Option<&'a str> {
    forbidden
        .iter()
        .copied()
        .find(|fragment| text.contains(fragment))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_each_missing_required_fragment() {
        let missing = missing_required_fragments(
            "Local harness coverage is not snapshot replay evidence",
            &REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS,
        );
        assert_eq!(
            missing.len(),
            REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS.len() - 1
        );
        assert!(!missing.contains(&REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS[1]));
    }

    #[test]
    fn detects_the_first_forbidden_fragment() {
        assert_eq!(
            find_forbidden_fragment(
                "this report shows assertion coverage proves replay",
                &FORBIDDEN_ASSERTION_OVERCLAIM_FRAGMENTS,
            ),
            Some("assertion coverage proves replay")
        );
        assert_eq!(
            find_forbidden_fragment(
                "bounded local evidence only",
                &FORBIDDEN_ASSERTION_OVERCLAIM_FRAGMENTS,
            ),
            None
        );
    }

    #[test]
    fn empty_text_misses_every_required_fragment() {
        let missing = missing_required_fragments("", &REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS);
        assert_eq!(missing.len(), REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS.len());
    }
}
