//! Machine-readable replay verdict artifacts.
//!
//! These records are Rust-owned runtime evidence. Nickel/contracts validate the
//! public JSON shape, but replay classification remains here with the executor
//! that observed snapshot validation and assertion outcomes.
//!
//! The DTO definitions, replay classes, snapshot statuses, and validation
//! decisions are owned by `chaoscontrol-replay-evidence-core` and re-exported
//! here for compatibility. This module keeps only shell concerns: binding
//! verdicts to observed bug/oracle state, hashing artifact bytes, generating
//! run IDs from the clock, and writing verdict files.

use sha2::Digest;

use std::io::Write;

pub use chaoscontrol_replay_evidence_core::dto::{
    ArtifactHash, ReplayClass, ReplayCommandContext, ReplayScheduleVariant,
    ReplaySnapshotValidation, ReplayVerdict, SnapshotValidationStatus, NOT_REPRODUCED_EXIT_STATUS,
    REPLAY_VERDICT_SCHEMA_VERSION, REPRODUCED_EXIT_STATUS,
};

pub struct ReproduceVerdictInput<'a> {
    pub run_id: String,
    pub command: String,
    pub exit_status: i32,
    pub bug_path: String,
    pub bug_artifact_hash: ArtifactHash,
    pub bug: &'a crate::checkpoint::SerializableBug,
    pub snapshot: ReplaySnapshotValidation,
    pub admitted_report: Option<&'a chaoscontrol_fault::oracle::OracleReport>,
    pub target_failed: bool,
    pub diagnostic: String,
}

fn replay_schedule_variant(
    variant: &chaoscontrol_vmm::scheduler::ScheduleVariant,
) -> Result<ReplayScheduleVariant, crate::bug::identity::BugIdentityError> {
    let bytes = serde_json::to_vec(variant)
        .map_err(|_error| crate::bug::identity::BugIdentityError::MalformedCarrier)?;
    let strategy = serde_json::to_string(&variant.strategy_override)
        .map_err(|_error| crate::bug::identity::BugIdentityError::MalformedCarrier)?;
    Ok(ReplayScheduleVariant {
        scheduler_seed: variant.scheduler_seed,
        strategy,
        quantum_override: variant.quantum_override,
        policy_blake3: blake3::hash(&bytes).to_hex().to_string(),
    })
}

#[cfg(test)]
fn replay_schedule_variant_matches(
    variant: &chaoscontrol_vmm::scheduler::ScheduleVariant,
    evidence: &ReplayScheduleVariant,
) -> bool {
    replay_schedule_variant(variant).is_ok_and(|expected| expected == *evidence)
}

