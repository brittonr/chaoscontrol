//! Pure admission logic for fresh workload proof promotion.
//!
//! Callers load files and observe KVM in an imperative shell. This module only
//! classifies supplied facts. It does not read files, inspect the host, run a
//! guest, access a clock, or publish artifacts.

pub const CURRENT_VERDICT_SCHEMA_VERSION: u32 = 2;
pub const CURRENT_SNAPSHOT_SCHEMA_VERSION: u32 = 2;
pub const CURRENT_SNAPSHOT_CODEC: &str = "simulation-snapshot-cbor-zstd-v2";
pub const REQUIRED_REPLAY_CLASS: &str = "snapshot_backed_reproduced";

const FORBIDDEN_CLAIM_FRAGMENTS: [&str; 4] = [
    "workload correctness",
    "universal determinism",
    "host equivalence",
    "release eligible",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogStatus {
    AcceptedV2,
    LegacyDiagnostic,
    Conflicting,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Facts {
    pub workload: String,
    pub profile_complete: bool,
    pub source_revision_matches: bool,
    pub kvm_available: bool,
    pub bug_found: bool,
    pub verdict_schema_version: u32,
    pub catalog_status: CatalogStatus,
    pub assertion_identity_matches: bool,
    pub snapshot_codec: String,
    pub snapshot_schema_version: u32,
    pub snapshot_reference_matches: bool,
    pub replay_class: String,
    pub reproduced: bool,
    pub artifact_hashes_match: bool,
    pub receipt_complete: bool,
    pub claim_text: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Blocker {
    IncompleteProfile,
    StaleSourceRevision,
    MissingKvm,
    LegacyAssertionIdentity,
    ConflictingAssertionIdentity,
    AssertionIdentityMismatch,
    LegacyVerdictSchema,
    SnapshotCodecMismatch,
    SnapshotSchemaMismatch,
    SnapshotReferenceMismatch,
    ReplayClassMismatch,
    ReplayDidNotReproduce,
    ArtifactHashMismatch,
    IncompleteReceipt,
    Overclaim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    PromotedBounded,
    DiagnosticNoBug,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Decision {
    pub workload: String,
    pub status: Status,
    pub blockers: Vec<Blocker>,
}

// r[impl chaoscontrol.fresh_workload_proofs.admission]
// r[impl chaoscontrol.fresh_workload_proofs.functional_core]
// r[impl chaoscontrol.fresh_workload_proofs.boundary]
// r[impl chaoscontrol.fresh_workload_proofs.onboarding]
pub fn classify(facts: &Facts) -> Decision {
    let mut blockers = preflight_blockers(facts);
    if facts.bug_found {
        append_replay_blockers(facts, &mut blockers);
    }
    blockers.sort();
    blockers.dedup();

    let status = if !blockers.is_empty() {
        Status::Blocked
    } else if facts.bug_found {
        Status::PromotedBounded
    } else {
        Status::DiagnosticNoBug
    };
    Decision {
        workload: facts.workload.clone(),
        status,
        blockers,
    }
}

fn preflight_blockers(facts: &Facts) -> Vec<Blocker> {
    let mut blockers = Vec::new();
    if !facts.profile_complete {
        blockers.push(Blocker::IncompleteProfile);
    }
    if !facts.source_revision_matches {
        blockers.push(Blocker::StaleSourceRevision);
    }
    if !facts.kvm_available {
        blockers.push(Blocker::MissingKvm);
    }
    if contains_overclaim(&facts.claim_text) {
        blockers.push(Blocker::Overclaim);
    }
    blockers
}

fn append_replay_blockers(facts: &Facts, blockers: &mut Vec<Blocker>) {
    match facts.catalog_status {
        CatalogStatus::AcceptedV2 => {}
        CatalogStatus::LegacyDiagnostic => blockers.push(Blocker::LegacyAssertionIdentity),
        CatalogStatus::Conflicting => blockers.push(Blocker::ConflictingAssertionIdentity),
    }
    if !facts.assertion_identity_matches {
        blockers.push(Blocker::AssertionIdentityMismatch);
    }
    if facts.verdict_schema_version != CURRENT_VERDICT_SCHEMA_VERSION {
        blockers.push(Blocker::LegacyVerdictSchema);
    }
    if facts.snapshot_codec != CURRENT_SNAPSHOT_CODEC {
        blockers.push(Blocker::SnapshotCodecMismatch);
    }
    if facts.snapshot_schema_version != CURRENT_SNAPSHOT_SCHEMA_VERSION {
        blockers.push(Blocker::SnapshotSchemaMismatch);
    }
    if !facts.snapshot_reference_matches {
        blockers.push(Blocker::SnapshotReferenceMismatch);
    }
    if facts.replay_class != REQUIRED_REPLAY_CLASS {
        blockers.push(Blocker::ReplayClassMismatch);
    }
    if !facts.reproduced {
        blockers.push(Blocker::ReplayDidNotReproduce);
    }
    if !facts.artifact_hashes_match {
        blockers.push(Blocker::ArtifactHashMismatch);
    }
    if !facts.receipt_complete {
        blockers.push(Blocker::IncompleteReceipt);
    }
}

fn contains_overclaim(claim_text: &str) -> bool {
    let normalized = claim_text.to_ascii_lowercase();
    FORBIDDEN_CLAIM_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
}

#[cfg(test)]
mod tests;
