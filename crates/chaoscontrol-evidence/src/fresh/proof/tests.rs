// r[verify chaoscontrol.fresh_workload_proofs.validation]
use super::*;

const WORKLOADS: [&str; 4] = ["raft", "redb", "net", "rust-workload"];
const LEGACY_SCHEMA_VERSION: u32 = 1;

fn accepted_facts(workload: &str) -> Facts {
    Facts {
        workload: workload.to_string(),
        profile_complete: true,
        source_revision_matches: true,
        kvm_available: true,
        bug_found: true,
        verdict_schema_version: CURRENT_VERDICT_SCHEMA_VERSION,
        catalog_status: CatalogStatus::AcceptedV2,
        assertion_identity_matches: true,
        snapshot_codec: CURRENT_SNAPSHOT_CODEC.to_string(),
        snapshot_schema_version: CURRENT_SNAPSHOT_SCHEMA_VERSION,
        snapshot_reference_matches: true,
        replay_class: REQUIRED_REPLAY_CLASS.to_string(),
        reproduced: true,
        artifact_hashes_match: true,
        receipt_complete: true,
        claim_text: "Bounded proof for the recorded workload and artifact cohort.".to_string(),
    }
}

fn assert_blocked(facts: Facts, expected: Blocker) {
    let decision = classify(&facts);
    assert_eq!(decision.status, Status::Blocked);
    assert!(decision.blockers.contains(&expected));
}

#[test]
fn applies_one_positive_rule_to_all_workloads() {
    for workload in WORKLOADS {
        let decision = classify(&accepted_facts(workload));
        assert_eq!(decision.status, Status::PromotedBounded);
        assert!(decision.blockers.is_empty());
    }
}

#[test]
fn no_bug_is_a_valid_diagnostic_result() {
    let mut facts = accepted_facts(WORKLOADS[0]);
    facts.bug_found = false;
    facts.reproduced = false;
    facts.receipt_complete = false;
    let decision = classify(&facts);
    assert_eq!(decision.status, Status::DiagnosticNoBug);
    assert!(decision.blockers.is_empty());
}

#[test]
fn rejects_legacy_identity_and_schema() {
    let mut facts = accepted_facts(WORKLOADS[0]);
    facts.catalog_status = CatalogStatus::LegacyDiagnostic;
    facts.verdict_schema_version = LEGACY_SCHEMA_VERSION;
    let decision = classify(&facts);
    assert_eq!(decision.status, Status::Blocked);
    assert!(decision
        .blockers
        .contains(&Blocker::LegacyAssertionIdentity));
    assert!(decision.blockers.contains(&Blocker::LegacyVerdictSchema));
}

#[test]
fn rejects_stale_source_and_missing_kvm() {
    let mut facts = accepted_facts(WORKLOADS[1]);
    facts.source_revision_matches = false;
    facts.kvm_available = false;
    let decision = classify(&facts);
    assert_eq!(decision.status, Status::Blocked);
    assert!(decision.blockers.contains(&Blocker::StaleSourceRevision));
    assert!(decision.blockers.contains(&Blocker::MissingKvm));
}

#[test]
fn rejects_conflicting_or_mismatched_identity() {
    let mut facts = accepted_facts(WORKLOADS[2]);
    facts.catalog_status = CatalogStatus::Conflicting;
    facts.assertion_identity_matches = false;
    let decision = classify(&facts);
    assert_eq!(decision.status, Status::Blocked);
    assert!(decision
        .blockers
        .contains(&Blocker::ConflictingAssertionIdentity));
    assert!(decision
        .blockers
        .contains(&Blocker::AssertionIdentityMismatch));
}

#[test]
fn rejects_tampered_snapshot_and_artifacts() {
    let mut facts = accepted_facts(WORKLOADS[3]);
    facts.snapshot_reference_matches = false;
    facts.artifact_hashes_match = false;
    assert_blocked(facts, Blocker::ArtifactHashMismatch);
}

#[test]
fn rejects_incomplete_profile_and_receipt() {
    let mut facts = accepted_facts(WORKLOADS[0]);
    facts.profile_complete = false;
    facts.receipt_complete = false;
    let decision = classify(&facts);
    assert_eq!(decision.status, Status::Blocked);
    assert!(decision.blockers.contains(&Blocker::IncompleteProfile));
    assert!(decision.blockers.contains(&Blocker::IncompleteReceipt));
}

#[test]
fn rejects_universal_claim_promotion() {
    let mut facts = accepted_facts(WORKLOADS[0]);
    facts.claim_text = "This proves universal determinism.".to_string();
    assert_blocked(facts, Blocker::Overclaim);
}

#[test]
fn classification_is_deterministic() {
    let facts = accepted_facts(WORKLOADS[0]);
    assert_eq!(classify(&facts), classify(&facts));
}
