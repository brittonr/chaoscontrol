//! Fail-closed validation decisions for replay/evidence DTOs.
//!
//! Every function here is pure: it inspects in-memory values and returns a
//! deterministic diagnostic naming the invalid field. Shells own all file,
//! VM, clock, and process effects.

use std::fmt;
use std::path::{Component, Path};

use crate::dto::{
    ArtifactHash, ReplayClass, ReplayParentSnapshotRef, ReplayScheduleVariant, ReplayVerdict,
    SnapshotValidationStatus, LEGACY_REPLAY_VERDICT_SCHEMA_VERSION, REPLAY_VERDICT_SCHEMA_VERSION,
    REPRODUCED_EXIT_STATUS,
};

/// Replay class accepted as replay proof by evidence gates.
pub const REQUIRED_REPLAY_CLASS: &str = "snapshot_backed_reproduced";
/// All replay classes admitted on the public verdict boundary.
pub const REPLAY_CLASSES: [&str; 8] = [
    "snapshot_backed_reproduced",
    "snapshot_backed_not_reproduced",
    "schedule_only_replay_gap",
    "missing_snapshot_ref",
    "missing_snapshot_artifact",
    "invalid_snapshot_digest",
    "no_bug_found",
    "replay_error",
];
/// All snapshot validation statuses admitted on the public verdict boundary.
pub const SNAPSHOT_STATUSES: [&str; 6] = [
    "not_required",
    "missing_ref",
    "valid",
    "missing_artifact",
    "invalid_digest",
    "invalid_ref",
];
/// Only snapshot store kind admitted on the public evidence boundary.
pub const FILE_STORE_KIND: &str = "file-content-addressed";
/// Snapshot codecs admitted for reading historical evidence.
pub const SUPPORTED_SNAPSHOT_CODECS: [&str; 2] = [
    "simulation-snapshot-cbor-zstd-v2",
    "simulation-snapshot-bincode-zstd-v1",
];
/// Snapshot schema versions admitted for reading historical evidence.
pub const SUPPORTED_SNAPSHOT_SCHEMA_VERSIONS: [u32; 2] = [1, 2];
/// Codec required for newly accepted replay proof.
pub const CURRENT_SNAPSHOT_CODEC: &str = "simulation-snapshot-cbor-zstd-v2";
/// Schema version required for newly accepted replay proof.
pub const CURRENT_SNAPSHOT_SCHEMA_VERSION: u32 = 2;

const SHA256_PREFIX: &str = "sha256:";
const SHA256_HEX_LENGTH: usize = 64;
const BLAKE3_HEX_LENGTH: usize = 64;
const MAXIMUM_STRATEGY_BYTES: usize = 128;

/// Deterministic validation failure naming the invalid field or claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    message: String,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

