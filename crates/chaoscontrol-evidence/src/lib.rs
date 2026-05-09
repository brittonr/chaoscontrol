//! Typed evidence/readiness models shared by Rust-owned ChaosControl gates.
//!
//! This crate intentionally keeps parsing and structural compatibility checks in
//! a small pure core. Filesystem and process orchestration belong in thin CLI or
//! Nix wrapper shells.

use std::collections::BTreeSet;
use std::fmt;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const ACCEPTED_PROOF_SCHEMA_VERSION: u64 = 1;
pub const CHUNK_MANIFEST_SCHEMA_VERSION: u64 = 1;
pub const REPLAY_VERDICT_SCHEMA_VERSION: u64 = 1;
pub const REQUIRED_REPLAY_CLASS: &str = "snapshot_backed_reproduced";
pub const REQUIRED_WORKLOADS: [&str; 2] = ["raft", "redb"];
pub const SUPPORTED_SNAPSHOT_CODECS: [&str; 2] = [
    "simulation-snapshot-cbor-zstd-v2",
    "simulation-snapshot-bincode-zstd-v1",
];
pub const SUPPORTED_SNAPSHOT_SCHEMA_VERSIONS: [u64; 2] = [1, 2];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceError {
    message: String,
}

impl EvidenceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for EvidenceError {}

impl From<serde_json::Error> for EvidenceError {
    fn from(value: serde_json::Error) -> Self {
        Self::new(format!("invalid JSON: {value}"))
    }
}

impl From<io::Error> for EvidenceError {
    fn from(value: io::Error) -> Self {
        Self::new(format!("I/O error: {value}"))
    }
}

pub type EvidenceResult<T> = Result<T, EvidenceError>;

