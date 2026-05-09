//! Typed evidence/readiness models shared by Rust-owned ChaosControl gates.
//!
//! This crate intentionally keeps parsing and structural compatibility checks in
//! a small pure core. Filesystem and process orchestration belong in thin CLI or
//! Nix wrapper shells.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ACCEPTED_PROOF_SCHEMA_VERSION: u64 = 1;
pub const CHUNK_MANIFEST_SCHEMA_VERSION: u64 = 1;
pub const REPLAY_PROOF_COVERAGE_DOC: &str = "docs/replay-proof-coverage.md";
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayProofCoverageLine {
    pub workload: String,
    pub replay_class: String,
    pub assertion_id: u64,
    pub replay_parent_depth: u64,
    pub snapshot_digest: String,
    pub snapshot_storage: SnapshotStorage,
}

impl ReplayProofCoverageLine {
    pub fn render(&self) -> String {
        format!(
            "{}: {}, assertion={}, depth={}, snapshot={} ({})",
            self.workload,
            self.replay_class,
            self.assertion_id,
            self.replay_parent_depth,
            self.snapshot_digest,
            self.snapshot_storage.as_str()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotStorage {
    Raw,
    Chunks,
}

impl SnapshotStorage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Chunks => "chunks",
        }
    }
}

pub fn validate_replay_proof_coverage(
    root: impl AsRef<Path>,
) -> EvidenceResult<Vec<ReplayProofCoverageLine>> {
    let root = root.as_ref();
    let manifest_path = root.join("dogfood-results/accepted-workload-proofs.json");
    let manifest = AcceptedWorkloadProofs::from_path(&manifest_path).map_err(|err| {
        EvidenceError::new(format!("{}: {err}", rel_display(root, &manifest_path)))
    })?;
    manifest.validate_shape()?;

    manifest
        .proofs
        .iter()
        .map(|proof| validate_workload_proof(root, proof))
        .collect()
}

pub fn render_replay_proof_coverage(lines: &[ReplayProofCoverageLine]) -> String {
    let mut output = String::from("replay proof coverage ok:\n");
    for line in lines {
        output.push_str("  ");
        output.push_str(&line.render());
        output.push('\n');
    }
    output
}

pub fn render_replay_proof_coverage_doc(root: impl AsRef<Path>) -> EvidenceResult<String> {
    let root = root.as_ref();
    let manifest_path = root.join("dogfood-results/accepted-workload-proofs.json");
    let manifest = AcceptedWorkloadProofs::from_path(&manifest_path).map_err(|err| {
        EvidenceError::new(format!("{}: {err}", rel_display(root, &manifest_path)))
    })?;
    manifest.validate_shape()?;
    let coverage = validate_replay_proof_coverage(root)?;
    render_replay_proof_coverage_doc_from_parts(&manifest, &coverage)
}

pub fn check_replay_proof_coverage_doc(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let expected = render_replay_proof_coverage_doc(root)?;
    let doc_path = root.join(REPLAY_PROOF_COVERAGE_DOC);
    let actual = std::fs::read_to_string(&doc_path).map_err(|err| {
        EvidenceError::new(format!(
            "missing or unreadable file: {}: {err}",
            rel_display(root, &doc_path)
        ))
    })?;
    ensure(
        actual == expected,
        format!(
            "{} is stale; run `cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- --write-doc .`",
            REPLAY_PROOF_COVERAGE_DOC
        ),
    )
}

pub fn write_replay_proof_coverage_doc(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let rendered = render_replay_proof_coverage_doc(root)?;
    let doc_path = root.join(REPLAY_PROOF_COVERAGE_DOC);
    std::fs::write(&doc_path, rendered).map_err(|err| {
        EvidenceError::new(format!(
            "failed to write {}: {err}",
            rel_display(root, &doc_path)
        ))
    })
}

