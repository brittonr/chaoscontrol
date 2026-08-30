//! Shared replay/evidence DTO definitions.
//!
//! These records are Rust-owned runtime evidence. Nickel contracts validate
//! the public JSON shape at review boundaries, but the definitions here are
//! the single authority for both the explorer emitter and the evidence gates.

use chaoscontrol_protocol::admission::AssertionEvidenceIdentity;
use serde::{Deserialize, Serialize};

/// Current schema version for emitted replay verdict artifacts.
pub const REPLAY_VERDICT_SCHEMA_VERSION: u32 = 2;
/// Oldest schema version still admitted by compatibility readers.
pub const LEGACY_REPLAY_VERDICT_SCHEMA_VERSION: u32 = 1;
/// Exit status recorded for a verdict whose replay reproduced the bug.
pub const REPRODUCED_EXIT_STATUS: i32 = 0;
/// Exit status recorded for a verdict whose replay did not reproduce the bug.
pub const NOT_REPRODUCED_EXIT_STATUS: i32 = 1;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct ReplayParentSnapshotRef {
    pub store: String,
    pub digest: String,
    pub codec: String,
    pub schema_version: u32,
    pub path: String,
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
pub struct ReplayScheduleVariant {
    pub scheduler_seed: u64,
    pub strategy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantum_override: Option<u64>,
    pub policy_blake3: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_scope: Option<chaoscontrol_protocol::fallback::FallbackAssertionScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_parent_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule_variant: Option<ReplayScheduleVariant>,
    pub snapshot: ReplaySnapshotValidation,
    pub artifact_hashes: Vec<ArtifactHash>,
}

fn no_assertion_identity() -> Option<AssertionEvidenceIdentity> {
    None
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
}

impl ReplayVerdict {
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
            fallback_scope: None,
            replay_parent_depth: None,
            schedule_variant: None,
            snapshot: ReplaySnapshotValidation::not_required(),
            artifact_hashes: Vec::new(),
        }
    }
}