/// Bind a replay verdict to the observed bug, oracle report, and command
/// outcome. Fails closed when the claimed outcome conflicts with the admitted
/// evidence carriers.
pub fn verdict_from_reproduce(
    input: ReproduceVerdictInput<'_>,
) -> Result<ReplayVerdict, crate::bug::identity::BugIdentityError> {
    let ReproduceVerdictInput {
        run_id,
        command,
        exit_status,
        bug_path,
        bug_artifact_hash,
        bug,
        snapshot,
        admitted_report,
        target_failed,
        diagnostic,
    } = input;
    let assertion_identity = bug.require_replay_identity()?.clone();
    let replay_class = classify_reproduce(bug, &snapshot, target_failed);
    let reproduced = replay_class == ReplayClass::SnapshotBackedReproduced;
    let expected_exit_status = if reproduced {
        REPRODUCED_EXIT_STATUS
    } else {
        NOT_REPRODUCED_EXIT_STATUS
    };
    if exit_status != expected_exit_status {
        return Err(crate::bug::identity::BugIdentityError::MalformedCarrier);
    }
    if matches!(
        replay_class,
        ReplayClass::SnapshotBackedReproduced | ReplayClass::SnapshotBackedNotReproduced
    ) {
        let report =
            admitted_report.ok_or(crate::bug::identity::BugIdentityError::ReportMismatch)?;
        let record = crate::bug::identity::resolve_restored_report(
            bug.assertion_id,
            Some(&assertion_identity),
            report,
        )?;
        let report_failed = record.verdict() == chaoscontrol_fault::oracle::Verdict::Failed;
        if report_failed != target_failed {
            return Err(crate::bug::identity::BugIdentityError::ReportMismatch);
        }
    }
    if bug_artifact_hash.path != bug_path {
        return Err(crate::bug::identity::BugIdentityError::ArtifactMismatch);
    }
    let mut artifact_hashes = vec![bug_artifact_hash];
    if snapshot.digest_verified {
        if let Some(reference) = snapshot.reference.as_ref() {
            let bug_parent = ::std::path::Path::new(&bug_path)
                .parent()
                .unwrap_or_else(|| ::std::path::Path::new("."));
            artifact_hashes.push(ArtifactHash {
                path: bug_parent
                    .join(&reference.path)
                    .to_string_lossy()
                    .into_owned(),
                sha256: reference.digest.clone(),
            });
        }
    }

    let schedule_variant = bug
        .schedule_variant
        .as_ref()
        .map(replay_schedule_variant)
        .transpose()?;

    Ok(ReplayVerdict {
        schema_version: REPLAY_VERDICT_SCHEMA_VERSION,
        run_id,
        replay_class,
        reproduced,
        command: ReplayCommandContext {
            command,
            exit_status,
        },
        diagnostic,
        bug_path: Some(bug_path),
        bug_id: Some(bug.bug_id),
        assertion_id: Some(bug.assertion_id),
        assertion_identity: Some(assertion_identity),
        fallback_scope: bug.fallback_scope.clone(),
        replay_parent_depth: Some(bug.replay_parent_depth),
        schedule_variant,
        snapshot,
        artifact_hashes,
    })
}

/// Map a snapshot store failure onto the public snapshot validation status.
pub fn snapshot_validation_from_error(
    reference: crate::snapshot_store::ReplayParentSnapshotRef,
    error: &crate::snapshot_store::SnapshotStoreError,
) -> ReplaySnapshotValidation {
    let status = match error {
        crate::snapshot_store::SnapshotStoreError::Missing { .. } => {
            SnapshotValidationStatus::MissingArtifact
        }
        crate::snapshot_store::SnapshotStoreError::DigestMismatch { .. } => {
            SnapshotValidationStatus::InvalidDigest
        }
        crate::snapshot_store::SnapshotStoreError::UnsupportedStore { .. }
        | crate::snapshot_store::SnapshotStoreError::UnsupportedCodec { .. }
        | crate::snapshot_store::SnapshotStoreError::UnsupportedSchema { .. }
        | crate::snapshot_store::SnapshotStoreError::PathEscape { .. }
        | crate::snapshot_store::SnapshotStoreError::NotRegular { .. }
        | crate::snapshot_store::SnapshotStoreError::TooLarge { .. }
        | crate::snapshot_store::SnapshotStoreError::DecompressedTooLarge { .. }
        | crate::snapshot_store::SnapshotStoreError::MetadataMismatch { .. }
        | crate::snapshot_store::SnapshotStoreError::Io { .. }
        | crate::snapshot_store::SnapshotStoreError::Json { .. }
        | crate::snapshot_store::SnapshotStoreError::CborEncode { .. }
        | crate::snapshot_store::SnapshotStoreError::CborDecode { .. } => {
            SnapshotValidationStatus::InvalidRef
        }
    };
    ReplaySnapshotValidation {
        status,
        present: false,
        digest_verified: false,
        reference: Some(reference),
        diagnostic: Some(error.to_string()),
    }
}

pub fn classify_reproduce(
    bug: &crate::checkpoint::SerializableBug,
    snapshot: &ReplaySnapshotValidation,
    target_failed: bool,
) -> ReplayClass {
    ::chaoscontrol_replay_evidence_core::classify::classify_replay(
        bug.require_replay_identity().is_ok(),
        bug.replay_parent_depth,
        snapshot.status,
        target_failed,
    )
}

pub fn write_verdict(
    path: impl AsRef<::std::path::Path>,
    verdict: &ReplayVerdict,
) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    let mut bytes = serde_json::to_vec_pretty(verdict)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    if let Some(parent) = path.parent() {
        ::std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        drop(file);
        if let Err(cleanup_error) = ::std::fs::remove_file(path) {
            return Err(std::io::Error::new(
                error.kind(),
                format!("{error}; failed to remove partial verdict: {cleanup_error}"),
            ));
        }
        return Err(error);
    }
    Ok(())
}