pub fn render_replay_proof_coverage_doc_from_parts(
    manifest: &AcceptedWorkloadProofs,
    coverage: &[ReplayProofCoverageLine],
) -> EvidenceResult<String> {
    ensure(
        manifest.proofs.len() == coverage.len(),
        "coverage lines do not match manifest proof count",
    )?;

    let mut output = String::new();
    output.push_str("# Replay Proof Coverage\n\n");
    output.push_str("ChaosControl currently has accepted snapshot-backed replay proof coverage for the workloads listed in `dogfood-results/accepted-workload-proofs.json`.\n\n");
    output.push_str("| Workload | Assertion ID | Evidence | Verdict |\n");
    output.push_str("| --- | ---: | --- | --- |\n");
    for proof in &manifest.proofs {
        let line = coverage
            .iter()
            .find(|line| line.workload == proof.workload)
            .ok_or_else(|| {
                EvidenceError::new(format!("missing coverage line for {}", proof.workload))
            })?;
        output.push_str(&format!(
            "| {} | `{}` | `{}/` | `{}` |\n",
            coverage_workload_label(&proof.workload),
            proof.assertion_id,
            proof.evidence_dir,
            line.replay_class
        ));
    }
    output.push('\n');
    output.push_str("The manifest/check are intentionally conservative: every listed proof must have an accepted summary, exported bug artifact, replay verdict with `replay_class = snapshot_backed_reproduced`, `reproduced = true`, `command.exit_status = 0`, `replay_parent_depth > 0`, and either a present digest-matching `.snapshot.bin` artifact or a verified `.snapshot.bin.chunks.json` sidecar whose ordered chunks reconstruct to the referenced digest.\n\n");
    output.push_str("This is workload coverage evidence, not a mathematical or universal determinism proof. It only supports claims about the named bounded workload rails and their committed verdict/snapshot artifacts. Operator-facing supported vs experimental status is generated in `docs/replay-readiness-status.md`. New breadth claims should add a manifest entry plus committed evidence and pass:\n\n");
    output.push_str("```bash\n");
    output.push_str("cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .\n");
    output.push_str(
        "cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- --check-doc .\n",
    );
    output.push_str("python scripts/generate-replay-readiness-report.py --check\n");
    output.push_str("```\n");
    Ok(output)
}

fn coverage_workload_label(workload: &str) -> String {
    match workload {
        "raft" => "Raft".to_string(),
        other => other.to_string(),
    }
}

fn validate_workload_proof(
    root: &Path,
    proof: &AcceptedWorkloadProof,
) -> EvidenceResult<ReplayProofCoverageLine> {
    let evidence_dir = root.join(&proof.evidence_dir);
    let summary_path = evidence_dir.join(&proof.summary);
    let bug_path = evidence_dir.join(&proof.bug);
    let verdict_path = evidence_dir.join(&proof.verdict);
    let snapshot_path = evidence_dir.join(&proof.snapshot);

    let summary: AcceptedVerdictSummary = load_json(root, &summary_path)?;
    let bug: BugRecord = load_json(root, &bug_path)?;
    let verdict: ReplayVerdict = load_json(root, &verdict_path)?;

    ensure(
        summary.accepted,
        format!("{}: summary is not accepted", proof.workload),
    )?;
    ensure(
        summary.export_exit_status == 0,
        format!("{}: export-bugs did not exit 0", proof.workload),
    )?;
    ensure(
        summary.reproduce_exit_status == 0,
        format!("{}: reproduce did not exit 0", proof.workload),
    )?;

    ensure(
        bug.assertion_id == proof.assertion_id,
        format!("{}: bug assertion mismatch", proof.workload),
    )?;
    ensure(
        bug.replay_parent_depth > 0,
        format!("{}: bug lacks replay parent depth", proof.workload),
    )?;
    ensure(
        bug.replay_parent_snapshot_ref.is_some(),
        format!("{}: bug lacks snapshot ref", proof.workload),
    )?;

    verdict
        .validate_shape()
        .map_err(|err| EvidenceError::new(format!("{}: {}", proof.workload, err.message())))?;
    ensure(
        verdict.assertion_id == proof.assertion_id,
        format!("{}: verdict assertion mismatch", proof.workload),
    )?;
    ensure(
        verdict.snapshot.reference.path == proof.snapshot,
        format!(
            "{}: manifest snapshot path disagrees with verdict ref",
            proof.workload
        ),
    )?;

    let (actual_snapshot_sha, storage) = snapshot_artifact_sha256(root, &snapshot_path)?;
    let expected_digest = format!("sha256:{actual_snapshot_sha}");
    ensure(
        verdict.snapshot.reference.digest == expected_digest,
        format!("{}: snapshot digest mismatch", proof.workload),
    )?;

    Ok(ReplayProofCoverageLine {
        workload: proof.workload.clone(),
        replay_class: REQUIRED_REPLAY_CLASS.to_string(),
        assertion_id: proof.assertion_id,
        replay_parent_depth: verdict.replay_parent_depth,
        snapshot_digest: expected_digest,
        snapshot_storage: storage,
    })
}

