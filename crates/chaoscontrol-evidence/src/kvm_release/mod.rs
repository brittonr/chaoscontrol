//! Pure KVM release-matrix admission and receipt classification.
//!
//! This module accepts supplied facts and returns a deterministic decision. It
//! does not inspect Git, open KVM, read files, run commands, access a clock, or
//! publish artifacts. Those effects belong in the worker and checker binaries.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

pub const MATRIX_SCHEMA_VERSION: u32 = 1;
pub const RECEIPT_SCHEMA_VERSION: u32 = 1;
pub const REQUIRED_WORKER_ARCH: &str = "x86_64";

const BLAKE3_PREFIX: &str = "blake3:";
const BLAKE3_HEX_CHARS: usize = 64;
const MAX_MATRIX_ROWS: usize = 32;
const MAX_MATRIX_CAPABILITIES: usize = 16;
const MAX_COMMAND_ARGS: usize = 64;
const MAX_ARTIFACTS_PER_ROW: usize = 4_096;
const MAX_TIMEOUT_SECONDS: u64 = 7_200;
const MAX_RECEIPT_AGE_SECONDS: u64 = 604_800;
const REQUIRED_NON_CLAIM_FRAGMENTS: [&str; 5] = [
    "worker integrity",
    "all-host equivalence",
    "universal determinism",
    "workload correctness",
    "production availability",
];
const FORBIDDEN_CLAIM_FRAGMENTS: [&str; 5] = REQUIRED_NON_CLAIM_FRAGMENTS;
const REQUIRED_BASE_KINDS: [RowKind; 5] = [
    RowKind::DeterministicSmp,
    RowKind::SerializedSnapshotReplay,
    RowKind::VirtioMalformedInput,
    RowKind::AdmittedDrift,
    RowKind::FreshWorkloadReplay,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RowKind {
    CohortBinaryBuild,
    CohortGuestBuild,
    DeterministicSmp,
    SerializedSnapshotReplay,
    VirtioMalformedInput,
    AdmittedDrift,
    FreshWorkloadReplay,
    Pmu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RowStatus {
    Passed,
    Failed,
    Unsupported,
    Skipped,
    TimedOut,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseClass {
    ReleaseEligible,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blocker {
    InvalidMatrix,
    ReceiptSchemaMismatch,
    MatrixIdentityMismatch,
    SourceRevisionMismatch,
    DirtySource,
    ReceiptTimeInvalid,
    StaleReceipt,
    WorkerArchitectureMismatch,
    WorkerCapabilityMissing,
    MissingRow,
    UnexpectedRow,
    DuplicateRow,
    RowKindMismatch,
    RowCapabilityMismatch,
    CommandMismatch,
    CommandIdentityMismatch,
    RowTimeInvalid,
    RowTimedOut,
    RowNotPassed,
    ArtifactMissing,
    ArtifactCountExceeded,
    ArtifactBytesExceeded,
    ArtifactPathInvalid,
    ArtifactIdentityMalformed,
    ArtifactSetMismatch,
    ClaimBoundaryMismatch,
    Overclaim,
    TerminalClassMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowLimits {
    pub timeout_seconds: u64,
    pub max_artifacts: usize,
    pub max_artifact_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixRow {
    pub id: String,
    pub kind: RowKind,
    pub required: bool,
    pub required_capabilities: Vec<String>,
    pub command: CommandSpec,
    pub limits: RowLimits,
    pub retained_artifacts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvmReleaseMatrix {
    pub schema_version: u32,
    pub profile_id: String,
    pub required_worker_arch: String,
    pub required_worker_capabilities: Vec<String>,
    pub max_receipt_age_seconds: u64,
    pub claims_pmu_support: bool,
    pub bounded_claim: String,
    pub non_claims: Vec<String>,
    pub rows: Vec<MatrixRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFacts {
    pub revision: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerFacts {
    pub architecture: String,
    pub kernel_release: String,
    pub kvm_api_version: Option<i32>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ArtifactIdentity {
    pub path: String,
    pub bytes: u64,
    pub blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowReceipt {
    pub id: String,
    pub kind: RowKind,
    pub required_capabilities: Vec<String>,
    pub command: CommandSpec,
    pub executed_argv: Vec<String>,
    pub command_identity: String,
    pub started_unix_seconds: u64,
    pub finished_unix_seconds: u64,
    pub status: RowStatus,
    pub exit_code: Option<i32>,
    pub artifacts: Vec<ArtifactIdentity>,
    pub artifact_set_identity: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvmReleaseReceipt {
    pub schema_version: u32,
    pub matrix_profile: String,
    pub matrix_identity: String,
    pub source: SourceFacts,
    pub runner_revision: String,
    pub worker: WorkerFacts,
    pub started_unix_seconds: u64,
    pub finished_unix_seconds: u64,
    pub rows: Vec<RowReceipt>,
    pub bounded_claim: String,
    pub non_claims: Vec<String>,
    pub terminal_class: ReleaseClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvmReleaseDecision {
    pub terminal_class: ReleaseClass,
    pub blockers: Vec<Blocker>,
    pub reasons: Vec<String>,
}

// r[impl chaoscontrol.kvm_release_rail.matrix]
// r[impl chaoscontrol.kvm_release_rail.required_rows]
// r[impl chaoscontrol.kvm_release_rail.boundary]
pub fn validate_matrix(matrix: &KvmReleaseMatrix) -> Result<(), Vec<String>> {
    let mut issues = Vec::new();
    push_unless(
        matrix.schema_version == MATRIX_SCHEMA_VERSION,
        &mut issues,
        "matrix schema version is not supported",
    );
    push_unless(
        !matrix.profile_id.is_empty(),
        &mut issues,
        "matrix profile id is empty",
    );
    push_unless(
        matrix.required_worker_arch == REQUIRED_WORKER_ARCH,
        &mut issues,
        "matrix worker architecture is not the admitted architecture",
    );
    push_unless(
        !matrix.required_worker_capabilities.is_empty()
            && matrix.required_worker_capabilities.len() <= MAX_MATRIX_CAPABILITIES,
        &mut issues,
        "matrix worker capability set is empty or exceeds its bound",
    );
    push_unless(
        matrix.max_receipt_age_seconds > 0
            && matrix.max_receipt_age_seconds <= MAX_RECEIPT_AGE_SECONDS,
        &mut issues,
        "matrix receipt freshness bound is invalid",
    );
    push_unless(
        !matrix.rows.is_empty() && matrix.rows.len() <= MAX_MATRIX_ROWS,
        &mut issues,
        "matrix row count is empty or exceeds its bound",
    );
    push_unless(
        !matrix.bounded_claim.is_empty() && !contains_overclaim(&matrix.bounded_claim),
        &mut issues,
        "matrix bounded claim is empty or overclaims",
    );
    validate_non_claims(&matrix.non_claims, &mut issues);

    let mut ids = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    for row in &matrix.rows {
        validate_matrix_row(row, &mut ids, &mut kinds, &mut issues);
    }
    for kind in REQUIRED_BASE_KINDS {
        push_unless(
            matrix
                .rows
                .iter()
                .any(|row| row.kind == kind && row.required),
            &mut issues,
            format!("matrix lacks required row kind: {kind:?}"),
        );
    }
    if matrix.claims_pmu_support {
        push_unless(
            matrix
                .rows
                .iter()
                .any(|row| row.kind == RowKind::Pmu && row.required),
            &mut issues,
            "matrix claims PMU support without a required PMU row",
        );
    }

    issues.sort();
    issues.dedup();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

fn validate_matrix_row(
    row: &MatrixRow,
    ids: &mut BTreeSet<String>,
    kinds: &mut BTreeSet<RowKind>,
    issues: &mut Vec<String>,
) {
    push_unless(!row.id.is_empty(), issues, "matrix row id is empty");
    push_unless(
        ids.insert(row.id.clone()),
        issues,
        format!("matrix row id is duplicated: {}", row.id),
    );
    push_unless(
        kinds.insert(row.kind),
        issues,
        format!("matrix row kind is duplicated: {:?}", row.kind),
    );
    push_unless(
        row.required,
        issues,
        format!("matrix row is optional: {}", row.id),
    );
    push_unless(
        !row.required_capabilities.is_empty()
            && row.required_capabilities.len() <= MAX_MATRIX_CAPABILITIES,
        issues,
        format!("matrix row capabilities are invalid: {}", row.id),
    );
    push_unless(
        !row.command.program.is_empty()
            && !row.command.args.is_empty()
            && row.command.args.len() <= MAX_COMMAND_ARGS
            && row.command.args.iter().all(|arg| !arg.is_empty()),
        issues,
        format!("matrix row command is invalid: {}", row.id),
    );
    push_unless(
        row.limits.timeout_seconds > 0 && row.limits.timeout_seconds <= MAX_TIMEOUT_SECONDS,
        issues,
        format!("matrix row timeout is invalid: {}", row.id),
    );
    push_unless(
        row.limits.max_artifacts > 0 && row.limits.max_artifacts <= MAX_ARTIFACTS_PER_ROW,
        issues,
        format!("matrix row artifact count bound is invalid: {}", row.id),
    );
    push_unless(
        row.limits.max_artifact_bytes > 0,
        issues,
        format!("matrix row artifact byte bound is invalid: {}", row.id),
    );
    push_unless(
        !row.retained_artifacts.is_empty(),
        issues,
        format!("matrix row retained artifact policy is empty: {}", row.id),
    );
}

fn validate_non_claims(non_claims: &[String], issues: &mut Vec<String>) {
    let normalized = non_claims
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    for fragment in REQUIRED_NON_CLAIM_FRAGMENTS {
        push_unless(
            normalized.iter().any(|value| value.contains(fragment)),
            issues,
            format!("matrix lacks required non-claim: {fragment}"),
        );
    }
}

// r[impl chaoscontrol.kvm_release_rail.functional_core]
// r[impl chaoscontrol.kvm_release_rail.receipt]
// r[impl chaoscontrol.kvm_release_rail.validation]
pub fn classify(
    matrix: &KvmReleaseMatrix,
    expected_source_revision: &str,
    receipt: &KvmReleaseReceipt,
    now_unix_seconds: u64,
) -> KvmReleaseDecision {
    let mut blockers = Vec::new();
    let mut reasons = Vec::new();

    if let Err(matrix_issues) = validate_matrix(matrix) {
        blockers.push(Blocker::InvalidMatrix);
        reasons.extend(matrix_issues);
    }
    classify_receipt_header(
        matrix,
        expected_source_revision,
        receipt,
        now_unix_seconds,
        &mut blockers,
        &mut reasons,
    );
    classify_rows(matrix, receipt, &mut blockers, &mut reasons);

    blockers.sort();
    blockers.dedup();
    reasons.sort();
    reasons.dedup();
    let terminal_class = if blockers.is_empty() {
        ReleaseClass::ReleaseEligible
    } else {
        ReleaseClass::Blocked
    };
    KvmReleaseDecision {
        terminal_class,
        blockers,
        reasons,
    }
}

pub fn validate_receipt(
    matrix: &KvmReleaseMatrix,
    expected_source_revision: &str,
    receipt: &KvmReleaseReceipt,
    now_unix_seconds: u64,
) -> KvmReleaseDecision {
    let mut decision = classify(matrix, expected_source_revision, receipt, now_unix_seconds);
    if receipt.terminal_class != decision.terminal_class {
        decision.blockers.push(Blocker::TerminalClassMismatch);
        decision.reasons.push(format!(
            "declared terminal class {:?} differs from computed class {:?}",
            receipt.terminal_class, decision.terminal_class
        ));
        decision.terminal_class = ReleaseClass::Blocked;
        decision.blockers.sort();
        decision.blockers.dedup();
        decision.reasons.sort();
        decision.reasons.dedup();
    }
    decision
}

fn classify_receipt_header(
    matrix: &KvmReleaseMatrix,
    expected_source_revision: &str,
    receipt: &KvmReleaseReceipt,
    now_unix_seconds: u64,
    blockers: &mut Vec<Blocker>,
    reasons: &mut Vec<String>,
) {
    add_blocker_unless(
        receipt.schema_version == RECEIPT_SCHEMA_VERSION,
        Blocker::ReceiptSchemaMismatch,
        "receipt schema version is not supported",
        blockers,
        reasons,
    );
    add_blocker_unless(
        receipt.matrix_profile == matrix.profile_id
            && receipt.matrix_identity == matrix_identity(matrix),
        Blocker::MatrixIdentityMismatch,
        "receipt matrix identity does not match the selected matrix",
        blockers,
        reasons,
    );
    add_blocker_unless(
        receipt.source.revision == expected_source_revision,
        Blocker::SourceRevisionMismatch,
        "receipt source revision does not match the selected revision",
        blockers,
        reasons,
    );
    add_blocker_unless(
        !receipt.source.dirty,
        Blocker::DirtySource,
        "receipt source worktree was dirty",
        blockers,
        reasons,
    );
    let receipt_time_valid = receipt.started_unix_seconds <= receipt.finished_unix_seconds
        && receipt.finished_unix_seconds <= now_unix_seconds;
    add_blocker_unless(
        receipt_time_valid,
        Blocker::ReceiptTimeInvalid,
        "receipt timestamps are inverted or in the future",
        blockers,
        reasons,
    );
    if receipt_time_valid {
        add_blocker_unless(
            now_unix_seconds - receipt.finished_unix_seconds <= matrix.max_receipt_age_seconds,
            Blocker::StaleReceipt,
            "receipt exceeds the selected freshness bound",
            blockers,
            reasons,
        );
    }
    add_blocker_unless(
        receipt.worker.architecture == matrix.required_worker_arch,
        Blocker::WorkerArchitectureMismatch,
        "worker architecture does not match the matrix",
        blockers,
        reasons,
    );
    for capability in &matrix.required_worker_capabilities {
        add_blocker_unless(
            receipt.worker.capabilities.contains(capability),
            Blocker::WorkerCapabilityMissing,
            format!("worker lacks required capability: {capability}"),
            blockers,
            reasons,
        );
    }
    let boundary_matches =
        receipt.bounded_claim == matrix.bounded_claim && receipt.non_claims == matrix.non_claims;
    add_blocker_unless(
        boundary_matches,
        Blocker::ClaimBoundaryMismatch,
        "receipt claim boundary differs from the matrix",
        blockers,
        reasons,
    );
    add_blocker_unless(
        !contains_overclaim(&receipt.bounded_claim),
        Blocker::Overclaim,
        "receipt bounded claim contains a prohibited claim",
        blockers,
        reasons,
    );
}

fn classify_rows(
    matrix: &KvmReleaseMatrix,
    receipt: &KvmReleaseReceipt,
    blockers: &mut Vec<Blocker>,
    reasons: &mut Vec<String>,
) {
    let mut receipt_rows = BTreeMap::new();
    for row in &receipt.rows {
        if receipt_rows.insert(row.id.as_str(), row).is_some() {
            blockers.push(Blocker::DuplicateRow);
            reasons.push(format!("receipt row is duplicated: {}", row.id));
        }
    }

    for matrix_row in &matrix.rows {
        let Some(receipt_row) = receipt_rows.get(matrix_row.id.as_str()) else {
            blockers.push(Blocker::MissingRow);
            reasons.push(format!("receipt lacks required row: {}", matrix_row.id));
            continue;
        };
        classify_row(matrix_row, receipt, receipt_row, blockers, reasons);
    }
    for receipt_row in &receipt.rows {
        if !matrix.rows.iter().any(|row| row.id == receipt_row.id) {
            blockers.push(Blocker::UnexpectedRow);
            reasons.push(format!(
                "receipt contains unexpected row: {}",
                receipt_row.id
            ));
        }
    }
}

fn classify_row(
    matrix_row: &MatrixRow,
    receipt: &KvmReleaseReceipt,
    receipt_row: &RowReceipt,
    blockers: &mut Vec<Blocker>,
    reasons: &mut Vec<String>,
) {
    add_blocker_unless(
        receipt_row.kind == matrix_row.kind,
        Blocker::RowKindMismatch,
        format!("row kind differs: {}", matrix_row.id),
        blockers,
        reasons,
    );
    add_blocker_unless(
        receipt_row.required_capabilities == matrix_row.required_capabilities,
        Blocker::RowCapabilityMismatch,
        format!("row capability predicate differs: {}", matrix_row.id),
        blockers,
        reasons,
    );
    add_blocker_unless(
        receipt_row.command == matrix_row.command,
        Blocker::CommandMismatch,
        format!("row command differs: {}", matrix_row.id),
        blockers,
        reasons,
    );
    add_blocker_unless(
        receipt_row.command_identity == command_identity(&matrix_row.command),
        Blocker::CommandIdentityMismatch,
        format!("row command identity differs: {}", matrix_row.id),
        blockers,
        reasons,
    );
    let row_time_valid = receipt.started_unix_seconds <= receipt_row.started_unix_seconds
        && receipt_row.started_unix_seconds <= receipt_row.finished_unix_seconds
        && receipt_row.finished_unix_seconds <= receipt.finished_unix_seconds;
    add_blocker_unless(
        row_time_valid,
        Blocker::RowTimeInvalid,
        format!("row timestamps are outside the receipt: {}", matrix_row.id),
        blockers,
        reasons,
    );
    if row_time_valid {
        add_blocker_unless(
            receipt_row.finished_unix_seconds - receipt_row.started_unix_seconds
                <= matrix_row.limits.timeout_seconds,
            Blocker::RowTimedOut,
            format!("row exceeded its timeout: {}", matrix_row.id),
            blockers,
            reasons,
        );
    }
    add_blocker_unless(
        receipt_row.status == RowStatus::Passed,
        Blocker::RowNotPassed,
        format!(
            "required row did not pass: {} ({:?})",
            matrix_row.id, receipt_row.status
        ),
        blockers,
        reasons,
    );
    validate_row_artifacts(matrix_row, receipt_row, blockers, reasons);
}

fn validate_row_artifacts(
    matrix_row: &MatrixRow,
    receipt_row: &RowReceipt,
    blockers: &mut Vec<Blocker>,
    reasons: &mut Vec<String>,
) {
    add_blocker_unless(
        !receipt_row.artifacts.is_empty(),
        Blocker::ArtifactMissing,
        format!("row retained no artifacts: {}", matrix_row.id),
        blockers,
        reasons,
    );
    add_blocker_unless(
        receipt_row.artifacts.len() <= matrix_row.limits.max_artifacts,
        Blocker::ArtifactCountExceeded,
        format!("row artifact count exceeds its bound: {}", matrix_row.id),
        blockers,
        reasons,
    );
    let mut total_bytes = 0_u64;
    let mut paths = BTreeSet::new();
    for artifact in &receipt_row.artifacts {
        let bytes_added = total_bytes.checked_add(artifact.bytes);
        if let Some(next_total) = bytes_added {
            total_bytes = next_total;
        } else {
            blockers.push(Blocker::ArtifactBytesExceeded);
            reasons.push(format!(
                "row artifact byte count overflowed: {}",
                matrix_row.id
            ));
        }
        add_blocker_unless(
            valid_relative_path(&artifact.path) && paths.insert(artifact.path.as_str()),
            Blocker::ArtifactPathInvalid,
            format!(
                "row artifact path is invalid or duplicated: {}",
                matrix_row.id
            ),
            blockers,
            reasons,
        );
        add_blocker_unless(
            valid_blake3(&artifact.blake3),
            Blocker::ArtifactIdentityMalformed,
            format!("row artifact identity is malformed: {}", matrix_row.id),
            blockers,
            reasons,
        );
    }
    add_blocker_unless(
        total_bytes <= matrix_row.limits.max_artifact_bytes,
        Blocker::ArtifactBytesExceeded,
        format!("row artifact bytes exceed the bound: {}", matrix_row.id),
        blockers,
        reasons,
    );
    add_blocker_unless(
        receipt_row.artifact_set_identity == artifact_set_identity(&receipt_row.artifacts),
        Blocker::ArtifactSetMismatch,
        format!("row artifact set identity differs: {}", matrix_row.id),
        blockers,
        reasons,
    );
}

pub fn matrix_identity(matrix: &KvmReleaseMatrix) -> String {
    json_identity(matrix)
}

pub fn command_identity(command: &CommandSpec) -> String {
    json_identity(command)
}

pub fn artifact_set_identity(artifacts: &[ArtifactIdentity]) -> String {
    let mut ordered = artifacts.to_vec();
    ordered.sort();
    json_identity(&ordered)
}

fn json_identity<T: Serialize>(value: &T) -> String {
    let encoded = serde_json::to_vec(value).expect("serializing typed identity input cannot fail");
    format!("{BLAKE3_PREFIX}{}", blake3::hash(&encoded).to_hex())
}

fn contains_overclaim(claim: &str) -> bool {
    let normalized = claim.to_ascii_lowercase();
    FORBIDDEN_CLAIM_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
}

fn valid_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn valid_blake3(value: &str) -> bool {
    value.strip_prefix(BLAKE3_PREFIX).is_some_and(|hex| {
        hex.len() == BLAKE3_HEX_CHARS
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn push_unless(condition: bool, issues: &mut Vec<String>, message: impl Into<String>) {
    if !condition {
        issues.push(message.into());
    }
}

fn add_blocker_unless(
    condition: bool,
    blocker: Blocker,
    reason: impl Into<String>,
    blockers: &mut Vec<Blocker>,
    reasons: &mut Vec<String>,
) {
    if !condition {
        blockers.push(blocker);
        reasons.push(reason.into());
    }
}

#[cfg(test)]
mod tests;