/// True when the digest string has the exact `sha256:<64 lowercase hex>` shape.
pub fn is_prefixed_sha256(value: &str) -> bool {
    let Some(hex) = value.strip_prefix(SHA256_PREFIX) else {
        return false;
    };
    hex.len() == SHA256_HEX_LENGTH
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

/// Reject empty, absolute, or parent-escaping public artifact paths.
pub fn validate_public_artifact_path(field: &str, path: &str) -> Result<(), ValidationError> {
    if path.is_empty() {
        return Err(ValidationError::new(format!(
            "{field}: path must be non-empty"
        )));
    }
    let parsed = Path::new(path);
    for component in parsed.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return Err(ValidationError::new(format!(
                    "{field}: path escapes the evidence root: {path}"
                )));
            }
            Component::ParentDir => {
                return Err(ValidationError::new(format!(
                    "{field}: path escapes the evidence root: {path}"
                )));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(())
}

/// Validate one artifact hash entry: non-empty path plus digest shape.
///
/// Path confinement is a separate public-evidence gate
/// ([`validate_public_verdict_paths`]); local replay tooling may legitimately
/// record absolute artifact paths.
pub fn validate_artifact_hash(hash: &ArtifactHash) -> Result<(), ValidationError> {
    if hash.path.is_empty() {
        return Err(ValidationError::new(
            "artifact-hash.path: path must be non-empty",
        ));
    }
    if !is_prefixed_sha256(&hash.sha256) {
        return Err(ValidationError::new(
            "artifact-hash.sha256: expected sha256:<64 hex>",
        ));
    }
    Ok(())
}

/// Public evidence boundary: every path a verdict publishes must be a
/// confined relative path. Committed receipts, contract fixtures, and
/// accepted workload proofs cross this gate; local replay output may not.
pub fn validate_public_verdict_paths(verdict: &ReplayVerdict) -> Result<(), ValidationError> {
    if let Some(bug_path) = verdict.bug_path.as_deref() {
        validate_public_artifact_path("replay-verdict.bug_path", bug_path)?;
    }
    for hash in &verdict.artifact_hashes {
        validate_public_artifact_path("replay-verdict.artifact_hashes.path", &hash.path)?;
    }
    if let Some(reference) = verdict.snapshot.reference.as_ref() {
        validate_public_artifact_path("replay-verdict.snapshot.reference.path", &reference.path)?;
    }
    Ok(())
}

/// Detect a stale artifact hash: the digest recorded in evidence no longer
/// matches the digest the shell recomputed over the artifact bytes.
pub fn verify_artifact_digest(
    recorded: &ArtifactHash,
    recomputed_digest: &str,
) -> Result<(), ValidationError> {
    validate_artifact_hash(recorded)?;
    if recorded.sha256 != recomputed_digest {
        return Err(ValidationError::new(format!(
            "artifact-hash.sha256: stale artifact hash for {}: recorded {}, recomputed {recomputed_digest}",
            recorded.path, recorded.sha256
        )));
    }
    Ok(())
}

/// Validate snapshot reference shape against the supported read set.
pub fn validate_snapshot_ref_shape(
    reference: &ReplayParentSnapshotRef,
) -> Result<(), ValidationError> {
    if reference.store != FILE_STORE_KIND {
        return Err(ValidationError::new(format!(
            "snapshot-ref.store: expected {FILE_STORE_KIND}, got {:?}",
            reference.store
        )));
    }
    if !is_prefixed_sha256(&reference.digest) {
        return Err(ValidationError::new(
            "snapshot-ref.digest: expected sha256:<64 hex>",
        ));
    }
    if !SUPPORTED_SNAPSHOT_CODECS.contains(&reference.codec.as_str()) {
        return Err(ValidationError::new(format!(
            "snapshot-ref.codec: unsupported codec {:?}",
            reference.codec
        )));
    }
    if !SUPPORTED_SNAPSHOT_SCHEMA_VERSIONS.contains(&reference.schema_version) {
        return Err(ValidationError::new(format!(
            "snapshot-ref.schema_version: unsupported version {}",
            reference.schema_version
        )));
    }
    validate_public_artifact_path("snapshot-ref.path", &reference.path)?;
    Ok(())
}

/// Validate that a snapshot reference matches the current codec and schema
/// version required for newly accepted replay proof.
pub fn validate_snapshot_ref_current(
    reference: &ReplayParentSnapshotRef,
) -> Result<(), ValidationError> {
    validate_snapshot_ref_shape(reference)?;
    if reference.codec != CURRENT_SNAPSHOT_CODEC
        || reference.schema_version != CURRENT_SNAPSHOT_SCHEMA_VERSION
    {
        return Err(ValidationError::new(
            "snapshot-ref: accepted snapshot evidence requires the current CBOR v2 codec",
        ));
    }
    Ok(())
}

pub fn replay_class_str(class: ReplayClass) -> &'static str {
    match class {
        ReplayClass::SnapshotBackedReproduced => "snapshot_backed_reproduced",
        ReplayClass::SnapshotBackedNotReproduced => "snapshot_backed_not_reproduced",
        ReplayClass::ScheduleOnlyReplayGap => "schedule_only_replay_gap",
        ReplayClass::MissingSnapshotRef => "missing_snapshot_ref",
        ReplayClass::MissingSnapshotArtifact => "missing_snapshot_artifact",
        ReplayClass::InvalidSnapshotDigest => "invalid_snapshot_digest",
        ReplayClass::NoBugFound => "no_bug_found",
        ReplayClass::ReplayError => "replay_error",
    }
}

