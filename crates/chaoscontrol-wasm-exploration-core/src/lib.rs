//! Pure deterministic core for the bounded host-side Wasm component
//! exploration rail.
//!
//! Profile, materialization, and case validation; seed and identity
//! construction; outcome classification; normalized strategy comparison;
//! failure predicates; shrink and promotion admission; bound decisions;
//! retention plans; and evidence DTOs live here. Filesystem, environment,
//! process, clock, network, runtime, and KVM effects stay in the harness
//! shells that later tasks add.
//!
//! The v1 exploration profile pins the materialization bundle to
//! `"not-yet-materialized"`, so this core fails closed: every v1 case is
//! diagnostic-only and promotion admission is always denied.

#![forbid(unsafe_code)]

pub mod bounds;
pub mod case;
pub mod classify;
pub mod compare;
pub mod evidence;
pub mod predicate;
pub mod profile;
pub mod promotion;
pub mod retention;
pub mod shrink;

pub use bounds::{check_artifact_bytes, check_wit_bytes, decide_bound, BoundDecision, BoundSkip};
pub use case::{
    case_identity, derive_seed_material, validate_case, CorpusClass, ExpectedValidity,
    ExplorationCase, GeneratorKind, SeedMaterial,
};
pub use classify::{classify_boundary_rejection, AdmissionBoundary, OutcomeClass};
pub use compare::{
    compare_strategies, NormalizedObservation, Strategy, StrategyComparison, StrategyRun,
};
pub use evidence::{
    check_claim, exploration_report, ExplorationReport, BOUNDED_EVIDENCE_CLAIM,
    DIAGNOSTIC_ONLY_CLAIM, REPORT_SCHEMA,
};
pub use predicate::{divergence_predicate, predicate_holds, same_predicate, FailurePredicate};
pub use profile::{
    profile_identity, proof_eligible, validate_profile, ExplorationBounds, ExplorationProfile,
    Materialization, RuntimeCohort, ToolCohort, PROFILE_SCHEMA,
};
pub use promotion::{admit_promotion, PromotionDecision, PromotionDenial};
pub use retention::{plan_retention, RetentionPlan};
pub use shrink::{
    admit_shrink_step, open_shrink_ledger, record_accepted_step, ShrinkCandidate, ShrinkLedger,
    ShrinkRejection, ShrinkStep,
};

/// Requirement evidence markers for the accepted Cairn specification.
/// r[chaoscontrol.wasm_component.functional_core]
pub const REQUIREMENT_MARKERS: [&str; 1] = ["chaoscontrol.wasm_component.functional_core"];

pub const BLAKE3_HEX_LENGTH: usize = 64;
pub const MAXIMUM_IDENTIFIER_BYTES: usize = 128;
const HEX_ALPHABET_LENGTH: usize = 16;
const HEX_CHARACTERS_PER_BYTE: usize = 2;
const HIGH_NIBBLE_SHIFT: u32 = 4;
const NIBBLE_MASK: u8 = 0x0f;
const GIT_REVISION_HEX_LENGTH: usize = 40;

/// Lowercase hex encoding of exactly the given bytes.
pub fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; HEX_ALPHABET_LENGTH] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(HEX_CHARACTERS_PER_BYTE));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> HIGH_NIBBLE_SHIFT)]));
        output.push(char::from(HEX[usize::from(byte & NIBBLE_MASK)]));
    }
    output
}

/// BLAKE3 digest of the bytes, lowercase hex encoded.
pub fn blake3_hex(bytes: &[u8]) -> String {
    encode_hex(&blake3::hash(bytes).as_bytes()[..])
}

pub(crate) fn is_lower_hex(value: &str, expected_length: usize) -> bool {
    value.len() == expected_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn is_blake3_hex(value: &str) -> bool {
    is_lower_hex(value, BLAKE3_HEX_LENGTH)
}

pub(crate) fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAXIMUM_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(crate) fn is_git_revision(value: &str) -> bool {
    is_lower_hex(value, GIT_REVISION_HEX_LENGTH)
}

/// Canonical JSON bytes for identity hashing. Serde struct field order is
/// declared once per DTO, so identical facts always serialize identically.
pub(crate) fn canonical_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| format!("serialize canonical identity: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encoding_is_lowercase_and_fixed_width() {
        assert_eq!(encode_hex(&[0x00, 0xff, 0xa5]), "00ffa5");
        assert_eq!(blake3_hex(b"identity").len(), BLAKE3_HEX_LENGTH);
        assert_eq!(blake3_hex(b"identity"), blake3_hex(b"identity"));
        assert_ne!(blake3_hex(b"identity"), blake3_hex(b"identity-two"));
    }

    #[test]
    fn shape_checks_accept_and_reject_exact_facts() {
        assert!(is_blake3_hex(&"a".repeat(BLAKE3_HEX_LENGTH)));
        assert!(!is_blake3_hex(&"a".repeat(BLAKE3_HEX_LENGTH - 1)));
        assert!(!is_blake3_hex(&"A".repeat(BLAKE3_HEX_LENGTH)));
        assert!(is_git_revision(&"b".repeat(GIT_REVISION_HEX_LENGTH)));
        assert!(!is_git_revision(&"b".repeat(GIT_REVISION_HEX_LENGTH + 1)));
        assert!(is_identifier("wasm-component-exploration-v1"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("Uppercase-Not-Allowed"));
        assert!(!is_identifier("spaces not allowed"));
        assert!(!is_identifier(&"x".repeat(MAXIMUM_IDENTIFIER_BYTES + 1)));
    }
}
