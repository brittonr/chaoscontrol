use serde::{Deserialize, Serialize};

use crate::{validate_parity_report, ParityReport};

/// Immutable VM Cohort source selected by this consumer.
pub const VM_COHORT_REVISION: &str = "ab123e3673b6dd616b3df5d044026b5e85755149";

const SELECTION_NON_CLAIMS: &[&str] = &[
    "behavioral parity does not prove either implementation correct",
    "KVM smoke does not prove guest correctness or universal determinism",
    "VM Cohort receipts do not grant fault, replay, evidence, or release authority",
    "cleanup observations do not prove data erasure",
    "the diagnostic rollback path is not a supported release fallback",
];

/// Bounded status for one required consumer verification case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    /// The named bounded case passed.
    Passed,
    /// The named bounded case failed.
    Failed,
    /// The named bounded case did not establish an outcome.
    Unknown,
}

/// Evidence required before shared mechanics can be selected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismSelectionEvidence {
    /// Exact VM Cohort source revision.
    pub source_revision: String,
    /// Complete bounded legacy/shared parity report.
    pub parity: ParityReport,
    /// Exact snapshot mapping case status.
    pub mapping: VerificationStatus,
    /// Exact vCPU and in-kernel device restore case status.
    pub exact_restore: VerificationStatus,
    /// Partial-creation cleanup case status.
    pub partial_creation_cleanup: VerificationStatus,
    /// Unknown-cleanup preservation case status.
    pub cleanup_uncertainty: VerificationStatus,
    /// Live KVM smoke case status.
    pub kvm_smoke: VerificationStatus,
    /// Whether a consumer policy type leaked into VM Cohort.
    pub consumer_policy_leak_detected: bool,
}

/// Supported cohort mechanism after bounded consumer verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectedMechanism {
    /// Product-neutral VM Cohort shared mechanics.
    VmCohort,
}

/// Explicit status of the old duplicate mechanism.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LegacyMechanismStatus {
    /// Manual diagnosis only; never an automatic or release fallback.
    DiagnosticRollbackOnly,
}

/// Fail-closed shared-mechanism selection record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismSelectionRecord {
    /// Exact selected source revision.
    pub source_revision: String,
    /// Supported shared mechanism.
    pub selected: SelectedMechanism,
    /// Old duplicate path status.
    pub legacy_status: LegacyMechanismStatus,
    /// A mechanism receipt did not grant fault authority.
    pub fault_authority_granted: bool,
    /// A mechanism receipt did not grant replay authority.
    pub replay_authority_granted: bool,
    /// A mechanism receipt did not grant evidence authority.
    pub evidence_authority_granted: bool,
    /// A mechanism receipt did not grant release authority.
    pub release_authority_granted: bool,
    /// Required bounded non-claims.
    pub non_claims: Vec<String>,
}

/// Closed reason why shared-mechanism selection was denied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionIssue {
    /// Cargo, Nix, or evidence selected a different source.
    SourceDrift,
    /// One required parity row failed or a parity overclaim was present.
    Parity,
    /// One required verification case failed or remained unknown.
    Verification,
    /// A ChaosControl policy type leaked into the product-neutral mechanism.
    PolicyLeak,
}

/// Selects VM Cohort only after every bounded positive and negative case passes.
///
/// # Errors
///
/// Returns a closed issue for source drift, parity failure, incomplete verification,
/// or consumer-policy leakage.
// r[impl chaoscontrol.vm_cohort.selection]
pub fn select_shared_mechanism(
    evidence: &MechanismSelectionEvidence,
) -> Result<MechanismSelectionRecord, SelectionIssue> {
    if evidence.source_revision != VM_COHORT_REVISION {
        return Err(SelectionIssue::SourceDrift);
    }
    if !validate_parity_report(&evidence.parity) {
        return Err(SelectionIssue::Parity);
    }
    let statuses = [
        evidence.mapping,
        evidence.exact_restore,
        evidence.partial_creation_cleanup,
        evidence.cleanup_uncertainty,
        evidence.kvm_smoke,
    ];
    if !statuses
        .iter()
        .all(|status| *status == VerificationStatus::Passed)
    {
        return Err(SelectionIssue::Verification);
    }
    if evidence.consumer_policy_leak_detected {
        return Err(SelectionIssue::PolicyLeak);
    }
    Ok(MechanismSelectionRecord {
        source_revision: VM_COHORT_REVISION.to_string(),
        selected: SelectedMechanism::VmCohort,
        legacy_status: LegacyMechanismStatus::DiagnosticRollbackOnly,
        fault_authority_granted: false,
        replay_authority_granted: false,
        evidence_authority_granted: false,
        release_authority_granted: false,
        non_claims: SELECTION_NON_CLAIMS
            .iter()
            .map(ToString::to_string)
            .collect(),
    })
}

/// Validates a persisted selection record and every authority boundary.
#[must_use]
pub fn validate_selection_record(record: &MechanismSelectionRecord) -> bool {
    record.source_revision == VM_COHORT_REVISION
        && record.selected == SelectedMechanism::VmCohort
        && record.legacy_status == LegacyMechanismStatus::DiagnosticRollbackOnly
        && !record.fault_authority_granted
        && !record.replay_authority_granted
        && !record.evidence_authority_granted
        && !record.release_authority_granted
        && record.non_claims == SELECTION_NON_CLAIMS
}