/// Parse a public replay class string. Unknown classes fail closed.
pub fn parse_replay_class(value: &str) -> Result<ReplayClass, ValidationError> {
    REPLAY_CLASSES
        .iter()
        .position(|candidate| *candidate == value)
        .map(|index| match index {
            0 => ReplayClass::SnapshotBackedReproduced,
            1 => ReplayClass::SnapshotBackedNotReproduced,
            2 => ReplayClass::ScheduleOnlyReplayGap,
            3 => ReplayClass::MissingSnapshotRef,
            4 => ReplayClass::MissingSnapshotArtifact,
            5 => ReplayClass::InvalidSnapshotDigest,
            6 => ReplayClass::NoBugFound,
            _ => ReplayClass::ReplayError,
        })
        .ok_or_else(|| {
            ValidationError::new(format!(
                "replay-verdict.replay_class: expected one of {REPLAY_CLASSES:?}, got {value:?}"
            ))
        })
}

pub fn snapshot_status_str(status: SnapshotValidationStatus) -> &'static str {
    match status {
        SnapshotValidationStatus::NotRequired => "not_required",
        SnapshotValidationStatus::MissingRef => "missing_ref",
        SnapshotValidationStatus::Valid => "valid",
        SnapshotValidationStatus::MissingArtifact => "missing_artifact",
        SnapshotValidationStatus::InvalidDigest => "invalid_digest",
        SnapshotValidationStatus::InvalidRef => "invalid_ref",
    }
}

/// Parse a public snapshot validation status string. Unknown statuses fail closed.
pub fn parse_snapshot_status(value: &str) -> Result<SnapshotValidationStatus, ValidationError> {
    SNAPSHOT_STATUSES
        .iter()
        .position(|candidate| *candidate == value)
        .map(|index| match index {
            0 => SnapshotValidationStatus::NotRequired,
            1 => SnapshotValidationStatus::MissingRef,
            2 => SnapshotValidationStatus::Valid,
            3 => SnapshotValidationStatus::MissingArtifact,
            4 => SnapshotValidationStatus::InvalidDigest,
            _ => SnapshotValidationStatus::InvalidRef,
        })
        .ok_or_else(|| {
            ValidationError::new(format!(
                "replay-verdict.snapshot.status: expected one of {SNAPSHOT_STATUSES:?}, got {value:?}"
            ))
        })
}

fn validate_verdict_schema(version: u32) -> Result<(), ValidationError> {
    if !matches!(
        version,
        LEGACY_REPLAY_VERDICT_SCHEMA_VERSION | REPLAY_VERDICT_SCHEMA_VERSION
    ) {
        return Err(ValidationError::new(format!(
            "replay-verdict.schema_version: unsupported version {version}"
        )));
    }
    Ok(())
}

fn validate_schedule_variant(variant: &ReplayScheduleVariant) -> Result<(), ValidationError> {
    if variant.strategy.is_empty() || variant.strategy.len() > MAXIMUM_STRATEGY_BYTES {
        return Err(ValidationError::new(
            "replay-verdict.schedule_variant.strategy: invalid length",
        ));
    }
    if variant.quantum_override == Some(0) {
        return Err(ValidationError::new(
            "replay-verdict.schedule_variant.quantum_override: must be positive",
        ));
    }
    if variant.policy_blake3.len() != BLAKE3_HEX_LENGTH
        || !variant
            .policy_blake3
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ValidationError::new(
            "replay-verdict.schedule_variant.policy_blake3: expected 64 lowercase hex characters",
        ));
    }
    Ok(())
}

fn require_bug_context(verdict: &ReplayVerdict) -> Result<(), ValidationError> {
    if verdict.bug_path.is_none() {
        return Err(ValidationError::new(
            "replay-verdict.bug_path: required for bug verdicts",
        ));
    }
    if verdict.bug_id.is_none() {
        return Err(ValidationError::new(
            "replay-verdict.bug_id: required for bug verdicts",
        ));
    }
    if verdict.assertion_id.is_none() {
        return Err(ValidationError::new(
            "replay-verdict.assertion_id: required for bug verdicts",
        ));
    }
    if verdict.replay_parent_depth.is_none() {
        return Err(ValidationError::new(
            "replay-verdict.replay_parent_depth: required for bug verdicts",
        ));
    }
    let bug_path = verdict.bug_path.as_deref().unwrap_or_default();
    match verdict.artifact_hashes.first() {
        Some(first) if first.path == bug_path => Ok(()),
        _ => Err(ValidationError::new(
            "replay-verdict.artifact_hashes: first entry must hash the bug artifact",
        )),
    }
}

