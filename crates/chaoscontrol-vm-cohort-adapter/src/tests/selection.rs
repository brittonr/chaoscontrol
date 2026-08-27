use crate::{
    run_parity_corpus, select_shared_mechanism, validate_selection_record,
    MechanismSelectionEvidence, SelectionIssue, VerificationStatus, VM_COHORT_REVISION,
};

// r[verify chaoscontrol.vm_cohort.selection]
#[test]
fn complete_consumer_evidence_selects_shared_mechanics_without_authority_transfer() {
    let evidence = passing_evidence();
    let record = select_shared_mechanism(&evidence).expect("select VM Cohort mechanics");
    assert!(validate_selection_record(&record));
    assert!(!record.fault_authority_granted);
    assert!(!record.replay_authority_granted);
    assert!(!record.evidence_authority_granted);
    assert!(!record.release_authority_granted);
}

// r[verify chaoscontrol.vm_cohort.verification]
#[test]
fn source_drift_unknown_cleanup_policy_leak_and_parity_overclaim_block_selection() {
    let mut drifted = passing_evidence();
    drifted.source_revision = "0000000000000000000000000000000000000000".to_string();
    assert_eq!(
        select_shared_mechanism(&drifted).expect_err("source drift must fail"),
        SelectionIssue::SourceDrift
    );

    let mut unknown_cleanup = passing_evidence();
    unknown_cleanup.cleanup_uncertainty = VerificationStatus::Unknown;
    assert_eq!(
        select_shared_mechanism(&unknown_cleanup).expect_err("unknown cleanup must fail"),
        SelectionIssue::Verification
    );

    let mut leaked = passing_evidence();
    leaked.consumer_policy_leak_detected = true;
    assert_eq!(
        select_shared_mechanism(&leaked).expect_err("policy leak must fail"),
        SelectionIssue::PolicyLeak
    );

    let mut overclaim = passing_evidence();
    overclaim.parity.product_authority_granted = true;
    assert_eq!(
        select_shared_mechanism(&overclaim).expect_err("parity overclaim must fail"),
        SelectionIssue::Parity
    );
}

// r[verify chaoscontrol.vm_cohort.authority]
#[test]
fn persisted_selection_rejects_later_authority_escalation() {
    let mut record = select_shared_mechanism(&passing_evidence()).expect("selection record");
    record.release_authority_granted = true;
    assert!(!validate_selection_record(&record));
}

pub fn passing_evidence() -> MechanismSelectionEvidence {
    MechanismSelectionEvidence {
        source_revision: VM_COHORT_REVISION.to_string(),
        parity: run_parity_corpus().expect("parity corpus"),
        mapping: VerificationStatus::Passed,
        exact_restore: VerificationStatus::Passed,
        partial_creation_cleanup: VerificationStatus::Passed,
        cleanup_uncertainty: VerificationStatus::Passed,
        kvm_smoke: VerificationStatus::Passed,
        consumer_policy_leak_detected: false,
    }
}