pub fn hash_bytes(path: impl Into<String>, bytes: &[u8]) -> ArtifactHash {
    let mut hasher = ::sha2::Sha256::new();
    hasher.update(bytes);
    ArtifactHash {
        path: path.into(),
        sha256: format!("sha256:{:x}", hasher.finalize()),
    }
}

pub fn new_run_id() -> String {
    let millis = ::std::time::SystemTime::now()
        .duration_since(::std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("replay-{millis}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::SerializableSchedule;
    use chaoscontrol_fault::oracle::PropertyOracle;
    use chaoscontrol_protocol::admission::{BoundAssertionEvent, CatalogBuilder};

    const TEST_RUN_ID: &str = "replay-test";
    const TEST_VARIANT_SEED: u64 = 73;
    const TEST_MINIMUM_QUANTUM: u64 = 2;
    const TEST_MAXIMUM_QUANTUM: u64 = 9;
    const TEST_QUANTUM_OVERRIDE: u64 = 5;

    fn snapshot_ref() -> crate::snapshot_store::ReplayParentSnapshotRef {
        crate::snapshot_store::ReplayParentSnapshotRef {
            store: "file-content-addressed".to_string(),
            digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            codec: "simulation-snapshot-cbor-zstd-v2".to_string(),
            schema_version: 2,
            path: "snapshots/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.snapshot.bin"
                .to_string(),
        }
    }

    fn bug_hash() -> ArtifactHash {
        hash_bytes("bug_2.json", b"bounded bug fixture")
    }

    fn bug(depth: u32, has_ref: bool) -> crate::checkpoint::SerializableBug {
        crate::checkpoint::SerializableBug {
            bug_id: 7,
            assertion_id: 1806003755,
            assertion_identity: Some(crate::test_support::assertion_identity(1806003755)),
            fallback_scope: None,
            assertion_location: "raft probe".to_string(),
            schedule: SerializableSchedule { faults: Vec::new() },
            tick: 123,
            replay_parent_depth: depth,
            replay_parent_snapshot_ref: if has_ref { Some(snapshot_ref()) } else { None },
            dedup_key: Some(99),
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        }
    }

    fn report_for_bug(
        bug: &crate::checkpoint::SerializableBug,
        observation: bool,
    ) -> chaoscontrol_fault::oracle::OracleReport {
        let identity = bug.require_replay_identity().expect("test identity");
        let mut builder = CatalogBuilder::begin(1).expect("catalog begins");
        builder
            .insert(identity.descriptor.clone())
            .expect("descriptor inserts");
        let catalog = builder
            .complete(identity.catalog_token)
            .expect("catalog completes");
        let mut oracle = PropertyOracle::new();
        oracle.activate_catalog(catalog).expect("catalog activates");
        oracle.begin_run();
        oracle
            .record_bound_event(
                &BoundAssertionEvent {
                    catalog_token: identity.catalog_token,
                    fingerprint: identity.fingerprint,
                    kind: identity.descriptor.kind,
                },
                observation,
                None,
            )
            .expect("observation records");
        oracle.end_run();
        oracle.report()
    }

    #[test]
    fn classifies_snapshot_backed_reproduction() {
        let bug = bug(1, true);
        let snapshot = ReplaySnapshotValidation::valid(
            bug.replay_parent_snapshot_ref
                .clone()
                .expect("snapshot ref"),
        );
        assert_eq!(
            classify_reproduce(&bug, &snapshot, true),
            ReplayClass::SnapshotBackedReproduced
        );
        assert_eq!(
            classify_reproduce(&bug, &snapshot, false),
            ReplayClass::SnapshotBackedNotReproduced
        );
    }

    #[test]
    fn classifies_schedule_only_as_gap_even_when_reproduced() {
        let bug = bug(0, false);
        assert_eq!(
            classify_reproduce(&bug, &ReplaySnapshotValidation::not_required(), true),
            ReplayClass::ScheduleOnlyReplayGap
        );
    }

    #[test]
    fn classifies_zero_depth_snapshot_ref_as_schedule_only_gap() {
        let bug = bug(0, true);
        let snapshot = ReplaySnapshotValidation::valid(
            bug.replay_parent_snapshot_ref
                .clone()
                .expect("snapshot ref"),
        );
        assert_eq!(
            classify_reproduce(&bug, &snapshot, false),
            ReplayClass::ScheduleOnlyReplayGap
        );
    }

    #[test]
    fn classifies_snapshot_validation_failures_as_distinct_negative_classes() {
        let bug = bug(2, true);
        let reference = snapshot_ref();

        assert_eq!(
            classify_reproduce(
                &bug,
                &ReplaySnapshotValidation::missing_ref("missing replay_parent_snapshot_ref"),
                true,
            ),
            ReplayClass::MissingSnapshotRef
        );
        assert_eq!(
            classify_reproduce(
                &bug,
                &snapshot_validation_from_error(
                    reference.clone(),
                    &crate::snapshot_store::SnapshotStoreError::Missing {
                        path: reference.path.clone(),
                    },
                ),
                true,
            ),
            ReplayClass::MissingSnapshotArtifact
        );
        assert_eq!(
            classify_reproduce(
                &bug,
                &snapshot_validation_from_error(
                    reference.clone(),
                    &crate::snapshot_store::SnapshotStoreError::DigestMismatch {
                        path: reference.path.clone(),
                        expected: reference.digest.clone(),
                        actual: "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                            .to_string(),
                    },
                ),
                true,
            ),
            ReplayClass::InvalidSnapshotDigest
        );
        assert_eq!(
            classify_reproduce(
                &bug,
                &snapshot_validation_from_error(
                    reference,
                    &crate::snapshot_store::SnapshotStoreError::UnsupportedCodec {
                        codec: "other-codec".to_string(),
                    },
                ),
                true,
            ),
            ReplayClass::InvalidSnapshotDigest
        );
    }

    #[test]
    fn no_bug_found_verdict_is_not_reproduced_and_has_no_bug_context() {
        let verdict = ReplayVerdict::no_bug_found(
            TEST_RUN_ID.to_string(),
            "chaoscontrol-explore reproduce --bug missing.json".to_string(),
            "bug file not found",
        );
        assert_eq!(verdict.replay_class, ReplayClass::NoBugFound);
        assert!(!verdict.reproduced);
        assert_eq!(verdict.command.exit_status, 1);
        assert!(verdict.bug_path.is_none());
        assert_eq!(
            verdict.snapshot.status,
            SnapshotValidationStatus::NotRequired
        );
        assert!(verdict.artifact_hashes.is_empty());
    }

    #[test]
    fn serializes_stable_snake_case_class() {
        let bug = bug(2, true);
        let report = report_for_bug(&bug, false);
        let verdict = verdict_from_reproduce(ReproduceVerdictInput {
            run_id: TEST_RUN_ID.to_string(),
            command: "chaoscontrol-explore reproduce ...".to_string(),
            exit_status: 0,
            bug_path: "bug_2.json".to_string(),
            bug_artifact_hash: bug_hash(),
            bug: &bug,
            snapshot: ReplaySnapshotValidation::valid(
                bug.replay_parent_snapshot_ref
                    .clone()
                    .expect("snapshot ref"),
            ),
            admitted_report: Some(&report),
            target_failed: true,
            diagnostic: "BUG REPRODUCED — assertion 1806003755 failed".to_string(),
        })
        .expect("strict identity produces verdict");
        let json = serde_json::to_string(&verdict).unwrap();
        assert!(json.contains("snapshot_backed_reproduced"));
        assert!(json.contains("digest_verified"));
        assert!(json.contains("assertion_identity"));
    }

    #[test]
    fn replay_verdict_binds_variant_and_rejects_identity_drift() {
        let mut bug = bug(2, true);
        bug.schedule_variant = Some(chaoscontrol_vmm::scheduler::ScheduleVariant {
            scheduler_seed: TEST_VARIANT_SEED,
            strategy_override: Some(
                chaoscontrol_vmm::scheduler::SchedulingStrategy::Randomized {
                    min_quantum: TEST_MINIMUM_QUANTUM,
                    max_quantum: TEST_MAXIMUM_QUANTUM,
                },
            ),
            quantum_override: Some(TEST_QUANTUM_OVERRIDE),
        });
        let report = report_for_bug(&bug, false);
        let verdict = verdict_from_reproduce(ReproduceVerdictInput {
            run_id: TEST_RUN_ID.to_string(),
            command: "chaoscontrol-explore reproduce ...".to_string(),
            exit_status: REPRODUCED_EXIT_STATUS,
            bug_path: "bug_2.json".to_string(),
            bug_artifact_hash: bug_hash(),
            bug: &bug,
            snapshot: ReplaySnapshotValidation::valid(
                bug.replay_parent_snapshot_ref
                    .clone()
                    .expect("snapshot ref"),
            ),
            admitted_report: Some(&report),
            target_failed: true,
            diagnostic: "variant-bound replay".to_string(),
        })
        .expect("variant-bound verdict");
        let evidence = verdict
            .schedule_variant
            .as_ref()
            .expect("schedule variant evidence");
        assert!(replay_schedule_variant_matches(
            bug.schedule_variant.as_ref().expect("bug variant"),
            evidence,
        ));
        assert!(!evidence.policy_blake3.is_empty());

        let mut drifted = evidence.clone();
        drifted.scheduler_seed = drifted.scheduler_seed.wrapping_add(1);
        assert!(!replay_schedule_variant_matches(
            bug.schedule_variant.as_ref().expect("bug variant"),
            &drifted,
        ));
    }

    #[test]
    fn rejects_legacy_bug_before_verdict_generation() {
        let mut legacy = bug(2, true);
        legacy.assertion_identity = None;
        let result = verdict_from_reproduce(ReproduceVerdictInput {
            run_id: TEST_RUN_ID.to_string(),
            command: "chaoscontrol-explore reproduce ...".to_string(),
            exit_status: 1,
            bug_path: "bug_2.json".to_string(),
            bug_artifact_hash: bug_hash(),
            bug: &legacy,
            snapshot: ReplaySnapshotValidation::missing_ref("missing ref"),
            admitted_report: None,
            target_failed: false,
            diagnostic: "legacy bug".to_string(),
        });

        assert!(result.is_err());
    }

    #[test]
    fn rejects_exit_status_that_conflicts_with_the_replay_class() {
        let bug = bug(2, true);
        let report = report_for_bug(&bug, false);
        let result = verdict_from_reproduce(ReproduceVerdictInput {
            run_id: TEST_RUN_ID.to_string(),
            command: "chaoscontrol-explore reproduce ...".to_string(),
            exit_status: NOT_REPRODUCED_EXIT_STATUS,
            bug_path: "bug_2.json".to_string(),
            bug_artifact_hash: bug_hash(),
            bug: &bug,
            snapshot: ReplaySnapshotValidation::valid(
                bug.replay_parent_snapshot_ref
                    .clone()
                    .expect("snapshot ref"),
            ),
            admitted_report: Some(&report),
            target_failed: true,
            diagnostic: "forged command status".to_string(),
        });

        assert_eq!(
            result,
            Err(crate::bug::identity::BugIdentityError::MalformedCarrier)
        );
    }

    #[test]
    fn rejects_a_reproduced_claim_that_conflicts_with_the_admitted_report() {
        let bug = bug(2, true);
        let report = report_for_bug(&bug, true);
        let result = verdict_from_reproduce(ReproduceVerdictInput {
            run_id: TEST_RUN_ID.to_string(),
            command: "chaoscontrol-explore reproduce ...".to_string(),
            exit_status: 0,
            bug_path: "bug_2.json".to_string(),
            bug_artifact_hash: bug_hash(),
            bug: &bug,
            snapshot: ReplaySnapshotValidation::valid(
                bug.replay_parent_snapshot_ref
                    .clone()
                    .expect("snapshot ref"),
            ),
            admitted_report: Some(&report),
            target_failed: true,
            diagnostic: "forged failure".to_string(),
        });

        assert_eq!(
            result,
            Err(crate::bug::identity::BugIdentityError::ReportMismatch)
        );
    }
}