fn validate_fallback_scope(verdict: &ReplayVerdict) -> Result<(), ValidationError> {
    let is_fallback = verdict.assertion_identity.as_ref().is_some_and(|identity| {
        identity.descriptor.category == chaoscontrol_protocol::fallback::FALLBACK_ASSERTION_CATEGORY
    });
    match (is_fallback, verdict.fallback_scope.as_ref()) {
        (false, None) => Ok(()),
        (true, Some(scope)) => {
            let identity = verdict.assertion_identity.as_ref().ok_or_else(|| {
                ValidationError::new(
                    "replay-verdict.fallback_scope: fallback scope requires assertion identity",
                )
            })?;
            identity.validate_for_catalog_admission().map_err(|error| {
                ValidationError::new(format!(
                    "replay-verdict.fallback_scope: invalid assertion identity: {error:?}"
                ))
            })?;
            scope.validate_against(identity).map_err(|error| {
                ValidationError::new(format!(
                    "replay-verdict.fallback_scope: process or record binding mismatch: {error:?}"
                ))
            })
        }
        (false, Some(_)) => Err(ValidationError::new(
            "replay-verdict.fallback_scope: non-fallback assertions cannot claim fallback scope",
        )),
        (true, None) => Err(ValidationError::new(
            "replay-verdict.fallback_scope: fallback assertions require process scope",
        )),
    }
}

/// Validate internal consistency of an emitted replay verdict, for every
/// replay class. This check is class-agnostic; use
/// [`validate_accepted_proof`] for the accepted-proof gate.
pub fn validate_verdict_consistency(verdict: &ReplayVerdict) -> Result<(), ValidationError> {
    validate_verdict_schema(verdict.schema_version)?;
    if verdict.run_id.is_empty() {
        return Err(ValidationError::new(
            "replay-verdict.run_id: must be non-empty",
        ));
    }
    let reproduced = verdict.replay_class == ReplayClass::SnapshotBackedReproduced;
    if verdict.reproduced != reproduced {
        return Err(ValidationError::new(format!(
            "replay-verdict.reproduced: conflicts with replay_class {}",
            replay_class_str(verdict.replay_class)
        )));
    }
    let expected_exit = if reproduced {
        REPRODUCED_EXIT_STATUS
    } else {
        crate::dto::NOT_REPRODUCED_EXIT_STATUS
    };
    if verdict.command.exit_status != expected_exit {
        return Err(ValidationError::new(format!(
            "replay-verdict.command.exit_status: expected {expected_exit} for replay_class {}",
            replay_class_str(verdict.replay_class)
        )));
    }
    for hash in &verdict.artifact_hashes {
        validate_artifact_hash(hash)?;
    }
    if let Some(variant) = verdict.schedule_variant.as_ref() {
        validate_schedule_variant(variant)?;
    }
    validate_fallback_scope(verdict)?;

    match verdict.replay_class {
        ReplayClass::SnapshotBackedReproduced | ReplayClass::SnapshotBackedNotReproduced => {
            require_bug_context(verdict)?;
            if verdict.replay_parent_depth == Some(0) {
                return Err(ValidationError::new(
                    "replay-verdict.replay_parent_depth: snapshot-backed classes require depth > 0",
                ));
            }
            if verdict.snapshot.status != SnapshotValidationStatus::Valid
                || !verdict.snapshot.present
                || !verdict.snapshot.digest_verified
            {
                return Err(ValidationError::new(
                    "replay-verdict.snapshot: snapshot-backed classes require a valid verified snapshot",
                ));
            }
            let reference = verdict.snapshot.reference.as_ref().ok_or_else(|| {
                ValidationError::new(
                    "replay-verdict.snapshot.reference: required for snapshot-backed classes",
                )
            })?;
            validate_snapshot_ref_shape(reference)?;
        }
        ReplayClass::ScheduleOnlyReplayGap => {
            require_bug_context(verdict)?;
            let depth_zero = verdict.replay_parent_depth == Some(0);
            match verdict.snapshot.status {
                SnapshotValidationStatus::NotRequired => {}
                SnapshotValidationStatus::Valid if depth_zero => {
                    let reference = verdict.snapshot.reference.as_ref().ok_or_else(|| {
                        ValidationError::new(
                            "replay-verdict.snapshot.reference: required when snapshot status is valid",
                        )
                    })?;
                    validate_snapshot_ref_shape(reference)?;
                }
                other => {
                    return Err(ValidationError::new(format!(
                        "replay-verdict.snapshot.status: {} is inconsistent with schedule_only_replay_gap",
                        snapshot_status_str(other)
                    )));
                }
            }
        }
        ReplayClass::MissingSnapshotRef => {
            require_bug_context(verdict)?;
            if verdict.snapshot.status != SnapshotValidationStatus::MissingRef {
                return Err(ValidationError::new(
                    "replay-verdict.snapshot.status: expected missing_ref for missing_snapshot_ref",
                ));
            }
        }
        ReplayClass::MissingSnapshotArtifact => {
            require_bug_context(verdict)?;
            if verdict.snapshot.status != SnapshotValidationStatus::MissingArtifact {
                return Err(ValidationError::new(
                    "replay-verdict.snapshot.status: expected missing_artifact for missing_snapshot_artifact",
                ));
            }
        }
        ReplayClass::InvalidSnapshotDigest => {
            require_bug_context(verdict)?;
            if !matches!(
                verdict.snapshot.status,
                SnapshotValidationStatus::InvalidDigest | SnapshotValidationStatus::InvalidRef
            ) {
                return Err(ValidationError::new(
                    "replay-verdict.snapshot.status: expected invalid_digest or invalid_ref for invalid_snapshot_digest",
                ));
            }
        }
        ReplayClass::NoBugFound => {
            if verdict.snapshot.status != SnapshotValidationStatus::NotRequired {
                return Err(ValidationError::new(
                    "replay-verdict.snapshot.status: expected not_required for no_bug_found",
                ));
            }
            if verdict.bug_path.is_some()
                || verdict.bug_id.is_some()
                || verdict.assertion_id.is_some()
                || verdict.replay_parent_depth.is_some()
                || verdict.schedule_variant.is_some()
                || verdict.fallback_scope.is_some()
            {
                return Err(ValidationError::new(
                    "replay-verdict: no_bug_found verdicts carry no bug context",
                ));
            }
            if !verdict.artifact_hashes.is_empty() {
                return Err(ValidationError::new(
                    "replay-verdict.artifact_hashes: no_bug_found verdicts carry no artifact hashes",
                ));
            }
        }
        ReplayClass::ReplayError => {}
    }
    Ok(())
}

