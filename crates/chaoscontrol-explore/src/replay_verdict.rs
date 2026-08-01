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
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const REPLAY_VERDICT_SCHEMA_VERSION: u32 = 2;

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

impl ReplayVerdict {
    pub fn from_reproduce(
        command: String,
        exit_status: i32,
        bug_path: impl Into<String>,
        bug: &SerializableBug,
        snapshot: ReplaySnapshotValidation,
        admitted_report: Option<&chaoscontrol_fault::oracle::OracleReport>,
        target_failed: bool,
        diagnostic: impl Into<String>,
    ) -> Result<Self, crate::bug::identity::BugIdentityError> {
        let assertion_identity = bug.require_replay_identity()?.clone();
        let replay_class = classify_reproduce(bug, &snapshot, target_failed);
        if matches!(
            replay_class,
            ReplayClass::SnapshotBackedReproduced | ReplayClass::SnapshotBackedNotReproduced
        ) {
            let report = admitted_report.ok_or(crate::bug::identity::BugIdentityError::ReportMismatch)?;
            let record = crate::bug::identity::resolve_restored_report(
                bug.assertion_id,
                Some(&assertion_identity),
                report,
            )?;
            let report_failed =
                record.verdict() == chaoscontrol_fault::oracle::Verdict::Failed;
            if report_failed != target_failed {
                return Err(crate::bug::identity::BugIdentityError::ReportMismatch);
            }
        }
        let bug_path = bug_path.into();
        let mut artifact_hashes = Vec::new();
        if let Ok(hash) = hash_file(&bug_path) {
            artifact_hashes.push(hash);
        }
        if snapshot.digest_verified {
            if let Some(reference) = snapshot.reference.as_ref() {
                let root = Path::new(&bug_path)
                    .parent()
                    .unwrap_or_else(|| Path::new("."));
                let snapshot_path = root.join(&reference.path);
                if let Ok(hash) = hash_file(snapshot_path.to_string_lossy().as_ref()) {
                    artifact_hashes.push(hash);
                }
            }
        }

        Ok(Self {
            schema_version: REPLAY_VERDICT_SCHEMA_VERSION,
            run_id: default_run_id(),
            replay_class,
            reproduced: replay_class == ReplayClass::SnapshotBackedReproduced,
            command: ReplayCommandContext {
                command,
                exit_status,
            },
            diagnostic: diagnostic.into(),
            bug_path: Some(bug_path),
            bug_id: Some(bug.bug_id),
            assertion_id: Some(bug.assertion_id),
            assertion_identity: Some(assertion_identity),
            replay_parent_depth: Some(bug.replay_parent_depth),
            snapshot,
            artifact_hashes,
        })
    }

    pub fn no_bug_found(command: String, diagnostic: impl Into<String>) -> Self {
        Self {
            schema_version: REPLAY_VERDICT_SCHEMA_VERSION,
            run_id: default_run_id(),
            replay_class: ReplayClass::NoBugFound,
            reproduced: false,
            command: ReplayCommandContext {
                command,
                exit_status: 1,
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
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(verdict).expect("replay verdict serializes");
    fs::write(path, format!("{json}\n"))
}

pub fn hash_file(path: &str) -> Result<ArtifactHash, std::io::Error> {
    let bytes = fs::read(path)?;
    let mut h = Sha256::new();
    h.update(bytes);
    Ok(ArtifactHash {
        path: path.to_string(),
        sha256: format!("sha256:{:x}", h.finalize()),
    })
}

fn default_run_id() -> String {
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

    fn snapshot_ref() -> ReplayParentSnapshotRef {
        ReplayParentSnapshotRef {
            store: "file-content-addressed".to_string(),
            digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            codec: "simulation-snapshot-bincode-zstd-v1".to_string(),
            schema_version: 1,
            path: "snapshots/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.snapshot.bin"
                .to_string(),
        }
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

    fn report_for_bug(bug: &SerializableBug, observation: bool) -> chaoscontrol_fault::oracle::OracleReport {
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
        let verdict = ReplayVerdict::from_reproduce(
            "chaoscontrol-explore reproduce ...".to_string(),
            0,
            "bug_2.json",
            &bug,
            ReplaySnapshotValidation::valid(
                bug.replay_parent_snapshot_ref
                    .clone()
                    .expect("snapshot ref"),
            ),
            Some(&report),
            true,
            "BUG REPRODUCED — assertion 1806003755 failed",
        )
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
        let result = ReplayVerdict::from_reproduce(
            "chaoscontrol-explore reproduce ...".to_string(),
            1,
            "bug_2.json",
            &legacy,
            ReplaySnapshotValidation::missing_ref("missing ref"),
            None,
            false,
            "legacy bug",
        );

        assert!(result.is_err());
    }

    #[test]
    fn rejects_a_reproduced_claim_that_conflicts_with_the_admitted_report() {
        let bug = bug(2, true);
        let report = report_for_bug(&bug, true);
        let result = ReplayVerdict::from_reproduce(
            "chaoscontrol-explore reproduce ...".to_string(),
            0,
            "bug_2.json",
            &bug,
            ReplaySnapshotValidation::valid(
                bug.replay_parent_snapshot_ref
                    .clone()
                    .expect("snapshot ref"),
            ),
            Some(&report),
            true,
            "forged failure",
        );

        assert_eq!(result, Err(crate::bug::identity::BugIdentityError::ReportMismatch));
    }
}