fn ensure(condition: bool, message: impl Into<String>) -> EvidenceResult<()> {
    if condition {
        Ok(())
    } else {
        Err(EvidenceError::new(message))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AcceptedWorkloadProofs {
    pub schema_version: u64,
    pub scope: String,
    pub anti_claims: Vec<String>,
    pub required_replay_class: String,
    pub proofs: Vec<AcceptedWorkloadProof>,
}

impl AcceptedWorkloadProofs {
    pub fn from_json_str(input: &str) -> EvidenceResult<Self> {
        Ok(serde_json::from_str(input)?)
    }

    pub fn from_path(path: impl AsRef<Path>) -> EvidenceResult<Self> {
        let input = std::fs::read_to_string(path)?;
        Self::from_json_str(&input)
    }

    pub fn validate_shape(&self) -> EvidenceResult<()> {
        ensure(
            self.schema_version == ACCEPTED_PROOF_SCHEMA_VERSION,
            format!("manifest schema_version must be {ACCEPTED_PROOF_SCHEMA_VERSION}"),
        )?;
        ensure(
            self.required_replay_class == REQUIRED_REPLAY_CLASS,
            "manifest required_replay_class mismatch",
        )?;
        ensure(
            self.proofs.len() >= REQUIRED_WORKLOADS.len(),
            "manifest must contain at least two independent workload proofs",
        )?;

        let mut workloads = BTreeSet::new();
        for proof in &self.proofs {
            proof.validate_shape()?;
            ensure(
                workloads.insert(proof.workload.as_str()),
                format!("duplicate workload proof: {}", proof.workload),
            )?;
        }
        for required in REQUIRED_WORKLOADS {
            ensure(
                workloads.contains(required),
                format!("manifest must include {required} proof"),
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AcceptedWorkloadProof {
    pub workload: String,
    pub assertion_id: u64,
    pub evidence_dir: String,
    pub summary: String,
    pub bug: String,
    pub verdict: String,
    pub snapshot: String,
    pub notes: Option<String>,
}

impl AcceptedWorkloadProof {
    pub fn validate_shape(&self) -> EvidenceResult<()> {
        ensure(
            !self.workload.is_empty(),
            "proof workload must be non-empty",
        )?;
        ensure(
            !self.evidence_dir.is_empty(),
            "proof evidence_dir must be non-empty",
        )?;
        ensure(!self.summary.is_empty(), "proof summary must be non-empty")?;
        ensure(!self.bug.is_empty(), "proof bug must be non-empty")?;
        ensure(!self.verdict.is_empty(), "proof verdict must be non-empty")?;
        ensure(
            !self.snapshot.is_empty(),
            "proof snapshot must be non-empty",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AcceptedVerdictSummary {
    pub workload: String,
    pub seed: Option<u64>,
    pub snapshot_probe_fail_after: Option<u64>,
    pub run_exit_status: i32,
    pub export_exit_status: i32,
    pub reproduce_exit_status: i32,
    pub bugs: Vec<SummaryBugRef>,
    pub verdict: SummaryVerdictRef,
    pub accepted: bool,
    pub accepted_bug: String,
    pub accepted_verdict: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SummaryBugRef {
    pub file: String,
    pub assertion_id: u64,
    pub replay_parent_depth: u64,
    pub has_snapshot_ref: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SummaryVerdictRef {
    pub path: String,
    pub replay_class: String,
    pub reproduced: bool,
    pub replay_parent_depth: u64,
    pub snapshot_status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BugRecord {
    pub bug_id: u64,
    pub assertion_id: u64,
    pub assertion_location: Option<String>,
    pub tick: Option<u64>,
    pub replay_parent_depth: u64,
    pub replay_parent_snapshot_ref: Option<SnapshotRef>,
    pub dedup_key: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReplayVerdict {
    pub schema_version: u64,
    pub run_id: String,
    pub replay_class: String,
    pub reproduced: bool,
    pub command: ReplayCommand,
    pub diagnostic: String,
    pub bug_path: String,
    pub bug_id: u64,
    pub assertion_id: u64,
    pub replay_parent_depth: u64,
    pub snapshot: SnapshotVerdict,
    pub artifact_hashes: Vec<ArtifactHash>,
}

impl ReplayVerdict {
    pub fn validate_shape(&self) -> EvidenceResult<()> {
        ensure(
            self.schema_version == REPLAY_VERDICT_SCHEMA_VERSION,
            format!("verdict schema_version must be {REPLAY_VERDICT_SCHEMA_VERSION}"),
        )?;
        ensure(!self.run_id.is_empty(), "verdict run_id must be non-empty")?;
        ensure(
            self.replay_class == REQUIRED_REPLAY_CLASS,
            format!("verdict class is {:?}", self.replay_class),
        )?;
        ensure(self.reproduced, "verdict did not reproduce")?;
        ensure(
            self.command.exit_status == 0,
            "verdict command did not exit 0",
        )?;
        ensure(
            self.replay_parent_depth > 0,
            "verdict lacks replay parent depth",
        )?;
        self.snapshot.validate_shape()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReplayCommand {
    pub command: String,
    pub exit_status: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SnapshotVerdict {
    pub status: String,
    pub present: bool,
    pub digest_verified: bool,
    pub reference: SnapshotRef,
}

impl SnapshotVerdict {
    pub fn validate_shape(&self) -> EvidenceResult<()> {
        ensure(self.status == "valid", "snapshot status is not valid")?;
        ensure(self.present, "snapshot not present")?;
        ensure(self.digest_verified, "snapshot digest not verified")?;
        self.reference.validate_shape()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SnapshotRef {
    pub store: String,
    pub digest: String,
    pub codec: String,
    pub schema_version: u64,
    pub path: String,
}

impl SnapshotRef {
    pub fn validate_shape(&self) -> EvidenceResult<()> {
        ensure(
            self.digest.starts_with("sha256:") && self.digest.len() == "sha256:".len() + 64,
            "snapshot digest is not sha256",
        )?;
        ensure(
            SUPPORTED_SNAPSHOT_CODECS.contains(&self.codec.as_str()),
            "unexpected snapshot codec",
        )?;
        ensure(
            SUPPORTED_SNAPSHOT_SCHEMA_VERSIONS.contains(&self.schema_version),
            "unexpected snapshot schema_version",
        )?;
        ensure(!self.path.is_empty(), "snapshot path must be non-empty")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArtifactHash {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SnapshotChunkManifest {
    pub schema_version: u64,
    pub original_path: String,
    pub original_size: u64,
    pub original_sha256: String,
    pub chunks: Vec<SnapshotChunk>,
}

impl SnapshotChunkManifest {
    pub fn validate_shape(&self) -> EvidenceResult<()> {
        ensure(
            self.schema_version == CHUNK_MANIFEST_SCHEMA_VERSION,
            format!("chunk manifest schema_version must be {CHUNK_MANIFEST_SCHEMA_VERSION}"),
        )?;
        ensure(
            !self.original_path.is_empty(),
            "chunk manifest original_path must be non-empty",
        )?;
        ensure(
            self.original_size > 0,
            "chunk manifest original_size invalid",
        )?;
        ensure(
            self.original_sha256.len() == 64
                && self.original_sha256.chars().all(|c| c.is_ascii_hexdigit()),
            "chunk manifest original_sha256 invalid",
        )?;
        ensure(!self.chunks.is_empty(), "chunk manifest has no chunks")?;
        for (idx, chunk) in self.chunks.iter().enumerate() {
            chunk.validate_shape(idx)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SnapshotChunk {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

impl SnapshotChunk {
    fn validate_shape(&self, idx: usize) -> EvidenceResult<()> {
        ensure(
            self.path.starts_with("snapshots/") && !self.path.contains(".."),
            format!("chunk {idx} path invalid"),
        )?;
        ensure(self.size > 0, format!("chunk {idx} size invalid"))?;
        ensure(
            self.sha256.len() == 64 && self.sha256.chars().all(|c| c.is_ascii_hexdigit()),
            format!("chunk {idx} sha256 invalid"),
        )?;
        Ok(())
    }
}
