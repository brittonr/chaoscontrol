use super::*;

const FIXTURE_REVISION: &str = "fixture-revision";
const STARTED_UNIX_SECONDS: u64 = 100;
const FINISHED_UNIX_SECONDS: u64 = 101;
const RECEIPT_AGE_SECONDS: u64 = 60;
const ROW_TIMEOUT_SECONDS: u64 = 10;
const ROW_ARTIFACT_LIMIT: usize = 8;
const ROW_ARTIFACT_BYTES: u64 = 1_024;
const EMPTY_BLAKE3: &str =
    "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

fn sample_matrix() -> KvmReleaseMatrix {
    let row_kinds = [
        ("build-binaries", RowKind::CohortBinaryBuild),
        ("build-guest", RowKind::CohortGuestBuild),
        ("deterministic-smp", RowKind::DeterministicSmp),
        ("serialized-snapshot", RowKind::SerializedSnapshotReplay),
        ("virtio-malformed-input", RowKind::VirtioMalformedInput),
        ("admitted-drift", RowKind::AdmittedDrift),
        ("fresh-workload-replay", RowKind::FreshWorkloadReplay),
    ];
    KvmReleaseMatrix {
        schema_version: MATRIX_SCHEMA_VERSION,
        profile_id: "fixture-kvm-release".to_string(),
        required_worker_arch: REQUIRED_WORKER_ARCH.to_string(),
        required_worker_capabilities: vec!["kvm".to_string(), "x86_64-linux".to_string()],
        max_receipt_age_seconds: RECEIPT_AGE_SECONDS,
        claims_pmu_support: false,
        bounded_claim:
            "This receipt covers the exact source, matrix, worker, and retained row artifacts."
                .to_string(),
        non_claims: vec![
            "This receipt does not prove worker integrity.".to_string(),
            "This receipt does not prove all-host equivalence.".to_string(),
            "This receipt does not prove universal determinism.".to_string(),
            "This receipt does not prove workload correctness.".to_string(),
            "This receipt does not prove production availability.".to_string(),
        ],
        rows: row_kinds
            .into_iter()
            .map(|(id, kind)| MatrixRow {
                id: id.to_string(),
                kind,
                required: true,
                required_capabilities: vec!["kvm".to_string()],
                command: CommandSpec {
                    program: "true".to_string(),
                    args: vec!["--version".to_string()],
                },
                limits: RowLimits {
                    timeout_seconds: ROW_TIMEOUT_SECONDS,
                    max_artifacts: ROW_ARTIFACT_LIMIT,
                    max_artifact_bytes: ROW_ARTIFACT_BYTES,
                },
                retained_artifacts: vec!["stdout.log".to_string(), "stderr.log".to_string()],
            })
            .collect(),
    }
}

fn sample_receipt(matrix: &KvmReleaseMatrix) -> KvmReleaseReceipt {
    let rows = matrix
        .rows
        .iter()
        .map(|row| {
            let artifacts = vec![ArtifactIdentity {
                path: "stdout.log".to_string(),
                bytes: 0,
                blake3: EMPTY_BLAKE3.to_string(),
            }];
            RowReceipt {
                id: row.id.clone(),
                kind: row.kind,
                required_capabilities: row.required_capabilities.clone(),
                command: row.command.clone(),
                executed_argv: vec![row.command.program.clone(), row.command.args[0].clone()],
                command_identity: command_identity(&row.command),
                started_unix_seconds: STARTED_UNIX_SECONDS,
                finished_unix_seconds: FINISHED_UNIX_SECONDS,
                status: RowStatus::Passed,
                exit_code: Some(0),
                artifact_set_identity: artifact_set_identity(&artifacts),
                artifacts,
                notes: Vec::new(),
            }
        })
        .collect();
    KvmReleaseReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        matrix_profile: matrix.profile_id.clone(),
        matrix_identity: matrix_identity(matrix),
        source: SourceFacts {
            revision: FIXTURE_REVISION.to_string(),
            dirty: false,
        },
        runner_revision: "fixture-runner".to_string(),
        worker: WorkerFacts {
            architecture: REQUIRED_WORKER_ARCH.to_string(),
            kernel_release: "fixture-kernel".to_string(),
            kvm_api_version: Some(12),
            capabilities: matrix.required_worker_capabilities.clone(),
        },
        started_unix_seconds: STARTED_UNIX_SECONDS,
        finished_unix_seconds: FINISHED_UNIX_SECONDS,
        rows,
        bounded_claim: matrix.bounded_claim.clone(),
        non_claims: matrix.non_claims.clone(),
        terminal_class: ReleaseClass::ReleaseEligible,
    }
}