/// Validate the stricter accepted-proof gate: a verdict only counts as
/// accepted replay proof when it is a current-schema, snapshot-backed
/// reproduction with a valid assertion identity and current snapshot codec.
pub fn validate_accepted_proof(verdict: &ReplayVerdict) -> Result<(), ValidationError> {
    validate_verdict_consistency(verdict)?;
    if verdict.schema_version != REPLAY_VERDICT_SCHEMA_VERSION {
        return Err(ValidationError::new(format!(
            "replay-verdict.schema_version: accepted proof requires {REPLAY_VERDICT_SCHEMA_VERSION}"
        )));
    }
    if verdict.replay_class != ReplayClass::SnapshotBackedReproduced {
        return Err(ValidationError::new(format!(
            "replay-verdict.replay_class: accepted replay proof requires {REQUIRED_REPLAY_CLASS}, got {}",
            replay_class_str(verdict.replay_class)
        )));
    }
    let assertion_id = verdict.assertion_id.ok_or_else(|| {
        ValidationError::new("replay-verdict.assertion_id: required for accepted proof")
    })?;
    let identity = verdict.assertion_identity.as_ref().ok_or_else(|| {
        ValidationError::new(
            "replay-verdict.assertion_identity: legacy assertion ID-only evidence cannot promote",
        )
    })?;
    identity
        .validate_compatibility_alias(assertion_id)
        .map_err(|error| {
            ValidationError::new(format!(
                "replay-verdict.assertion_identity: invalid assertion identity: {error:?}"
            ))
        })?;
    let reference = verdict
        .snapshot
        .reference
        .as_ref()
        .expect("consistency guarantees a snapshot reference for snapshot-backed classes");
    validate_snapshot_ref_current(reference)?;
    Ok(())
}
