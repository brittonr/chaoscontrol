//! Machine-readable replay verdict artifacts.
//!
//! These records are Rust-owned runtime evidence. Nickel/contracts validate the
//! public JSON shape, but replay classification remains here with the executor
//! that observed snapshot validation and assertion outcomes.

use crate::checkpoint::SerializableBug;
use crate::snapshot_store::{ReplayParentSnapshotRef, SnapshotStoreError};
use chaoscontrol_protocol::admission::AssertionEvidenceIdentity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const REPLAY_VERDICT_SCHEMA_VERSION: u32 = 2;
const REPRODUCED_EXIT_STATUS: i32 = 0;
const NOT_REPRODUCED_EXIT_STATUS: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayClass {
    SnapshotBackedReproduced,
    SnapshotBackedNotReproduced,
    ScheduleOnlyReplayGap,
    MissingSnapshotRef,
    MissingSnapshotArtifact,
    InvalidSnapshotDigest,
    NoBugFound,
    ReplayError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotValidationStatus {
    NotRequired,
    MissingRef,
    Valid,
    MissingArtifact,
    InvalidDigest,
    InvalidRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactHash {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayCommandContext {
    pub command: String,
    pub exit_status: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaySnapshotValidation {
    pub status: SnapshotValidationStatus,
    pub present: bool,
    pub digest_verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<ReplayParentSnapshotRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayVerdict {
    pub schema_version: u32,
    pub run_id: String,
    pub replay_class: ReplayClass,
    pub reproduced: bool,
    pub command: ReplayCommandContext,
    pub diagnostic: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bug_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bug_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assertion_id: Option<u64>,
    #[serde(
        default = "no_assertion_identity",
        deserialize_with = "crate::non_null_option::deserialize",
        skip_serializing_if = "Option::is_none"
    )]
    pub assertion_identity: Option<AssertionEvidenceIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_parent_depth: Option<u32>,
    pub snapshot: ReplaySnapshotValidation,
    pub artifact_hashes: Vec<ArtifactHash>,
}

fn no_assertion_identity() -> Option<AssertionEvidenceIdentity> {
    None
}

pub struct ReproduceVerdictInput<'a> {
    pub run_id: String,
    pub command: String,
    pub exit_status: i32,
    pub bug_path: String,
    pub bug_artifact_hash: ArtifactHash,
    pub bug: &'a SerializableBug,
    pub snapshot: ReplaySnapshotValidation,
    pub admitted_report: Option<&'a chaoscontrol_fault::oracle::OracleReport>,
    pub target_failed: bool,
    pub diagnostic: String,
}

impl ReplayVerdict {
    pub fn from_reproduce(
        input: ReproduceVerdictInput<'_>,
    ) -> Result<Self, crate::bug::identity::BugIdentityError> {
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
                let bug_parent = Path::new(&bug_path)
                    .parent()
                    .unwrap_or_else(|| Path::new("."));
                artifact_hashes.push(ArtifactHash {
                    path: bug_parent
                        .join(&reference.path)
                        .to_string_lossy()
                        .into_owned(),
                    sha256: reference.digest.clone(),
                });
            }
        }

        Ok(Self {
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
            replay_parent_depth: Some(bug.replay_parent_depth),
            snapshot,
            artifact_hashes,
        })
    }

    pub fn no_bug_found(run_id: String, command: String, diagnostic: impl Into<String>) -> Self {
        Self {
            schema_version: REPLAY_VERDICT_SCHEMA_VERSION,
            run_id,
            replay_class: ReplayClass::NoBugFound,
            reproduced: false,
            command: ReplayCommandContext {
                command,
                exit_status: NOT_REPRODUCED_EXIT_STATUS,
            },
            diagnostic: diagnostic.into(),
            bug_path: None,
            bug_id: None,
            assertion_id: None,
            assertion_identity: None,
            replay_parent_depth: None,
            snapshot: ReplaySnapshotValidation::not_required(),
            artifact_hashes: Vec::new(),
        }
    }
}

impl ReplaySnapshotValidation {
    pub fn not_required() -> Self {
        Self {
            status: SnapshotValidationStatus::NotRequired,
            present: false,
            digest_verified: false,
            reference: None,
            diagnostic: None,
        }
    }

    pub fn missing_ref(diagnostic: impl Into<String>) -> Self {
        Self {
            status: SnapshotValidationStatus::MissingRef,
            present: false,
            digest_verified: false,
            reference: None,
            diagnostic: Some(diagnostic.into()),
        }
    }