fn assert_blocked(receipt: &KvmReleaseReceipt, blocker: Blocker, now: u64) {
    let matrix = sample_matrix();
    let decision = validate_receipt(&matrix, FIXTURE_REVISION, receipt, now);
    assert_eq!(decision.terminal_class, ReleaseClass::Blocked);
    assert!(decision.blockers.contains(&blocker));
    assert!(!decision.reasons.is_empty());
}

#[test]
fn complete_matrix_is_release_eligible_and_deterministic() {
    let matrix = sample_matrix();
    let receipt = sample_receipt(&matrix);
    let first = validate_receipt(&matrix, FIXTURE_REVISION, &receipt, FINISHED_UNIX_SECONDS);
    let second = validate_receipt(&matrix, FIXTURE_REVISION, &receipt, FINISHED_UNIX_SECONDS);

    assert_eq!(first, second);
    assert_eq!(first.terminal_class, ReleaseClass::ReleaseEligible);
    assert!(first.blockers.is_empty());
    assert!(first.reasons.is_empty());
}

#[test]
fn repository_matrix_fixture_satisfies_validation() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/kvm-release/matrix.json");
    let bytes = std::fs::read(&path).expect("repository matrix fixture");
    let matrix: KvmReleaseMatrix = serde_json::from_slice(&bytes).expect("matrix JSON");
    validate_matrix(&matrix).expect("repository matrix passes admission");
}