fn load_json<T>(root: &Path, path: &Path) -> EvidenceResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let input = std::fs::read_to_string(path).map_err(|err| {
        EvidenceError::new(format!(
            "missing or unreadable file: {}: {err}",
            rel_display(root, path)
        ))
    })?;
    serde_json::from_str(&input).map_err(|err| {
        EvidenceError::new(format!(
            "invalid JSON in {}: {err}",
            rel_display(root, path)
        ))
    })
}

fn snapshot_artifact_sha256(
    root: &Path,
    snapshot_path: &Path,
) -> EvidenceResult<(String, SnapshotStorage)> {
    if snapshot_path.exists() {
        return Ok((sha256_file(snapshot_path)?, SnapshotStorage::Raw));
    }

    let manifest_path = snapshot_path.with_file_name(format!(
        "{}.chunks.json",
        snapshot_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| EvidenceError::new("snapshot path has no file name"))?
    ));
    let manifest: SnapshotChunkManifest = load_json(root, &manifest_path)?;
    manifest.validate_shape().map_err(|err| {
        EvidenceError::new(format!(
            "chunk manifest invalid: {}: {}",
            rel_display(root, &manifest_path),
            err.message()
        ))
    })?;

    let snapshot_name = snapshot_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| EvidenceError::new("snapshot path has no file name"))?;
    ensure(
        manifest.original_path == snapshot_name,
        format!(
            "chunk manifest original_path mismatch: {}",
            rel_display(root, &manifest_path)
        ),
    )?;

    let mut aggregate = Sha256::new();
    let mut total_size = 0_u64;
    let evidence_dir = snapshot_path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| EvidenceError::new("snapshot path lacks evidence directory"))?;
    for (idx, chunk) in manifest.chunks.iter().enumerate() {
        let chunk_path = safe_join_relative(evidence_dir, &chunk.path)
            .map_err(|err| EvidenceError::new(format!("chunk {idx} path invalid: {err}")))?;
        ensure(
            chunk_path.exists(),
            format!("snapshot chunk missing: {}", rel_display(root, &chunk_path)),
        )?;
        let metadata = chunk_path.metadata().map_err(|err| {
            EvidenceError::new(format!(
                "snapshot chunk unreadable: {}: {err}",
                rel_display(root, &chunk_path)
            ))
        })?;
        ensure(
            metadata.len() == chunk.size,
            format!(
                "snapshot chunk size mismatch: {}",
                rel_display(root, &chunk_path)
            ),
        )?;
        let actual_chunk_sha = sha256_file(&chunk_path)?;
        ensure(
            actual_chunk_sha == chunk.sha256,
            format!(
                "snapshot chunk hash mismatch: {}",
                rel_display(root, &chunk_path)
            ),
        )?;
        hash_file_into(&chunk_path, &mut aggregate)?;
        total_size += metadata.len();
    }

    let actual = format!("{:x}", aggregate.finalize());
    ensure(
        total_size == manifest.original_size,
        format!(
            "chunk manifest aggregate size mismatch: {}",
            rel_display(root, &manifest_path)
        ),
    )?;
    ensure(
        actual == manifest.original_sha256,
        format!(
            "chunk manifest aggregate hash mismatch: {}",
            rel_display(root, &manifest_path)
        ),
    )?;
    Ok((actual, SnapshotStorage::Chunks))
}

fn safe_join_relative(base: &Path, relative: &str) -> EvidenceResult<PathBuf> {
    let path = Path::new(relative);
    ensure(!path.is_absolute(), "absolute paths are not allowed")?;
    ensure(
        path.components()
            .all(|component| matches!(component, std::path::Component::Normal(_))),
        "path traversal is not allowed",
    )?;
    Ok(base.join(path))
}

fn sha256_file(path: &Path) -> EvidenceResult<String> {
    let mut hasher = Sha256::new();
    hash_file_into(path, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_file_into(path: &Path, hasher: &mut Sha256) -> EvidenceResult<()> {
    let mut file =
        File::open(path).map_err(|err| EvidenceError::new(format!("{}: {err}", path.display())))?;
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

fn rel_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
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