    pub fn valid(reference: ReplayParentSnapshotRef) -> Self {
        Self {
            status: SnapshotValidationStatus::Valid,
            present: true,
            digest_verified: true,
            reference: Some(reference),
            diagnostic: None,
        }
    }

    pub fn from_error(reference: ReplayParentSnapshotRef, error: &SnapshotStoreError) -> Self {
        let status = match error {
            SnapshotStoreError::Missing { .. } => SnapshotValidationStatus::MissingArtifact,
            SnapshotStoreError::DigestMismatch { .. } => SnapshotValidationStatus::InvalidDigest,
            SnapshotStoreError::UnsupportedStore { .. }
            | SnapshotStoreError::UnsupportedCodec { .. }
            | SnapshotStoreError::UnsupportedSchema { .. }
            | SnapshotStoreError::PathEscape { .. }
            | SnapshotStoreError::NotRegular { .. }
            | SnapshotStoreError::TooLarge { .. }
            | SnapshotStoreError::DecompressedTooLarge { .. }
            | SnapshotStoreError::MetadataMismatch { .. }
            | SnapshotStoreError::Io { .. }
            | SnapshotStoreError::Json { .. }
            | SnapshotStoreError::CborEncode { .. }
            | SnapshotStoreError::CborDecode { .. } => SnapshotValidationStatus::InvalidRef,
        };
        Self {
            status,
            present: false,
            digest_verified: false,
            reference: Some(reference),
            diagnostic: Some(error.to_string()),
        }
    }
}

pub fn classify_reproduce(
    bug: &SerializableBug,
    snapshot: &ReplaySnapshotValidation,
    target_failed: bool,
) -> ReplayClass {
    if bug.require_replay_identity().is_err() {
        return ReplayClass::ReplayError;
    }
    match snapshot.status {
        SnapshotValidationStatus::MissingRef => ReplayClass::MissingSnapshotRef,
        SnapshotValidationStatus::MissingArtifact => ReplayClass::MissingSnapshotArtifact,
        SnapshotValidationStatus::InvalidDigest | SnapshotValidationStatus::InvalidRef => {
            ReplayClass::InvalidSnapshotDigest
        }
        SnapshotValidationStatus::Valid if bug.replay_parent_depth == 0 => {
            ReplayClass::ScheduleOnlyReplayGap
        }
        SnapshotValidationStatus::Valid if target_failed => ReplayClass::SnapshotBackedReproduced,
        SnapshotValidationStatus::Valid => ReplayClass::SnapshotBackedNotReproduced,
        SnapshotValidationStatus::NotRequired => ReplayClass::ScheduleOnlyReplayGap,
    }
}

pub fn write_verdict(
    path: impl AsRef<Path>,
    verdict: &ReplayVerdict,
) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    let mut bytes = serde_json::to_vec_pretty(verdict)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        drop(file);
        if let Err(cleanup_error) = fs::remove_file(path) {
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
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    ArtifactHash {
        path: path.into(),
        sha256: format!("sha256:{:x}", hasher.finalize()),
    }
}

pub fn new_run_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
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

    fn snapshot_ref() -> ReplayParentSnapshotRef {
        ReplayParentSnapshotRef {
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

    fn bug(depth: u32, has_ref: bool) -> SerializableBug {
        SerializableBug {
            bug_id: 7,
            assertion_id: 1806003755,
            assertion_identity: Some(crate::test_support::assertion_identity(1806003755)),
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
        bug: &SerializableBug,
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
                &ReplaySnapshotValidation::from_error(
                    reference.clone(),
                    &SnapshotStoreError::Missing {
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
                &ReplaySnapshotValidation::from_error(
                    reference.clone(),
                    &SnapshotStoreError::DigestMismatch {
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
                &ReplaySnapshotValidation::from_error(
                    reference,
                    &SnapshotStoreError::UnsupportedCodec {
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
        let verdict = ReplayVerdict::from_reproduce(ReproduceVerdictInput {
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
    fn rejects_legacy_bug_before_verdict_generation() {
        let mut legacy = bug(2, true);
        legacy.assertion_identity = None;
        let result = ReplayVerdict::from_reproduce(ReproduceVerdictInput {
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
        let result = ReplayVerdict::from_reproduce(ReproduceVerdictInput {
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
        let result = ReplayVerdict::from_reproduce(ReproduceVerdictInput {
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