#[test]
fn matrix_requires_base_rows_bounds_and_pmu_evidence() {
    let mut matrix = sample_matrix();
    matrix
        .rows
        .retain(|row| row.kind != RowKind::FreshWorkloadReplay);
    assert!(validate_matrix(&matrix).is_err());

    let mut matrix = sample_matrix();
    matrix.rows[0].limits.timeout_seconds = 0;
    assert!(validate_matrix(&matrix).is_err());

    let mut matrix = sample_matrix();
    matrix.claims_pmu_support = true;
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn matrix_rejects_unsafe_row_ids() {
    for id in ["../escape", "nested/row", ".", "..", "bad\\row"] {
        let mut matrix = sample_matrix();
        matrix.rows[0].id = id.to_string();
        assert!(
            validate_matrix(&matrix).is_err(),
            "row id should be rejected: {id}"
        );
    }

    let mut matrix = sample_matrix();
    matrix.rows[0].id = "x".repeat(MAX_MATRIX_ROW_ID_BYTES + 1);
    assert!(validate_matrix(&matrix).is_err());

    let mut matrix = sample_matrix();
    matrix.rows[0].id = "x".repeat(MAX_MATRIX_ROW_ID_BYTES);
    validate_matrix(&matrix).expect("row id at the byte bound is admitted");
}

#[test]
fn matrix_accepts_claim_text_at_the_bound() {
    let mut matrix = sample_matrix();
    matrix.bounded_claim = "x".repeat(MAX_MATRIX_CLAIM_BYTES);
    validate_matrix(&matrix).expect("claim at the byte bound is admitted");
}

#[test]
fn matrix_accepts_command_values_at_the_bound() {
    let mut matrix = sample_matrix();
    matrix.rows[0].command.program = "p".repeat(MAX_COMMAND_BYTES);
    matrix.rows[0].command.args = vec!["a".repeat(MAX_COMMAND_BYTES)];
    validate_matrix(&matrix).expect("command values at the byte bound are admitted");
}

#[test]
fn matrix_rejects_malformed_commands() {
    let mut matrix = sample_matrix();
    matrix.rows[0].command.program = "bad\0program".to_string();
    assert!(validate_matrix(&matrix).is_err());

    let mut matrix = sample_matrix();
    matrix.rows[0].command.args = vec!["x".repeat(MAX_COMMAND_BYTES + 1)];
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn matrix_rejects_malformed_claim_text() {
    let mut matrix = sample_matrix();
    matrix.bounded_claim = "x".repeat(MAX_MATRIX_CLAIM_BYTES + 1);
    assert!(validate_matrix(&matrix).is_err());

    let mut matrix = sample_matrix();
    matrix.non_claims = (0..=MAX_MATRIX_NON_CLAIMS)
        .map(|index| format!("non-claim {index}"))
        .collect();
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn matrix_rejects_invalid_profile_ids() {
    let mut matrix = sample_matrix();
    matrix.profile_id = "bad\nid".to_string();
    assert!(validate_matrix(&matrix).is_err());

    let mut matrix = sample_matrix();
    matrix.profile_id = "x".repeat(MAX_MATRIX_PROFILE_ID_BYTES + 1);
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn matrix_accepts_capability_names_at_the_bound() {
    let capability = "x".repeat(MAX_CAPABILITY_BYTES);
    let mut matrix = sample_matrix();
    matrix.required_worker_capabilities = vec![capability.clone()];
    for row in &mut matrix.rows {
        row.required_capabilities = vec![capability.clone()];
    }
    validate_matrix(&matrix).expect("capability at the byte bound is admitted");
}

#[test]
fn matrix_rejects_malformed_capabilities() {
    let mut matrix = sample_matrix();
    matrix.required_worker_capabilities = vec!["x".repeat(MAX_CAPABILITY_BYTES + 1)];
    assert!(validate_matrix(&matrix).is_err());

    let mut matrix = sample_matrix();
    matrix.rows[0].required_capabilities = vec!["bad\ncap".to_string()];
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn matrix_rejects_empty_or_duplicate_capabilities() {
    let mut matrix = sample_matrix();
    matrix.required_worker_capabilities.push("kvm".to_string());
    assert!(validate_matrix(&matrix).is_err());

    let mut matrix = sample_matrix();
    matrix.rows[0].required_capabilities = vec![String::new()];
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn matrix_rejects_artifact_byte_bounds_above_hash_cap() {
    let mut matrix = sample_matrix();
    matrix.rows[0].limits.max_artifact_bytes = MAX_MATRIX_ARTIFACT_BYTES + 1;
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn matrix_rejects_retained_policy_above_artifact_bound() {
    let mut matrix = sample_matrix();
    matrix.rows[0].retained_artifacts = (0..=ROW_ARTIFACT_LIMIT)
        .map(|index| format!("log{index}.txt"))
        .collect();
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn matrix_rejects_duplicate_retained_artifact_paths() {
    let mut matrix = sample_matrix();
    matrix.rows[0].retained_artifacts = vec!["stdout.log".to_string(), "stdout.log".to_string()];
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn matrix_accepts_safe_nested_retained_artifact_paths() {
    let mut matrix = sample_matrix();
    matrix.rows[0].retained_artifacts = vec!["logs/stdout.log".to_string()];
    validate_matrix(&matrix).expect("safe nested artifact path is admitted");
}

#[test]
fn matrix_accepts_retained_artifact_path_at_the_bound() {
    let mut matrix = sample_matrix();
    matrix.rows[0].retained_artifacts = vec!["x".repeat(MAX_RETAINED_ARTIFACT_PATH_BYTES)];
    validate_matrix(&matrix).expect("retained artifact path at the byte bound is admitted");
}

#[test]
fn matrix_rejects_unsafe_retained_artifact_paths() {
    for path in [
        "../escape",
        "/absolute",
        "nested/../escape",
        "bad\\path",
        "nested//file",
        "nested/",
    ] {
        let mut matrix = sample_matrix();
        matrix.rows[0].retained_artifacts = vec![path.to_string()];
        assert!(
            validate_matrix(&matrix).is_err(),
            "retained artifact path should be rejected: {path}"
        );
    }

    let mut matrix = sample_matrix();
    matrix.rows[0].retained_artifacts = vec!["x".repeat(MAX_RETAINED_ARTIFACT_PATH_BYTES + 1)];
    assert!(validate_matrix(&matrix).is_err());
}

#[test]
fn missing_stale_and_dirty_evidence_fail_closed() {
    let matrix = sample_matrix();
    let mut missing = sample_receipt(&matrix);
    missing.rows.pop();
    missing.terminal_class = ReleaseClass::Blocked;
    assert_blocked(&missing, Blocker::MissingRow, FINISHED_UNIX_SECONDS);

    let mut stale = sample_receipt(&matrix);
    stale.terminal_class = ReleaseClass::Blocked;
    assert_blocked(
        &stale,
        Blocker::StaleReceipt,
        FINISHED_UNIX_SECONDS + RECEIPT_AGE_SECONDS + 1,
    );

    let mut dirty = sample_receipt(&matrix);
    dirty.source.dirty = true;
    dirty.terminal_class = ReleaseClass::Blocked;
    assert_blocked(&dirty, Blocker::DirtySource, FINISHED_UNIX_SECONDS);
}

#[test]
fn skipped_unsupported_and_timed_out_rows_fail_closed() {
    let matrix = sample_matrix();
    for (status, blocker) in [
        (RowStatus::Skipped, Blocker::RowNotPassed),
        (RowStatus::Unsupported, Blocker::RowNotPassed),
        (RowStatus::TimedOut, Blocker::RowNotPassed),
    ] {
        let mut receipt = sample_receipt(&matrix);
        receipt.rows[0].status = status;
        receipt.terminal_class = ReleaseClass::Blocked;
        assert_blocked(&receipt, blocker, FINISHED_UNIX_SECONDS);
    }
}

#[test]
fn tampered_artifact_and_command_identities_fail_closed() {
    let matrix = sample_matrix();
    let mut receipt = sample_receipt(&matrix);
    receipt.rows[0].artifacts[0].bytes = 1;
    receipt.terminal_class = ReleaseClass::Blocked;
    assert_blocked(
        &receipt,
        Blocker::ArtifactSetMismatch,
        FINISHED_UNIX_SECONDS,
    );

    let mut receipt = sample_receipt(&matrix);
    receipt.rows[0].command_identity = EMPTY_BLAKE3.to_string();
    receipt.terminal_class = ReleaseClass::Blocked;
    assert_blocked(
        &receipt,
        Blocker::CommandIdentityMismatch,
        FINISHED_UNIX_SECONDS,
    );
}

#[test]
fn overclaim_and_wrong_terminal_class_fail_closed() {
    let matrix = sample_matrix();
    let mut receipt = sample_receipt(&matrix);
    receipt.bounded_claim = "This proves universal determinism.".to_string();
    receipt.terminal_class = ReleaseClass::Blocked;
    assert_blocked(&receipt, Blocker::Overclaim, FINISHED_UNIX_SECONDS);

    let mut receipt = sample_receipt(&matrix);
    receipt.terminal_class = ReleaseClass::Blocked;
    assert_blocked(
        &receipt,
        Blocker::TerminalClassMismatch,
        FINISHED_UNIX_SECONDS,
    );
}

const LEGACY_DIAGNOSTIC_RECEIPT_ARTIFACT: &str = "fresh/attempts/attempt-01/frontier-policy.json";

// r[verify chaoscontrol.campaign_policy.rollback.valid]
#[test]
fn matrix_rows_cannot_select_the_legacy_diagnostic_policy() {
    for args in [
        vec!["run", "--frontier-policy", "legacy-diagnostic"],
        vec!["run", "--frontier-policy=legacy-diagnostic"],
    ] {
        let mut matrix = sample_matrix();
        matrix.rows[0].command.args = args.into_iter().map(str::to_string).collect();
        let issues = validate_matrix(&matrix).expect_err("diagnostic row refused");
        assert!(
            issues
                .iter()
                .any(|issue| issue
                    .starts_with(crate::frontier_policy_gate::LEGACY_DIAGNOSTIC_REFUSAL)),
            "{issues:?}"
        );
    }
    let mut matrix = sample_matrix();
    matrix.rows[0].command.args = vec!["--frontier-policy".to_string(), "campaign".to_string()];
    validate_matrix(&matrix).expect("Campaign rows are admitted");
}

// r[verify chaoscontrol.campaign_policy.rollback.valid]
#[test]
fn legacy_diagnostic_row_output_blocks_release() {
    let matrix = sample_matrix();
    let mut receipt = sample_receipt(&matrix);
    let row = receipt.rows.last_mut().expect("fresh workload row");
    row.artifacts.push(ArtifactIdentity {
        path: LEGACY_DIAGNOSTIC_RECEIPT_ARTIFACT.to_string(),
        bytes: 0,
        blake3: EMPTY_BLAKE3.to_string(),
    });
    row.artifacts.sort();
    row.artifact_set_identity = artifact_set_identity(&row.artifacts);
    receipt.terminal_class = ReleaseClass::Blocked;
    assert_blocked(
        &receipt,
        Blocker::LegacyDiagnosticOutput,
        FINISHED_UNIX_SECONDS,
    );

    let mut receipt = sample_receipt(&matrix);
    receipt.rows[0].executed_argv.extend([
        "--frontier-policy".to_string(),
        "legacy-diagnostic".to_string(),
    ]);
    receipt.terminal_class = ReleaseClass::Blocked;
    assert_blocked(
        &receipt,
        Blocker::LegacyDiagnosticOutput,
        FINISHED_UNIX_SECONDS,
    );
}
