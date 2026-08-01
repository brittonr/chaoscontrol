//! Typed evidence/readiness models shared by Rust-owned ChaosControl gates.
//!
//! This crate intentionally keeps parsing and structural compatibility checks in
//! a small pure core. Filesystem and process orchestration belong in thin CLI or
//! Nix wrapper shells.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod assertion;
mod assertion_evidence_carrier;
mod assertion_summary_identity;
mod assertion_summary_semantics;
mod bounded_file;
pub mod consistency_checker;
pub mod contract_registry;
pub mod dogfood_guards;
pub mod evidence_contracts;
pub mod in_process_simulator;
mod json_preflight;
pub mod kernel_bundle_initrd;
pub mod kernel_bundle_validation;
mod non_null_option;
pub mod operator_triage;
pub mod profile_projection;
mod profile_projection_spec;
mod profile_projection_verification;
pub mod readiness_promotion_gate;
pub mod replay_readiness_surfaces;
mod replay_verdict_artifact;
mod sdk_local_catalog;
mod sdk_local_event;
mod sdk_local_identity;
mod sdk_local_identity_value;
mod sdk_local_quality;
pub mod sdk_local_report;
mod sdk_local_verdict;
pub mod simulator_profile;
pub use consistency_checker::{
    check_history_path as check_consistency_history_path, history_digest,
    read_history_path as read_consistency_history_path,
    read_report_path as read_consistency_report_path,
    validate_history as validate_consistency_history,
    validate_history_path as validate_consistency_history_path,
    validate_report as validate_consistency_report,
    validate_report_for_history as validate_consistency_report_for_history,
    write_adapter_sample_history_path as write_adapter_sample_consistency_history_path,
    write_check_report_path as write_consistency_check_report_path,
    write_sample_history_path as write_sample_consistency_history_path, CheckerVerdict,
    ConsistencyCheckReport, ConsistencyChecker, Counterexample, HistoryOperation,
    OperationCompletion, OperationHistory, OperationInvocation, RegisterHistoryAdapterConfig,
    RegisterWorkloadHistoryAdapter, SingleRegisterChecker,
};
pub use contract_registry::{validate_contract_registry, validate_contract_registry_json};
pub use dogfood_guards::{
    check_dogfood_artifact_sizes, run_dogfood_guards_selftest, validate_accepted_dogfood_config,
    DEFAULT_MAX_DOGFOOD_ARTIFACT_BYTES,
};
pub use evidence_contracts::{
    check_evidence_contract_fixtures, check_evidence_contracts, run_nickel_examples,
    validate_artifact_hash, validate_assertion_summary, validate_assertion_summary_for_promotion,
    validate_bug_report, validate_bug_report_for_replay, validate_checkpoint_reference,
    validate_markdown_receipt, validate_receipt, validate_receipt_with_root,
    validate_replay_verdict, validate_replay_verdict_with_options, validate_run_config,
    validate_snapshot_ref, validate_snapshot_ref_with_root, EVIDENCE_CONTRACTS_SUCCESS,
};
pub use in_process_simulator::{
    compare_simulator_receipts, compare_simulator_vm_receipt_bridge, run_simulator_adapter,
    run_simulator_adapter_receipt, sample_simulated_fault_hooks, sample_simulator_config,
    sample_simulator_run_evidence, sample_vm_replay_bridge_metadata,
    summarize_simulator_receipt as summarize_in_process_simulator_receipt,
    validate_simulator_config, validate_simulator_receipt,
    validate_simulator_receipt_path as validate_in_process_simulator_receipt_path,
    write_sample_simulator_receipt_path as write_sample_in_process_simulator_receipt_path,
    DeterministicClock, DeterministicRng, DeterministicScheduler, DeterministicSimulatorCore,
    DiskProfile, EntropySource, EvidenceClass, FaultAction, FaultScheduleRef,
    InProcessWorkloadAdapter, NetworkMessage, NetworkProfile, ReceiptBridgeMetadata,
    RegisterSimulatorAdapter, RngPolicy, SchedulerPolicy, SchedulerStep, SimulatedDisk,
    SimulatedFaultHooks, SimulatedNetwork, SimulatorAdapterEvent, SimulatorConfig, SimulatorFault,
    SimulatorObservation, SimulatorOperation, SimulatorOperationResult, SimulatorReceipt,
    SimulatorReceiptComparison, SimulatorReceiptMismatch, SimulatorRunEvidence,
    SimulatorRunSummary, SimulatorVmReceiptBridgeComparison, VirtualClockPolicy,
    VmReplayReceiptBridgeMetadata, WorkloadIdentity, DEFAULT_SIMULATOR_SCOPE,
    SIMULATOR_CONFIG_SCHEMA_VERSION, SIMULATOR_RECEIPT_SCHEMA_VERSION,
};
pub use kernel_bundle_initrd::{
    private_kfunc_init_script, write_private_kfunc_initrd, PrivateKfuncInitrdRequest,
    PrivateKfuncInitrdSummary, PRIVATE_KFUNC_BPFFS_PIN, PRIVATE_KFUNC_BPF_FILE,
    PRIVATE_KFUNC_EXPECTED_KERNEL_RELEASE, PRIVATE_KFUNC_INITRD_SCHEMA_VERSION,
    PRIVATE_KFUNC_LOADER_FILE, PRIVATE_KFUNC_MODULE_FILE,
};
pub use kernel_bundle_validation::{
    expected_kernel_bundle_kvm_observations, extract_kvm_observations,
    kernel_bundle_kvm_rail_receipt, kernel_bundle_receipt_supports_use,
    kernel_bundle_smoke_profile_identity, kernel_bundle_smoke_receipt,
    sample_mantle_private_kfunc_kvm_markers, sample_mantle_private_kfunc_profile,
    sample_mantle_private_kfunc_receipt, validate_kernel_bundle_smoke_profile, BootCase, BpfCase,
    KernelBundleEvidenceUse, KernelBundleKvmRailReceipt, KernelBundleKvmRun,
    KernelBundleKvmScenario, KernelBundleSmokeProfile, KernelBundleSmokeReceipt,
    MantleMaterializationRefs, ModuleCase, OnixKernelBundleRefs, SmokeBounds, SmokeObservation,
    SmokeRunnerEvidence, DEFAULT_KVM_MAX_EXITS, KERNEL_BUNDLE_KVM_EXECUTION_MODE,
    KERNEL_BUNDLE_KVM_MARKER_PREFIX, KERNEL_BUNDLE_SMOKE_ROLE, KERNEL_BUNDLE_SMOKE_SCHEMA_VERSION,
    KERNEL_BUNDLE_SMOKE_SCOPE, KERNEL_BUNDLE_TRANSCRIPT_EXECUTION_MODE,
};
pub use operator_triage::{
    check_operator_triage_runbook_path, committed_operator_triage_runbook_path,
    render_operator_triage_runbook, render_operator_triage_runbook_path,
    write_operator_triage_runbook_path, TriageReceiptSource,
};
pub use readiness_promotion_gate::{
    default_readiness_promotion_paths, run_readiness_promotion_selftest,
    validate_readiness_promotion, validate_readiness_promotion_files,
};
pub use replay_readiness_surfaces::{
    check_readiness_surface_drift,
    execute_fleet_scheduler_receipt_path as execute_replay_readiness_fleet_scheduler_receipt_path,
    execute_hosted_shared_state_receipt_path as execute_replay_readiness_hosted_shared_state_receipt_path,
    execute_multi_hypervisor_campaign_receipt_path as execute_replay_readiness_multi_hypervisor_campaign_receipt_path,
    execute_networked_hosted_scheduler_receipt_path as execute_replay_readiness_networked_hosted_scheduler_receipt_path,
    execute_scheduler_receipt_path as execute_replay_readiness_scheduler_receipt_path,
    render_dashboard as render_replay_readiness_dashboard, render_fleet_triage_index,
    render_fleet_triage_index_path,
    render_multi_hypervisor_campaign_dashboard as render_replay_readiness_multi_hypervisor_campaign_dashboard,
    render_readme_status_block as render_replay_readiness_readme_status_block,
    replace_readme_marker_block as replace_replay_readiness_readme_marker_block,
    run_readiness_surface_drift_selftest,
    sample_decision_receipt as sample_replay_readiness_decision_receipt,
    sample_fleet_scheduler_plan as sample_replay_readiness_fleet_scheduler_plan,
    sample_fleet_scheduler_receipt as sample_replay_readiness_fleet_scheduler_receipt,
    sample_hosted_shared_state_plan as sample_replay_readiness_hosted_shared_state_plan,
    sample_hosted_shared_state_receipt as sample_replay_readiness_hosted_shared_state_receipt,
    sample_multi_hypervisor_campaign_plan as sample_replay_readiness_multi_hypervisor_campaign_plan,
    sample_multi_hypervisor_campaign_receipt as sample_replay_readiness_multi_hypervisor_campaign_receipt,
    sample_networked_hosted_scheduler_plan as sample_replay_readiness_networked_hosted_scheduler_plan,
    sample_networked_hosted_scheduler_receipt as sample_replay_readiness_networked_hosted_scheduler_receipt,
    sample_replay_readiness_receipt,
    sample_scheduler_receipt as sample_replay_readiness_scheduler_receipt,
    summarize_receipt as summarize_replay_readiness_receipt,
    summarize_receipt_path as summarize_replay_readiness_receipt_path,
    update_readme_status_path as update_replay_readiness_readme_status_path,
    validate_decision_receipt as validate_replay_readiness_decision_receipt,
    validate_decision_receipt_path as validate_replay_readiness_decision_receipt_path,
    validate_fleet_scheduler_receipt as validate_replay_readiness_fleet_scheduler_receipt,
    validate_fleet_scheduler_receipt_path as validate_replay_readiness_fleet_scheduler_receipt_path,
    validate_gate_metadata,
    validate_hosted_shared_state_receipt as validate_replay_readiness_hosted_shared_state_receipt,
    validate_hosted_shared_state_receipt_path as validate_replay_readiness_hosted_shared_state_receipt_path,
    validate_multi_hypervisor_campaign_receipt as validate_replay_readiness_multi_hypervisor_campaign_receipt,
    validate_multi_hypervisor_campaign_receipt_path as validate_replay_readiness_multi_hypervisor_campaign_receipt_path,
    validate_networked_hosted_scheduler_receipt as validate_replay_readiness_networked_hosted_scheduler_receipt,
    validate_networked_hosted_scheduler_receipt_path as validate_replay_readiness_networked_hosted_scheduler_receipt_path,
    validate_scheduler_execution_receipt as validate_replay_readiness_scheduler_execution_receipt,
    validate_scheduler_execution_receipt_path as validate_replay_readiness_scheduler_execution_receipt_path,
    validate_scheduler_receipt as validate_replay_readiness_scheduler_receipt,
    validate_scheduler_receipt_path as validate_replay_readiness_scheduler_receipt_path,
    write_dashboard_path as write_replay_readiness_dashboard_path,
    write_decision_receipt_path as write_replay_readiness_decision_receipt_path,
    write_fleet_scheduler_receipt_path as write_replay_readiness_fleet_scheduler_receipt_path,
    write_fleet_triage_index_path,
    write_hosted_shared_state_receipt_path as write_replay_readiness_hosted_shared_state_receipt_path,
    write_multi_hypervisor_campaign_dashboard_path as write_replay_readiness_multi_hypervisor_campaign_dashboard_path,
    write_multi_hypervisor_campaign_receipt_path as write_replay_readiness_multi_hypervisor_campaign_receipt_path,
    write_networked_hosted_scheduler_receipt_path as write_replay_readiness_networked_hosted_scheduler_receipt_path,
    write_scheduler_receipt_path as write_replay_readiness_scheduler_receipt_path,
};
pub use replay_verdict_artifact::{
    validate_snapshot_backed_replay_artifact, ReplayVerdictArtifactSummary,
};
pub use sdk_local_report::{
    check_sdk_assertion_quality_fixtures, check_sdk_assertion_quality_path,
    check_sdk_assertion_quality_report, check_sdk_local_report_tracks, summarize_sdk_local_jsonl,
    summarize_sdk_local_report, write_sdk_local_report, AssertionQualityGate,
    DEFAULT_SDK_LOCAL_EVIDENCE_CLASS,
};

const MAX_EVIDENCE_JSON_BYTES: u64 = 16 * 1024 * 1024;

pub const ACCEPTED_PROOF_SCHEMA_VERSION: u64 = 1;
pub const CHUNK_MANIFEST_SCHEMA_VERSION: u64 = 1;
pub const REPLAY_PROOF_COVERAGE_DOC: &str = "docs/replay-proof-coverage.md";
pub const REPLAY_VERDICT_SCHEMA_VERSION: u64 = 2;
pub const SNAPSHOT_COPY_BUFFER_BYTES: usize = 1024 * 1024;
pub const REQUIRED_REPLAY_CLASS: &str = "snapshot_backed_reproduced";
pub const REQUIRED_WORKLOADS: [&str; 2] = ["raft", "redb"];
pub const SUPPORTED_SNAPSHOT_CODECS: [&str; 2] = [
    "simulation-snapshot-cbor-zstd-v2",
    "simulation-snapshot-bincode-zstd-v1",
];
pub const SUPPORTED_SNAPSHOT_SCHEMA_VERSIONS: [u64; 2] = [1, 2];
const CURRENT_SNAPSHOT_CODEC: &str = "simulation-snapshot-cbor-zstd-v2";
const CURRENT_SNAPSHOT_SCHEMA_VERSION: u64 = 2;

pub const REPLAY_READINESS_STATUS_DOC: &str = "docs/replay-readiness-status.md";
pub const ASSERTION_READINESS_STATUS_DOC: &str = "docs/assertion-readiness-status.md";
pub const LOCAL_ASSERTION_HARNESSES_PATH: &str = "dogfood-results/local-assertion-harnesses.json";
pub const REQUIRED_ASSERTION_SUMMARY_FRAGMENTS: [&str; 2] = [
    "assertion-density and uncovered-catalog view over historical replay evidence",
    "not replay proof by itself",
];
pub const REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS: [&str; 5] = [
    "A high exercised count only says the committed run observed cataloged SDK assertions",
    "Local harness coverage is not snapshot replay evidence",
    "Zero ordinary assertion blockers applies only to accepted v2 assertion evidence",
    "Legacy bare-array assertion artifacts are diagnostic-only",
    "Operator/product readiness still requires separate replay, minimization/reproduction, workload onboarding, and triage evidence",
];
pub const FORBIDDEN_ASSERTION_OVERCLAIM_FRAGMENTS: [&str; 5] = [
    "product parity is established",
    "full antithesis-style product replacement",
    "assertion density proves replay",
    "assertion coverage proves replay",
    "zero assertion blockers proves product parity",
];
pub const SUPPORTED_REPLAY_STATUS: &str = "supported-bounded";
pub const BLOCKED_ASSERTION_IDENTITY_STATUS: &str = "blocked-assertion-identity";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperimentalReplaySurface {
    pub surface: &'static str,
    pub status: &'static str,
    pub reason: &'static str,
    pub promotion_evidence: &'static str,
}

pub const EXPERIMENTAL_REPLAY_SURFACES: [ExperimentalReplaySurface; 8] = [
    ExperimentalReplaySurface {
        surface: "Rust workload authoring",
        status: "experimental-rust-only",
        reason: "New Rust workloads need their own bounded probe, accepted verdict, manifest entry, and committed raw or chunked snapshot artifact before promotion. Non-Rust SDKs are not current product blockers.",
        promotion_evidence: "Committed Rust workload recipe, accepted-verdict wrapper expectation, manifest entry, snapshot artifact, and replay/assertion readiness checks for that Rust workload.",
    },
    ExperimentalReplaySurface {
        surface: "Schedule-only replay",
        status: "gap-evidence-only",
        reason: "Depth-zero replay results classify replay gaps; they do not prove snapshot-backed replay coverage.",
        promotion_evidence: "A reproduced bug with `replay_parent_depth > 0`, valid snapshot ref/artifact or chunks, and `snapshot_backed_reproduced` verdict.",
    },
    ExperimentalReplaySurface {
        surface: "Arbitrary guest/device determinism",
        status: "bounded-matrix-rail",
        reason: "Current evidence includes a bounded hide-TSC device/profile matrix rail (`nix run .#vm-determinism-matrix`) that emits a `matrix-receipt.json` from listed VM determinism observations. Matrix rows bind named single-machine multi-hypervisor product profiles, worker counts, workload identity, kernel/initrd fingerprints, device profile, clock profile, and controller configuration. This is matrix-scoped evidence only; unlisted guests, devices, clock profiles, and timing behaviors remain unproven, and the rail is not a universal hypervisor/device/timing determinism proof.",
        promotion_evidence: "Committed device/profile matrix receipts for each promoted row, visible failing/unsupported rows with bounded mismatch details, negative drift evidence for unsupported profiles, and promotion-gate checks that reject converting the bounded matrix rail into an arbitrary or universal determinism claim.",
    },
    ExperimentalReplaySurface {
        surface: "Operator triage UX",
        status: "local-runbook",
        reason: "Current evidence includes a committed local operator triage runbook generated from replay-readiness receipts and historical diagnostic artifacts. Its blocked sections do not run reproduction or minimization for ID-only bugs. It is not a hosted service or fleet workflow.",
        promotion_evidence: "A promotable local triage path requires fresh admitted v2 KVM evidence, exact bug/report identity binding, replay and minimization commands, and operator decisions without raw-log scraping.",
    },
    ExperimentalReplaySurface {
        surface: "Hosted/fleet triage UI",
        status: "non-goal-current-scope",
        reason: "Hosted UI, SaaS service, and real cross-machine operator workflows are out of current product scope. Local operator triage remains bounded to generated runbooks and local decision receipts.",
        promotion_evidence: "No current-scope promotion path; any future hosted/UI-backed fleet triage would need explicit scope reopening plus evidence that ingests readiness receipts, links bug/replay artifacts, persists shared operator decisions across real machine boundaries, and proves the workflow without raw-log scraping.",
    },
    ExperimentalReplaySurface {
        surface: "Local multi-hypervisor control plane",
        status: "supported-bounded-local",
        reason: "Current evidence includes bounded local sequential scheduler execution, a durable local multi-hypervisor campaign receipt, a real KVM multi-hypervisor smoke rail, worker resource budgets, artifact roots/indexes, queue-state transitions, run receipts, bug follow-up jobs, and local artifact retention. This is a supported one-machine local control-plane workflow only; it is not a hosted service, shared remote queue, cross-machine scheduler, universal fleet-scale throughput claim, or full Antithesis-style product replacement.",
        promotion_evidence: "Keep the committed single-machine multi-hypervisor control-plane receipt, KVM smoke rail, worker budgets, artifact roots/indexes, queue-state transitions, run receipts, bug follow-up jobs, local artifact retention, and anti-overclaim gates green without raw-log scraping or hosted/cross-machine claims.",
    },
    ExperimentalReplaySurface {
        surface: "FoundationDB-style in-process deterministic simulator",
        status: "adapter-simulator-receipt",
        reason: "Current evidence includes a Rust-owned in-process simulator adapter receipt emitted by `in-process-simulator-receipt`; it binds deterministic scheduler, virtual clock, RNG, simulated network/disk hooks, fault schedule, history, output digests, and sim-vm bridge metadata for workload/adapter/scenario comparison. This is adapter-simulator evidence only: not VM replay proof, not arbitrary binary support, and not full FoundationDB parity.",
        promotion_evidence: "Committed simulator receipts for promoted workload adapters, negative nondeterminism fixtures, sim-vm bridge comparisons that preserve simulator-local vs vm-snapshot-replay evidence classes, readiness gates that reject VM-replay or full-FoundationDB overclaims, and separate VMM replay evidence before any replay-product claim.",
    },
    ExperimentalReplaySurface {
        surface: "Full Antithesis-style product replacement",
        status: "non-goal-current-scope",
        reason: "Full Antithesis-style hosted product replacement is not the current product target; no hosted service, broad workload catalog, fleet-scale scheduler, UI, or formal determinism theorem is claimed by this evidence.",
        promotion_evidence: "No current-scope promotion path; no existing bounded local/Rust rail may imply full Antithesis-style product parity.",
    },
];

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedSnapshot {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
}

impl MaterializedSnapshot {
    pub fn render(&self) -> String {
        format!(
            "materialized {} sha256:{} size={}",
            self.path.display(),
            self.sha256,
            self.size
        )
    }
}

pub fn materialize_snapshot_chunks(
    manifest_path: impl AsRef<Path>,
    force: bool,
) -> EvidenceResult<MaterializedSnapshot> {
    let manifest_path = manifest_path.as_ref();
    let manifest: SnapshotChunkManifest =
        load_json(Path::new(""), manifest_path).map_err(|err| {
            EvidenceError::new(format!(
                "chunk manifest invalid: {}: {}",
                manifest_path.display(),
                err.message()
            ))
        })?;
    manifest.validate_shape()?;

    ensure(
        manifest.original_path.ends_with(".snapshot.bin")
            && !manifest.original_path.contains('/')
            && !manifest.original_path.contains(".."),
        format!(
            "chunk manifest original_path must be a local snapshot filename: {}",
            manifest_path.display()
        ),
    )?;

    let snapshot_dir = manifest_path.parent().ok_or_else(|| {
        EvidenceError::new(format!(
            "chunk manifest has no parent directory: {}",
            manifest_path.display()
        ))
    })?;
    let evidence_dir = snapshot_dir.parent().ok_or_else(|| {
        EvidenceError::new(format!(
            "chunk manifest lacks evidence directory: {}",
            manifest_path.display()
        ))
    })?;
    let original_path = snapshot_dir.join(&manifest.original_path);
    ensure(
        force || !original_path.exists(),
        format!(
            "raw snapshot already exists: {} (use --force)",
            original_path.display()
        ),
    )?;

    let tmp_path = original_path.with_file_name(format!("{}.tmp", manifest.original_path));
    let materialized =
        write_materialized_snapshot(&manifest, evidence_dir, &original_path, &tmp_path);
    if materialized.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    materialized
}

fn write_materialized_snapshot(
    manifest: &SnapshotChunkManifest,
    evidence_dir: &Path,
    original_path: &Path,
    tmp_path: &Path,
) -> EvidenceResult<MaterializedSnapshot> {
    let mut aggregate = Sha256::new();
    let mut total_size = 0_u64;
    let mut output = File::create(tmp_path).map_err(|err| {
        EvidenceError::new(format!(
            "failed to create temp snapshot {}: {err}",
            tmp_path.display()
        ))
    })?;

    for (idx, chunk) in manifest.chunks.iter().enumerate() {
        let chunk_path = safe_join_relative(evidence_dir, &chunk.path)
            .map_err(|err| EvidenceError::new(format!("chunk {idx} path invalid: {err}")))?;
        ensure(
            chunk_path.exists(),
            format!("snapshot chunk missing: {}", chunk_path.display()),
        )?;
        let metadata = chunk_path.metadata().map_err(|err| {
            EvidenceError::new(format!(
                "snapshot chunk unreadable: {}: {err}",
                chunk_path.display()
            ))
        })?;
        ensure(
            metadata.len() == chunk.size,
            format!(
                "snapshot chunk size mismatch: {} expected={} actual={}",
                chunk_path.display(),
                chunk.size,
                metadata.len()
            ),
        )?;
        let actual_chunk_sha = sha256_file(&chunk_path)?;
        ensure(
            actual_chunk_sha == chunk.sha256,
            format!(
                "snapshot chunk hash mismatch: {} expected={} actual={}",
                chunk_path.display(),
                chunk.sha256,
                actual_chunk_sha
            ),
        )?;
        copy_file_into(&chunk_path, &mut output, &mut aggregate)?;
        total_size += metadata.len();
    }
    drop(output);

    ensure(
        total_size == manifest.original_size,
        format!(
            "aggregate size mismatch: {} expected={} actual={}",
            original_path.display(),
            manifest.original_size,
            total_size
        ),
    )?;
    let actual = format!("{:x}", aggregate.finalize());
    ensure(
        actual == manifest.original_sha256,
        format!(
            "aggregate hash mismatch: {} expected={} actual={}",
            original_path.display(),
            manifest.original_sha256,
            actual
        ),
    )?;
    std::fs::rename(tmp_path, original_path).map_err(|err| {
        EvidenceError::new(format!(
            "failed to install materialized snapshot {}: {err}",
            original_path.display()
        ))
    })?;

    Ok(MaterializedSnapshot {
        path: original_path.to_path_buf(),
        sha256: actual,
        size: total_size,
    })
}

pub fn write_snapshot_chunk_fixture(root: impl AsRef<Path>) -> EvidenceResult<PathBuf> {
    let root = root.as_ref();
    let snapshots = root.join("snapshots");
    std::fs::create_dir(&snapshots).map_err(|err| {
        EvidenceError::new(format!(
            "failed to create fixture snapshot dir {}: {err}",
            snapshots.display()
        ))
    })?;
    let parts: [&[u8]; 3] = [b"alpha", b"-beta", b"-gamma"];
    let original = parts.concat();
    let digest = sha256_bytes(&original);
    let mut chunks = Vec::new();
    for (idx, data) in parts.iter().enumerate() {
        let name = format!("{digest}.snapshot.bin.part{idx:02}");
        let path = snapshots.join(&name);
        std::fs::write(&path, data).map_err(|err| {
            EvidenceError::new(format!(
                "failed to write fixture chunk {}: {err}",
                path.display()
            ))
        })?;
        chunks.push(SnapshotChunk {
            path: format!("snapshots/{name}"),
            size: data.len() as u64,
            sha256: sha256_bytes(data),
        });
    }

    let manifest = SnapshotChunkManifest {
        schema_version: CHUNK_MANIFEST_SCHEMA_VERSION,
        original_path: format!("{digest}.snapshot.bin"),
        original_size: original.len() as u64,
        original_sha256: digest.clone(),
        chunks,
    };
    let manifest_path = snapshots.join(format!("{digest}.snapshot.bin.chunks.json"));
    let rendered = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(&manifest_path, format!("{rendered}\n")).map_err(|err| {
        EvidenceError::new(format!(
            "failed to write fixture manifest {}: {err}",
            manifest_path.display()
        ))
    })?;
    Ok(manifest_path)
}

pub fn run_materialize_snapshot_chunks_selftest() -> EvidenceResult<()> {
    let temp = tempfile::tempdir()?;
    let manifest_path = write_snapshot_chunk_fixture(temp.path())?;
    let result = materialize_snapshot_chunks(&manifest_path, false)?;
    ensure(
        result.path.exists() && result.render().contains("sha256:"),
        "positive materialization result malformed",
    )?;

    let temp = tempfile::tempdir()?;
    let manifest_path = write_snapshot_chunk_fixture(temp.path())?;
    let manifest: SnapshotChunkManifest = load_json(Path::new(""), &manifest_path)?;
    let missing = temp.path().join(&manifest.chunks[1].path);
    std::fs::remove_file(&missing)?;
    let err = materialize_snapshot_chunks(&manifest_path, true)
        .expect_err("missing chunk materialization should fail");
    ensure(
        err.message().contains("snapshot chunk missing"),
        format!("missing chunk error mismatch: {}", err.message()),
    )?;
    ensure(
        !manifest_path
            .with_file_name(format!("{}.tmp", manifest.original_path))
            .exists(),
        "missing chunk left a partial .tmp snapshot",
    )?;

    let temp = tempfile::tempdir()?;
    let manifest_path = write_snapshot_chunk_fixture(temp.path())?;
    let mut manifest: SnapshotChunkManifest = load_json(Path::new(""), &manifest_path)?;
    manifest.chunks.swap(0, 1);
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    let err = materialize_snapshot_chunks(&manifest_path, true)
        .expect_err("reordered chunks should fail");
    ensure(
        err.message().contains("aggregate hash mismatch"),
        format!("reordered chunks error mismatch: {}", err.message()),
    )?;

    let temp = tempfile::tempdir()?;
    let manifest_path = write_snapshot_chunk_fixture(temp.path())?;
    let manifest: SnapshotChunkManifest = load_json(Path::new(""), &manifest_path)?;
    let corrupt = temp.path().join(&manifest.chunks[0].path);
    std::fs::write(corrupt, b"ALPHA")?;
    let err =
        materialize_snapshot_chunks(&manifest_path, true).expect_err("corrupt chunk should fail");
    ensure(
        err.message().contains("snapshot chunk hash mismatch"),
        format!("corrupt chunk error mismatch: {}", err.message()),
    )
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

pub fn review_replay_proof_coverage(
    root: impl AsRef<Path>,
) -> EvidenceResult<Vec<ReplayProofCoverageLine>> {
    let root = root.as_ref();
    let manifest_path = root.join("dogfood-results/accepted-workload-proofs.json");
    let manifest = AcceptedWorkloadProofs::from_path(&manifest_path).map_err(|error| {
        EvidenceError::new(format!("{}: {error}", rel_display(root, &manifest_path)))
    })?;
    manifest.validate_shape()?;
    manifest
        .proofs
        .iter()
        .map(|proof| review_workload_proof(root, proof))
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
    let coverage = manifest
        .proofs
        .iter()
        .map(|proof| review_workload_proof(root, proof))
        .collect::<EvidenceResult<Vec<_>>>()?;
    render_replay_proof_coverage_doc_from_parts(&manifest, &coverage)
}

pub fn check_replay_proof_coverage_doc(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let expected = render_replay_proof_coverage_doc(root)?;
    let doc_path = root.join(REPLAY_PROOF_COVERAGE_DOC);
    let actual = bounded_file::read_bounded_regular_file(&doc_path, MAX_EVIDENCE_JSON_BYTES)?;
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

    let promoted = coverage
        .iter()
        .filter(|line| line.replay_class == REQUIRED_REPLAY_CLASS)
        .count();
    let blocked = coverage.len() - promoted;
    let mut output = String::new();
    output.push_str("# Replay Proof Coverage\n\n");
    output.push_str(&format!(
        "The manifest retains historical snapshot-backed replay artifacts. Current assertion-identity admission promotes {promoted} workload(s) and blocks {blocked} workload(s).\n\n"
    ));
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
    output.push_str("A promoted proof requires an accepted summary, exported bug artifact, replay verdict, retained snapshot, and accepted v2 assertion summary. The v2 summary must bind the selected alias to one admitted structured descriptor. Historical bare-array assertion files remain diagnostic-only.\n\n");
    output.push_str("This is workload coverage evidence, not a mathematical or universal determinism proof. A blocked row does not support a current bounded replay claim. Fresh admitted KVM evidence must pass:\n\n");
    output.push_str("```bash\n");
    output.push_str("cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .\n");
    output.push_str(
        "cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- --check-doc .\n",
    );
    output.push_str(
        "cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check .\n",
    );
    output.push_str("```\n");
    Ok(output)
}

pub fn render_replay_readiness_status(root: impl AsRef<Path>) -> EvidenceResult<String> {
    let root = root.as_ref();
    let manifest_path = root.join("dogfood-results/accepted-workload-proofs.json");
    let manifest = AcceptedWorkloadProofs::from_path(&manifest_path).map_err(|err| {
        EvidenceError::new(format!("{}: {err}", rel_display(root, &manifest_path)))
    })?;
    manifest.validate_shape()?;
    ensure(
        !manifest.proofs.is_empty(),
        "accepted workload proof manifest has no proofs",
    )?;

    let reviews = manifest
        .proofs
        .iter()
        .map(|proof| review_workload_proof(root, proof))
        .collect::<EvidenceResult<Vec<_>>>()?;
    let promoted_workloads = reviews
        .iter()
        .filter(|line| line.replay_class == REQUIRED_REPLAY_CLASS)
        .map(|line| format!("`{}`", line.workload))
        .collect::<Vec<_>>();
    let mut output = String::new();
    output.push_str("# Replay Readiness Status\n\n");
    output.push_str("Generated from `dogfood-results/accepted-workload-proofs.json`. Do not hand-edit this file; run `cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --write .`.\n\n");
    output.push_str("## Summary\n\n");
    if promoted_workloads.is_empty() {
        output.push_str("ChaosControl currently has no workload with fresh admitted v2 KVM evidence. Historical replay artifacts remain diagnostic and blocked from current promotion.\n\n");
    } else {
        output.push_str(&format!(
            "ChaosControl currently supports bounded snapshot-backed replay proof claims for: {}.\n\n",
            promoted_workloads.join(", ")
        ));
    }
    output.push_str("Current product target: Rust-only workload support on one machine with multiple local ChaosControl hypervisors. The supported local control-plane baseline now covers durable one-machine multi-hypervisor orchestration; remaining product gaps are Rust workload authoring/onboarding, bounded determinism/fault coverage expansion, local triage depth, and local artifact hygiene. Hosted services, cross-machine fleet scheduling, and non-Rust SDKs are out of current product scope even though their claims remain forbidden overclaims.\n\n");
    output.push_str("This status is evidence-backed but narrow: it is not a mathematical determinism proof, not a universal hypervisor/device/timing proof, and not a full Antithesis-style product replacement claim.\n\n");
    output.push_str("## Bounded replay evidence promotion status\n\n");
    output.push_str("| Workload | Status | Assertion ID | Historical verdict | Replay parent depth | export/reproduce exit | Evidence |\n");
    output.push_str("| --- | --- | ---: | --- | ---: | --- | --- |\n");
    for (proof, review) in manifest.proofs.iter().zip(&reviews) {
        output.push_str(&render_replay_readiness_proof_row(root, proof, review)?);
        output.push('\n');
    }
    output.push('\n');
    output.push_str("`blocked-assertion-identity` means that the retained replay files predate admitted v2 structured assertion identity. Numeric alias agreement cannot promote them. Fresh KVM evidence must bind the selected alias and complete catalog through an accepted v2 summary.\n\n");
    output.push_str("## Experimental or unproven surfaces\n\n");
    output
        .push_str("| Surface | Status | Why it is not promoted | Required promotion evidence |\n");
    output.push_str("| --- | --- | --- | --- |\n");
    for item in EXPERIMENTAL_REPLAY_SURFACES {
        output.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            item.surface, item.status, item.reason, item.promotion_evidence
        ));
    }
    output.push('\n');
    output.push_str("## Promotion rule\n\n");
    output.push_str("A new surface can move into `supported-bounded` only after it has committed evidence in the accepted workload manifest and all of these checks pass:\n\n");
    output.push_str("```bash\n");
    output.push_str("cargo run -p chaoscontrol-evidence --bin check-replay-proof-coverage -- .\n");
    output.push_str(
        "cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --check .\n",
    );
    output.push_str(
        "cargo run -p chaoscontrol-evidence --bin check-readiness-promotion-gate -- --root .\n",
    );
    output.push_str("nix build .#checks.x86_64-linux.evidence-contracts --no-link -L\n");
    output.push_str("```\n");
    Ok(output)
}

fn render_replay_readiness_proof_row(
    root: &Path,
    proof: &AcceptedWorkloadProof,
    review: &ReplayProofCoverageLine,
) -> EvidenceResult<String> {
    let evidence_dir = root.join(&proof.evidence_dir);
    let verdict: ReplayVerdict = load_json(root, &evidence_dir.join(&proof.verdict))?;
    let summary: AcceptedVerdictSummary = load_json(root, &evidence_dir.join(&proof.summary))?;
    let status = if review.replay_class == REQUIRED_REPLAY_CLASS {
        SUPPORTED_REPLAY_STATUS
    } else {
        BLOCKED_ASSERTION_IDENTITY_STATUS
    };
    Ok(format!(
        "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` / `{}` | `{}/` |",
        proof.workload,
        status,
        proof.assertion_id,
        verdict.replay_class,
        verdict.replay_parent_depth,
        summary.export_exit_status,
        summary.reproduce_exit_status,
        proof.evidence_dir
    ))
}

pub fn check_replay_readiness_status(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let expected = render_replay_readiness_status(root)?;
    let report_path = root.join(REPLAY_READINESS_STATUS_DOC);
    let actual = bounded_file::read_bounded_regular_file(&report_path, MAX_EVIDENCE_JSON_BYTES)?;
    ensure(
        actual == expected,
        "readiness report stale: run cargo run -p chaoscontrol-evidence --bin generate-replay-readiness-report -- --write .",
    )
}

pub fn write_replay_readiness_status(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let rendered = render_replay_readiness_status(root)?;
    let report_path = root.join(REPLAY_READINESS_STATUS_DOC);
    std::fs::write(&report_path, rendered).map_err(|err| {
        EvidenceError::new(format!(
            "failed to write {}: {err}",
            rel_display(root, &report_path)
        ))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionReadinessRow {
    pub workload: String,
    pub identity_status: String,
    pub identity_blocker: Option<String>,
    pub total: usize,
    pub exercised: usize,
    pub always: usize,
    pub sometimes: usize,
    pub reachability: usize,
    pub unreachable: usize,
    pub uncategorized: usize,
    pub nonpassing: usize,
    pub replay_probe_failures: usize,
    pub evidence_path: String,
    pub gaps: Vec<String>,
    pub replay_probe_signals: Vec<String>,
    pub gap_details: Vec<AssertionGapDetail>,
    pub local_fixture_covered: Vec<String>,
}

impl AssertionReadinessRow {
    fn render(&self) -> String {
        format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` / `{}` / `{}` / `{}` | `{}` | `{}` | `{}` | `{}` |",
            self.workload,
            self.identity_status,
            self.total,
            self.exercised,
            self.always,
            self.sometimes,
            self.reachability,
            self.unreachable,
            self.uncategorized,
            self.nonpassing,
            self.replay_probe_failures,
            self.evidence_path
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionGapDetail {
    pub workload: String,
    pub gap_class: AssertionGapClass,
    pub label: String,
    pub kind: String,
    pub category: String,
    pub category_inferred: bool,
    pub verdict: String,
    pub hit_count: i64,
}

impl AssertionGapDetail {
    fn render(&self) -> String {
        let category = if self.category_inferred {
            format!("{} (inferred)", self.category)
        } else {
            self.category.clone()
        };
        format!(
            "- {} / {}: `{}` (kind={}, category={}, verdict={}, hit_count={})",
            self.workload,
            self.gap_class.as_str(),
            self.label,
            self.kind,
            category,
            self.verdict,
            self.hit_count
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionGapClass {
    Unhit,
    NonPassing,
}

impl AssertionGapClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unhit => "unhit",
            Self::NonPassing => "non-passing",
        }
    }
}

pub fn render_assertion_readiness_status(root: impl AsRef<Path>) -> EvidenceResult<String> {
    let root = root.as_ref();
    let manifest_path = root.join("dogfood-results/accepted-workload-proofs.json");
    let manifest = AcceptedWorkloadProofs::from_path(&manifest_path).map_err(|err| {
        EvidenceError::new(format!("{}: {err}", rel_display(root, &manifest_path)))
    })?;
    manifest.validate_shape()?;
    ensure(
        !manifest.proofs.is_empty(),
        "accepted workload proof manifest has no proofs",
    )?;

    let rows = manifest
        .proofs
        .iter()
        .map(|proof| assertion_readiness_row(root, proof))
        .collect::<EvidenceResult<Vec<_>>>()?;

    let mut output = String::new();
    output.push_str("# Assertion Readiness Status\n\n");
    output.push_str("Generated from `dogfood-results/accepted-workload-proofs.json` and each committed `assertions.json`. Do not hand-edit this file; run `cargo run -p chaoscontrol-evidence --bin generate-assertion-readiness-report -- --write .`.\n\n");
    output.push_str("## Summary\n\n");
    output.push_str("This report is an assertion-density and uncovered-catalog view over historical replay evidence plus explicitly-labeled deterministic local assertion harnesses. It helps decide whether a workload is richly instrumented enough to be a credible Antithesis-alternative rail, but it is not replay proof by itself.\n\n");
    output.push_str("Legacy bare-array assertion artifacts are diagnostic-only. Only an accepted v2 summary with a complete admitted structured catalog can qualify for promotion.\n\n");
    output.push_str("## Assertion evidence status\n\n");
    output.push_str("| Workload | Identity status | Cataloged | Exercised | always / sometimes / reachability / unreachable | Uncategorized | Non-passing | Replay probe failures | Evidence |\n");
    output.push_str("| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | --- |\n");
    for row in &rows {
        output.push_str(&row.render());
        output.push('\n');
    }
    output.push_str("\n## Promotion guidance\n\n");
    output.push_str("Before promotion, each workload must have accepted v2 assertion identity. Category or coverage rationale cannot waive this identity requirement.\n\n");
    for row in &rows {
        if let Some(blocker) = &row.identity_blocker {
            output.push_str(&format!(
                "- {}: identity status `{}` blocks promotion: {}. Fresh admitted v2 KVM evidence is required.\n",
                row.workload, row.identity_status, blocker
            ));
        }
        for gap in &row.gaps {
            output.push_str("- ");
            output.push_str(gap);
            output.push('\n');
        }
    }
    output.push_str("\n## Replay proof signals\n\n");
    output.push_str("Historical replay-probe failures remain checked diagnostic evidence. They do not provide current snapshot-replay authority or ordinary instrumentation-readiness promotion.\n\n");
    let mut replay_probe_signals = rows
        .iter()
        .flat_map(|row| row.replay_probe_signals.iter())
        .cloned()
        .collect::<Vec<_>>();
    replay_probe_signals.sort();
    if replay_probe_signals.is_empty() {
        output.push_str("- No replay-probe failure signals in historical proof artifacts.\n");
    } else {
        for signal in replay_probe_signals {
            output.push_str("- ");
            output.push_str(&signal);
            output.push('\n');
        }
    }

    output.push_str("\n## Gap details\n\n");
    output.push_str("These details are derived from committed historical `assertions.json` artifacts, deterministic report-local category inference, and optional local assertion harness fixtures. Inferred categories and local-harness coverage are marked. No fresh VM campaign is implied.\n\n");
    let mut rendered_details = rows
        .iter()
        .flat_map(|row| row.gap_details.iter())
        .map(AssertionGapDetail::render)
        .collect::<Vec<_>>();
    rendered_details.sort();
    if rendered_details.is_empty() {
        output.push_str(
            "- No unhit or non-passing assertion details in historical proof artifacts.\n",
        );
    } else {
        for detail in rendered_details {
            output.push_str(&detail);
            output.push('\n');
        }
    }
    output.push_str("\n## Local deterministic assertion harness coverage\n\n");
    let mut local_covered = rows
        .iter()
        .flat_map(|row| {
            row.local_fixture_covered
                .iter()
                .map(|detail| format!("{}: {detail}", row.workload))
        })
        .collect::<Vec<_>>();
    local_covered.sort();
    if local_covered.is_empty() {
        output.push_str("- No historical proof gaps are covered by local deterministic assertion harness fixtures.\n");
    } else {
        for detail in local_covered {
            output.push_str("- ");
            output.push_str(&detail);
            output.push('\n');
        }
    }

    output.push_str("\n## Operator interpretation\n\n");
    output.push_str("Zero ordinary assertion blockers applies only to accepted v2 assertion evidence after deterministic local harness coverage is applied. Diagnostic-only rows cannot promote. Any future accepted result is an instrumentation-readiness signal only. It does not establish hosted-product parity. Operator/product readiness still requires separate replay, minimization/reproduction, workload onboarding, and triage evidence.\n");

    output.push_str("\n## Anti-claim\n\n");
    output.push_str("A high exercised count only says the committed run observed cataloged SDK assertions or that a clearly-labeled local deterministic harness covered a previously unhit assertion condition. Local harness coverage is not snapshot replay evidence. Replay-probe failure visibility is proof-signal accounting, not an application invariant failure. Product parity still requires workload setup ergonomics, replay evidence, minimization/reproduction UX, and operator triage surfaces outside this report.\n");
    Ok(output)
}

pub fn check_assertion_readiness_status(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let expected = render_assertion_readiness_status(root)?;
    let report_path = root.join(ASSERTION_READINESS_STATUS_DOC);
    let actual = bounded_file::read_bounded_regular_file(&report_path, MAX_EVIDENCE_JSON_BYTES)?;
    ensure(
        actual == expected,
        "assertion readiness report stale: run cargo run -p chaoscontrol-evidence --bin generate-assertion-readiness-report -- --write .",
    )
}

pub fn write_assertion_readiness_status(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let rendered = render_assertion_readiness_status(root)?;
    let report_path = root.join(ASSERTION_READINESS_STATUS_DOC);
    std::fs::write(&report_path, rendered).map_err(|err| {
        EvidenceError::new(format!(
            "failed to write {}: {err}",
            rel_display(root, &report_path)
        ))
    })
}

pub fn check_assertion_readiness_promotion(root: impl AsRef<Path>) -> EvidenceResult<Vec<String>> {
    let root = root.as_ref();
    check_assertion_readiness_promotion_paths(
        root,
        root.join("dogfood-results/accepted-workload-proofs.json"),
        root.join(ASSERTION_READINESS_STATUS_DOC),
    )
}

pub fn check_assertion_readiness_boundary(root: impl AsRef<Path>) -> EvidenceResult<Vec<String>> {
    let root = root.as_ref();
    let manifest_path = root.join("dogfood-results/accepted-workload-proofs.json");
    let report_path = root.join(ASSERTION_READINESS_STATUS_DOC);
    let manifest = AcceptedWorkloadProofs::from_path(&manifest_path).map_err(|error| {
        EvidenceError::new(format!("{}: {error}", rel_display(root, &manifest_path)))
    })?;
    manifest.validate_shape()?;
    let report = bounded_file::read_bounded_regular_file(&report_path, MAX_EVIDENCE_JSON_BYTES)?;
    validate_assertion_readiness_boundary(root, &manifest, &report)
}

pub fn check_assertion_readiness_promotion_paths(
    root: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
    report_path: impl AsRef<Path>,
) -> EvidenceResult<Vec<String>> {
    let root = root.as_ref();
    let manifest_path = manifest_path.as_ref();
    let report_path = report_path.as_ref();
    let manifest = AcceptedWorkloadProofs::from_path(manifest_path).map_err(|err| {
        EvidenceError::new(format!("{}: {err}", rel_display(root, manifest_path)))
    })?;
    manifest.validate_shape()?;
    let report = bounded_file::read_bounded_regular_file(report_path, MAX_EVIDENCE_JSON_BYTES)?;
    validate_assertion_readiness_promotion(root, &manifest, &report)
}

pub fn validate_assertion_readiness_promotion(
    root: &Path,
    manifest: &AcceptedWorkloadProofs,
    report: &str,
) -> EvidenceResult<Vec<String>> {
    validate_assertion_readiness(root, manifest, report, true)
}

pub fn validate_assertion_readiness_boundary(
    root: &Path,
    manifest: &AcceptedWorkloadProofs,
    report: &str,
) -> EvidenceResult<Vec<String>> {
    validate_assertion_readiness(root, manifest, report, false)
}

fn validate_assertion_readiness(
    root: &Path,
    manifest: &AcceptedWorkloadProofs,
    report: &str,
    require_promotion: bool,
) -> EvidenceResult<Vec<String>> {
    for fragment in REQUIRED_ASSERTION_SUMMARY_FRAGMENTS
        .iter()
        .chain(REQUIRED_ASSERTION_ANTI_CLAIM_FRAGMENTS.iter())
    {
        ensure(
            report.contains(fragment),
            format!("assertion-readiness report missing anti-claim fragment: {fragment}"),
        )?;
    }
    let lowered_report = report.to_lowercase();
    for fragment in FORBIDDEN_ASSERTION_OVERCLAIM_FRAGMENTS {
        ensure(
            !lowered_report.contains(fragment),
            format!("assertion-readiness report contains overclaim fragment: {fragment}"),
        )?;
    }

    let expected = manifest
        .proofs
        .iter()
        .map(|proof| assertion_readiness_row(root, proof))
        .collect::<EvidenceResult<Vec<_>>>()?;
    let rows = parse_assertion_readiness_rows(report)?;
    let gaps = parse_assertion_readiness_gaps(report)?;
    let replay_probe_counts = parse_assertion_replay_probe_counts(report)?;

    let expected_workloads = expected
        .iter()
        .map(|row| row.workload.as_str())
        .collect::<BTreeSet<_>>();
    let report_workloads = rows.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let missing = expected_workloads
        .difference(&report_workloads)
        .copied()
        .collect::<Vec<_>>();
    let extra = report_workloads
        .difference(&expected_workloads)
        .copied()
        .collect::<Vec<_>>();
    ensure(
        missing.is_empty(),
        format!(
            "accepted manifest proofs missing from assertion-readiness report: {}",
            missing.join(", ")
        ),
    )?;
    ensure(
        extra.is_empty(),
        format!(
            "assertion-readiness report lists workloads missing from manifest: {}",
            extra.join(", ")
        ),
    )?;

    let identity_statuses = expected
        .iter()
        .map(|summary| {
            let row = rows
                .get(&summary.workload)
                .expect("report workload sets were checked");
            assertion::readiness::WorkloadIdentityStatus {
                workload: &summary.workload,
                artifact_status: &summary.identity_status,
                report_status: &row.identity_status,
            }
        })
        .collect::<Vec<_>>();
    assertion::readiness::require_report_bindings(&identity_statuses)?;

    for summary in &expected {
        let row = rows
            .get(&summary.workload)
            .expect("report workload sets were checked");
        compare_assertion_field(&summary.workload, "cataloged", row.cataloged, summary.total)?;
        compare_assertion_field(
            &summary.workload,
            "exercised",
            row.exercised,
            summary.exercised,
        )?;
        compare_assertion_field(&summary.workload, "always", row.always, summary.always)?;
        compare_assertion_field(
            &summary.workload,
            "sometimes",
            row.sometimes,
            summary.sometimes,
        )?;
        compare_assertion_field(
            &summary.workload,
            "reachability",
            row.reachability,
            summary.reachability,
        )?;
        compare_assertion_field(
            &summary.workload,
            "unreachable",
            row.unreachable,
            summary.unreachable,
        )?;
        compare_assertion_field(
            &summary.workload,
            "uncategorized",
            row.uncategorized,
            summary.uncategorized,
        )?;
        compare_assertion_field(
            &summary.workload,
            "nonpassing",
            row.nonpassing,
            summary.nonpassing,
        )?;
        compare_assertion_field(
            &summary.workload,
            "replay_probe_failures",
            row.replay_probe_failures,
            summary.replay_probe_failures,
        )?;
        ensure(
            row.evidence_path == summary.evidence_path,
            format!(
                "{}: report evidence {} does not match {}",
                summary.workload, row.evidence_path, summary.evidence_path
            ),
        )?;
        let expected_gaps = [
            ("unhit", summary.total - summary.exercised),
            ("uncategorized", summary.uncategorized),
            ("non-passing", summary.nonpassing),
        ];
        for (gap_class, count) in expected_gaps {
            let actual = gaps.get(&(summary.workload.clone(), gap_class.to_string()));
            ensure(
                actual == Some(&count),
                format!(
                    "{}: promotion guidance {} gap {:?}, expected {}",
                    summary.workload, gap_class, actual, count
                ),
            )?;
        }
        let actual_replay_probe_failures = replay_probe_counts.get(&summary.workload);
        ensure(
            actual_replay_probe_failures == Some(&summary.replay_probe_failures),
            format!(
                "{}: replay-probe signal count {:?}, expected {}",
                summary.workload, actual_replay_probe_failures, summary.replay_probe_failures
            ),
        )?;
    }

    if require_promotion {
        assertion::readiness::require_all_accepted(&identity_statuses)?;
    }

    let mut lines = expected
        .iter()
        .map(|summary| {
            format!(
                "{}: identity_status={} cataloged={} exercised={} unhit={} uncategorized={} nonpassing={} replay_probe_failures={}",
                summary.workload,
                summary.identity_status,
                summary.total,
                summary.exercised,
                summary.total - summary.exercised,
                summary.uncategorized,
                summary.nonpassing,
                summary.replay_probe_failures
            )
        })
        .collect::<Vec<_>>();
    lines.sort();
    Ok(lines)
}

pub fn run_assertion_readiness_promotion_selftest(root: impl AsRef<Path>) -> EvidenceResult<()> {
    let root = root.as_ref();
    let error = check_assertion_readiness_promotion(root)
        .expect_err("historical diagnostic artifacts must block promotion");
    ensure(
        error.message().contains("fresh admitted v2 KVM evidence"),
        "assertion-readiness selftest did not reach the v2 identity blocker",
    )?;
    for workload in REQUIRED_WORKLOADS {
        ensure(
            error.message().contains(workload),
            format!("assertion-readiness selftest omitted blocked workload {workload}"),
        )?;
    }
    Ok(())
}

fn compare_assertion_field(
    workload: &str,
    field: &str,
    actual: usize,
    expected: usize,
) -> EvidenceResult<()> {
    ensure(
        actual == expected,
        format!(
            "{workload}: report {field}={actual} does not match assertion artifacts {expected}"
        ),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssertionReadinessParsedRow {
    workload: String,
    identity_status: String,
    cataloged: usize,
    exercised: usize,
    always: usize,
    sometimes: usize,
    reachability: usize,
    unreachable: usize,
    uncategorized: usize,
    nonpassing: usize,
    replay_probe_failures: usize,
    evidence_path: String,
}

fn parse_assertion_readiness_rows(
    report: &str,
) -> EvidenceResult<BTreeMap<String, AssertionReadinessParsedRow>> {
    let mut rows = BTreeMap::new();
    for line in report.lines() {
        if !line.starts_with("| `") {
            continue;
        }
        let cells = markdown_table_cells(line);
        if cells.len() != 9 {
            continue;
        }
        let workload = unbacktick(cells[0]).ok_or_else(|| {
            EvidenceError::new(format!("malformed assertion-readiness row: {line}"))
        })?;
        let identity_status = unbacktick(cells[1]).ok_or_else(|| {
            EvidenceError::new(format!("malformed assertion identity status: {line}"))
        })?;
        let cataloged = parse_backtick_usize(cells[2], line)?;
        let exercised = parse_backtick_usize(cells[3], line)?;
        let kinds = cells[4]
            .split(" / ")
            .map(|part| parse_backtick_usize(part, line))
            .collect::<EvidenceResult<Vec<_>>>()?;
        if kinds.len() != 4 {
            continue;
        }
        let uncategorized = parse_backtick_usize(cells[5], line)?;
        let nonpassing = parse_backtick_usize(cells[6], line)?;
        let replay_probe_failures = parse_backtick_usize(cells[7], line)?;
        let evidence_path = unbacktick(cells[8]).ok_or_else(|| {
            EvidenceError::new(format!(
                "malformed assertion-readiness evidence cell: {line}"
            ))
        })?;
        ensure(
            rows.insert(
                workload.clone(),
                AssertionReadinessParsedRow {
                    workload: workload.clone(),
                    identity_status,
                    cataloged,
                    exercised,
                    always: kinds[0],
                    sometimes: kinds[1],
                    reachability: kinds[2],
                    unreachable: kinds[3],
                    uncategorized,
                    nonpassing,
                    replay_probe_failures,
                    evidence_path,
                },
            )
            .is_none(),
            format!("duplicate assertion-readiness row: {workload}"),
        )?;
    }
    ensure(
        !rows.is_empty(),
        "assertion-readiness report has no accepted proof coverage rows",
    )?;
    Ok(rows)
}

fn parse_assertion_readiness_gaps(
    report: &str,
) -> EvidenceResult<BTreeMap<(String, String), usize>> {
    let mut gaps = BTreeMap::new();
    for line in report.lines() {
        let Some(rest) = line.strip_prefix("- ") else {
            continue;
        };
        let Some((workload, rest)) = rest.split_once(": ") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(count_text) = parts.next() else {
            continue;
        };
        let Some(class) = parts.next() else {
            continue;
        };
        let Some(assertion_text) = parts.next() else {
            continue;
        };
        if parts.next().is_some()
            || !matches!(class, "unhit" | "uncategorized" | "non-passing")
            || assertion_text != "assertion(s)"
        {
            continue;
        }
        let count = count_text.parse::<usize>().map_err(|err| {
            EvidenceError::new(format!(
                "invalid assertion-readiness gap count in {line:?}: {err}"
            ))
        })?;
        let key = (workload.to_string(), class.to_string());
        ensure(
            gaps.insert(key.clone(), count).is_none(),
            format!(
                "duplicate assertion-readiness gap line: {} {}",
                key.0, key.1
            ),
        )?;
    }
    ensure(
        !gaps.is_empty(),
        "assertion-readiness report has no promotion guidance gap lines",
    )?;
    Ok(gaps)
}

fn parse_assertion_replay_probe_counts(report: &str) -> EvidenceResult<BTreeMap<String, usize>> {
    let mut counts = BTreeMap::<String, usize>::new();
    for line in report.lines() {
        let Some(rest) = line.strip_prefix("- ") else {
            continue;
        };
        let Some((workload, rest)) = rest.split_once(": `") else {
            continue;
        };
        if !rest.contains("snapshot replay probe") || !rest.contains("category=replay-probe") {
            continue;
        }
        *counts.entry(workload.to_string()).or_default() += 1;
    }
    ensure(
        !counts.is_empty(),
        "assertion-readiness report has no replay-probe signal lines",
    )?;
    Ok(counts)
}

fn markdown_table_cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

fn parse_backtick_usize(cell: &str, line: &str) -> EvidenceResult<usize> {
    let text = unbacktick(cell).ok_or_else(|| {
        EvidenceError::new(format!(
            "malformed assertion-readiness numeric cell in {line:?}"
        ))
    })?;
    text.parse::<usize>().map_err(|err| {
        EvidenceError::new(format!(
            "invalid assertion-readiness numeric cell {text:?} in {line:?}: {err}"
        ))
    })
}

fn unbacktick(cell: &str) -> Option<String> {
    cell.strip_prefix('`')
        .and_then(|text| text.strip_suffix('`'))
        .map(str::to_string)
}

fn load_assertion_admission(
    root: &Path,
    proof: &AcceptedWorkloadProof,
) -> EvidenceResult<assertion::readiness::IdentityAdmission> {
    let path = root.join(&proof.evidence_dir).join("assertions.json");
    let value: serde_json::Value = load_json(root, &path)?;
    assertion::readiness::classify(&value).map_err(|error| {
        EvidenceError::new(format!("{}: {}", rel_display(root, &path), error.message()))
    })
}

fn assertion_readiness_row(
    root: &Path,
    proof: &AcceptedWorkloadProof,
) -> EvidenceResult<AssertionReadinessRow> {
    let evidence_path = format!("{}/assertions.json", proof.evidence_dir);
    let admission = load_assertion_admission(root, proof)?;
    let identity_status = admission.status.as_str().to_string();
    let identity_blocker = admission.promotion_blocker.clone();
    let assertions = admission.entries;
    let local_support = local_assertion_support(root, &proof.workload)?;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut uncategorized = 0_usize;
    let mut unhit = Vec::new();
    let mut nonpassing = Vec::new();
    let mut replay_probe_signals = Vec::new();
    let mut gap_details = Vec::new();
    let mut local_fixture_covered = Vec::new();

    for item in assertions {
        let kind = item.kind_string();
        let kind = assertion_kind_label(kind.as_deref()).to_string();
        *counts.entry(kind.clone()).or_default() += 1;
        let label = item.message_or_id();
        let id = item.id_u64();
        let category =
            effective_assertion_category(&proof.workload, &label, item.category_string());
        if category.name == "uncategorized" {
            uncategorized += 1;
        }
        let artifact_verdict = item
            .verdict_string()
            .unwrap_or_else(|| "unknown".to_string());
        let artifact_hit_count = item.hit_count_i64().unwrap_or(0);
        let local_evidence = (artifact_hit_count == 0)
            .then(|| local_support.evidence_for(id, &label))
            .flatten();
        let locally_covered = local_evidence.is_some();
        let hit_count = if locally_covered {
            1
        } else {
            artifact_hit_count
        };
        let verdict = if locally_covered {
            "passed".to_string()
        } else {
            artifact_verdict.clone()
        };
        if let Some(evidence) = local_evidence {
            local_fixture_covered.push(format!(
                "`{label}` covered by local deterministic harness `{evidence}` (historical verdict={artifact_verdict}, hit_count={artifact_hit_count})"
            ));
        }
        if hit_count == 0 {
            unhit.push(label.clone());
            gap_details.push(AssertionGapDetail {
                workload: proof.workload.clone(),
                gap_class: AssertionGapClass::Unhit,
                label: label.clone(),
                kind: kind.clone(),
                category: category.name.clone(),
                category_inferred: category.inferred,
                verdict: verdict.clone(),
                hit_count,
            });
        }
        if verdict != "passed" {
            let is_replay_probe = category.name == "replay-probe";
            if is_replay_probe {
                let rendered_category = if category.inferred {
                    format!("{} (inferred)", category.name)
                } else {
                    category.name.clone()
                };
                replay_probe_signals.push(format!(
                    "{}: `{}` (kind={}, category={}, verdict={}, hit_count={})",
                    proof.workload, label, kind, rendered_category, verdict, hit_count
                ));
            } else {
                nonpassing.push(label.clone());
                gap_details.push(AssertionGapDetail {
                    workload: proof.workload.clone(),
                    gap_class: AssertionGapClass::NonPassing,
                    label,
                    kind,
                    category: category.name,
                    category_inferred: category.inferred,
                    verdict,
                    hit_count,
                });
            }
        }
    }

    let total = counts.values().sum();
    let exercised = total - unhit.len();
    Ok(AssertionReadinessRow {
        workload: proof.workload.clone(),
        identity_status,
        identity_blocker,
        total,
        exercised,
        always: *counts.get("always").unwrap_or(&0),
        sometimes: *counts.get("sometimes").unwrap_or(&0),
        reachability: *counts.get("reachability").unwrap_or(&0),
        unreachable: *counts.get("unreachable").unwrap_or(&0),
        uncategorized,
        nonpassing: nonpassing.len(),
        replay_probe_failures: replay_probe_signals.len(),
        evidence_path,
        gaps: vec![
            format!("{}: {} unhit assertion(s)", proof.workload, unhit.len()),
            format!(
                "{}: {} uncategorized assertion(s)",
                proof.workload, uncategorized
            ),
            format!(
                "{}: {} non-passing assertion(s)",
                proof.workload,
                nonpassing.len()
            ),
        ],
        replay_probe_signals,
        gap_details,
        local_fixture_covered,
    })
}

#[derive(Debug, Default, Clone)]
struct LocalAssertionSupport {
    by_id: BTreeMap<u64, String>,
    by_message: BTreeMap<String, String>,
}

impl LocalAssertionSupport {
    fn evidence_for(&self, id: Option<u64>, message: &str) -> Option<String> {
        id.and_then(|id| self.by_id.get(&id).cloned())
            .or_else(|| self.by_message.get(message).cloned())
    }
}

fn local_assertion_support(root: &Path, workload: &str) -> EvidenceResult<LocalAssertionSupport> {
    let path = root.join(LOCAL_ASSERTION_HARNESSES_PATH);
    if !path.exists() {
        return Ok(LocalAssertionSupport::default());
    }
    let manifest: LocalAssertionHarnessManifest = load_json(root, &path)?;
    ensure(
        manifest.schema_version == 1,
        format!(
            "{}: unsupported local assertion harness schema_version {}",
            rel_display(root, &path),
            manifest.schema_version
        ),
    )?;
    let mut support = LocalAssertionSupport::default();
    for harness in manifest
        .harnesses
        .into_iter()
        .filter(|h| h.workload == workload)
    {
        ensure(
            !harness.evidence.is_empty(),
            format!("{workload}: local assertion harness evidence must be non-empty"),
        )?;
        for assertion in harness.covered_assertions {
            ensure(
                assertion.status == "passed",
                format!(
                    "{workload}: local assertion harness {} has non-passing status {}",
                    assertion.message, assertion.status
                ),
            )?;
            if let Some(id) = assertion.id {
                support.by_id.insert(id, harness.evidence.clone());
            }
            support
                .by_message
                .insert(assertion.message, harness.evidence.clone());
        }
    }
    Ok(support)
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct LocalAssertionHarnessManifest {
    schema_version: u64,
    harnesses: Vec<LocalAssertionHarness>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct LocalAssertionHarness {
    workload: String,
    evidence: String,
    covered_assertions: Vec<LocalAssertionHarnessAssertion>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct LocalAssertionHarnessAssertion {
    id: Option<u64>,
    message: String,
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EffectiveAssertionCategory {
    name: String,
    inferred: bool,
}

fn effective_assertion_category(
    workload: &str,
    message: &str,
    artifact_category: Option<String>,
) -> EffectiveAssertionCategory {
    if let Some(category) = artifact_category.filter(|category| category != "uncategorized") {
        return EffectiveAssertionCategory {
            name: category,
            inferred: false,
        };
    }

    let inferred = infer_assertion_category(workload, message);
    EffectiveAssertionCategory {
        name: inferred.unwrap_or("uncategorized").to_string(),
        inferred: inferred.is_some(),
    }
}

fn infer_assertion_category(workload: &str, message: &str) -> Option<&'static str> {
    if message.contains("snapshot replay probe") {
        return Some("replay-probe");
    }

    match workload {
        "redb" => infer_redb_assertion_category(message),
        "raft" => infer_raft_assertion_category(message),
        "net" => Some("network"),
        "rust-workload" => infer_rust_workload_assertion_category(message),
        _ => None,
    }
}

fn infer_redb_assertion_category(message: &str) -> Option<&'static str> {
    if message.starts_with("op: ") || matches!(message, "commit succeeds" | "large batch committed")
    {
        Some("operation")
    } else {
        Some("invariant")
    }
}

fn infer_raft_assertion_category(message: &str) -> Option<&'static str> {
    if matches!(
        message,
        "message reordered"
            | "message duplicated"
            | "message delivered"
            | "message dropped"
            | "partition healed"
            | "link partitioned"
    ) {
        Some("network")
    } else if matches!(message, "node crashed" | "node restarted") {
        Some("fault")
    } else if message.contains("election")
        || message.contains("vote")
        || message.contains("candidate")
        || message.contains("leader elected")
        || message.contains("leader skipped")
        || message.contains("stepped down")
        || message.contains("term")
        || message.contains("timer")
        || message.contains("quorum")
    {
        Some("election")
    } else if message.contains("log")
        || message.contains("append")
        || message.contains("commit")
        || message.contains("entry")
        || message.contains("entries")
        || message.contains("index")
        || message.contains("proposal")
        || message.contains("value")
    {
        Some("replication")
    } else {
        None
    }
}

fn infer_rust_workload_assertion_category(message: &str) -> Option<&'static str> {
    match message {
        "read branch exercised" => Some("branch"),
        "write succeeds" | "at least one write succeeds" => Some("operation"),
        "choice remains in range" | "operation counters stay bounded" => Some("workload-driver"),
        _ => None,
    }
}

fn assertion_kind_label(kind: Option<&str>) -> &str {
    match kind.unwrap_or("unknown") {
        "always" => "always",
        "sometimes" => "sometimes",
        "reachable" | "reachability" => "reachability",
        "unreachable" => "unreachable",
        other => other,
    }
}

fn coverage_workload_label(workload: &str) -> String {
    match workload {
        "raft" => "Raft".to_string(),
        other => other.to_string(),
    }
}

struct ReplayArtifactReview {
    line: ReplayProofCoverageLine,
    bug: BugRecord,
    bug_value: serde_json::Value,
    verdict: ReplayVerdict,
    verdict_value: serde_json::Value,
}

fn validate_workload_proof(
    root: &Path,
    proof: &AcceptedWorkloadProof,
) -> EvidenceResult<ReplayProofCoverageLine> {
    let review = validate_workload_replay_artifacts(root, proof)?;
    validate_workload_assertion_identity(root, proof, &review)?;
    Ok(review.line)
}

fn review_workload_proof(
    root: &Path,
    proof: &AcceptedWorkloadProof,
) -> EvidenceResult<ReplayProofCoverageLine> {
    let mut review = validate_workload_replay_artifacts(root, proof)?;
    if validate_workload_assertion_identity(root, proof, &review).is_err() {
        review.line.replay_class = BLOCKED_ASSERTION_IDENTITY_STATUS.to_string();
    }
    Ok(review.line)
}

fn validate_workload_assertion_identity(
    root: &Path,
    proof: &AcceptedWorkloadProof,
    review: &ReplayArtifactReview,
) -> EvidenceResult<()> {
    validate_bug_report_for_replay(&review.bug_value)?;
    validate_replay_verdict_with_options(&review.verdict_value, true, true, root)?;
    let bug_identity = review.bug.require_assertion_identity()?;
    let verdict_identity = review.verdict.require_assertion_identity()?;
    review.verdict.validate_shape()?;
    ensure(
        bug_identity == verdict_identity,
        format!(
            "{}: bug and replay verdict assertion identities differ",
            proof.workload
        ),
    )?;
    load_assertion_admission(root, proof)?.require_evidence_identity(
        &proof.workload,
        proof.assertion_id,
        bug_identity,
    )
}

fn validate_workload_replay_artifacts(
    root: &Path,
    proof: &AcceptedWorkloadProof,
) -> EvidenceResult<ReplayArtifactReview> {
    let evidence_dir = root.join(&proof.evidence_dir);
    let summary_path = evidence_dir.join(&proof.summary);
    let bug_path = evidence_dir.join(&proof.bug);
    let verdict_path = evidence_dir.join(&proof.verdict);
    let snapshot_path = evidence_dir.join(&proof.snapshot);

    let summary: AcceptedVerdictSummary = load_json(root, &summary_path)?;
    let bug_value: serde_json::Value = load_json(root, &bug_path)?;
    validate_bug_report(&bug_value)?;
    let bug: BugRecord = serde_json::from_value(bug_value.clone()).map_err(|error| {
        EvidenceError::new(format!("{}: invalid bug report: {error}", proof.workload))
    })?;
    let verdict_value: serde_json::Value = load_json(root, &verdict_path)?;
    validate_replay_verdict_with_options(&verdict_value, false, false, root)?;
    let verdict: ReplayVerdict =
        serde_json::from_value(verdict_value.clone()).map_err(|error| {
            EvidenceError::new(format!(
                "{}: invalid replay verdict: {error}",
                proof.workload
            ))
        })?;

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

    ensure(
        verdict.replay_class == REQUIRED_REPLAY_CLASS
            && verdict.reproduced
            && verdict.command.exit_status == 0
            && verdict.replay_parent_depth > 0,
        format!(
            "{}: historical replay verdict facts are invalid",
            proof.workload
        ),
    )?;
    verdict.snapshot.validate_shape()?;
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

    Ok(ReplayArtifactReview {
        line: ReplayProofCoverageLine {
            workload: proof.workload.clone(),
            replay_class: REQUIRED_REPLAY_CLASS.to_string(),
            assertion_id: proof.assertion_id,
            replay_parent_depth: verdict.replay_parent_depth,
            snapshot_digest: expected_digest,
            snapshot_storage: storage,
        },
        bug,
        bug_value,
        verdict,
        verdict_value,
    })
}

fn load_json<T>(root: &Path, path: &Path) -> EvidenceResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let input = bounded_file::read_bounded_regular_file(path, MAX_EVIDENCE_JSON_BYTES)?;
    json_preflight::preflight_json(&input, json_preflight::QUALITY_REPORT_LIMITS)?;
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

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn copy_file_into(path: &Path, writer: &mut impl Write, hasher: &mut Sha256) -> EvidenceResult<()> {
    let mut file =
        File::open(path).map_err(|err| EvidenceError::new(format!("{}: {err}", path.display())))?;
    let mut buffer = [0_u8; SNAPSHOT_COPY_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read])?;
    }
    Ok(())
}

fn hash_file_into(path: &Path, hasher: &mut Sha256) -> EvidenceResult<()> {
    let mut file =
        File::open(path).map_err(|err| EvidenceError::new(format!("{}: {err}", path.display())))?;
    let mut buffer = [0_u8; SNAPSHOT_COPY_BUFFER_BYTES];
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
        let path = path.as_ref();
        let input = bounded_file::read_bounded_regular_file(path, MAX_EVIDENCE_JSON_BYTES)?;
        json_preflight::preflight_json(&input, json_preflight::QUALITY_REPORT_LIMITS)?;
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
    #[serde(
        default = "no_evidence_identity",
        deserialize_with = "non_null_option::deserialize"
    )]
    pub assertion_identity: Option<chaoscontrol_protocol::admission::AssertionEvidenceIdentity>,
    pub assertion_location: Option<String>,
    pub tick: Option<u64>,
    pub replay_parent_depth: u64,
    pub replay_parent_snapshot_ref: Option<SnapshotRef>,
    pub dedup_key: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
    #[serde(
        default = "no_evidence_identity",
        deserialize_with = "non_null_option::deserialize"
    )]
    pub assertion_identity: Option<chaoscontrol_protocol::admission::AssertionEvidenceIdentity>,
    pub replay_parent_depth: u64,
    pub snapshot: SnapshotVerdict,
    pub artifact_hashes: Vec<ArtifactHash>,
}

fn no_evidence_identity() -> Option<chaoscontrol_protocol::admission::AssertionEvidenceIdentity> {
    None
}

fn require_evidence_identity<'a>(
    assertion_id: u64,
    identity: Option<&'a chaoscontrol_protocol::admission::AssertionEvidenceIdentity>,
    carrier: &str,
) -> EvidenceResult<&'a chaoscontrol_protocol::admission::AssertionEvidenceIdentity> {
    let identity = identity.ok_or_else(|| {
        EvidenceError::new(format!(
            "{carrier}: legacy assertion ID-only evidence cannot promote"
        ))
    })?;
    identity
        .validate_compatibility_alias(assertion_id)
        .map_err(|error| {
            EvidenceError::new(format!("{carrier}: invalid assertion identity: {error:?}"))
        })?;
    Ok(identity)
}

impl BugRecord {
    fn require_assertion_identity(
        &self,
    ) -> EvidenceResult<&chaoscontrol_protocol::admission::AssertionEvidenceIdentity> {
        require_evidence_identity(
            self.assertion_id,
            self.assertion_identity.as_ref(),
            "bug-report",
        )
    }
}

impl ReplayVerdict {
    fn require_assertion_identity(
        &self,
    ) -> EvidenceResult<&chaoscontrol_protocol::admission::AssertionEvidenceIdentity> {
        require_evidence_identity(
            self.assertion_id,
            self.assertion_identity.as_ref(),
            "replay-verdict",
        )
    }

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
        self.require_assertion_identity()?;
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
        self.reference.validate_current_shape()
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

    fn validate_current_shape(&self) -> EvidenceResult<()> {
        self.validate_shape()?;
        ensure(
            self.codec == CURRENT_SNAPSHOT_CODEC
                && self.schema_version == CURRENT_SNAPSHOT_SCHEMA_VERSION,
            "accepted snapshot evidence requires the current CBOR v2 codec",
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArtifactHash {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AssertionSummaryEntry {
    pub id: Option<serde_json::Value>,
    pub message: Option<serde_json::Value>,
    pub kind: Option<serde_json::Value>,
    pub category: Option<serde_json::Value>,
    pub hit_count: Option<serde_json::Value>,
    pub verdict: Option<serde_json::Value>,
}

impl AssertionSummaryEntry {
    fn value_string(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Null => "null".to_string(),
            other => other.to_string(),
        }
    }

    fn message_or_id(&self) -> String {
        self.message
            .as_ref()
            .or(self.id.as_ref())
            .map(Self::value_string)
            .unwrap_or_else(|| "<unnamed>".to_string())
    }

    fn id_u64(&self) -> Option<u64> {
        match self.id.as_ref()? {
            serde_json::Value::Number(number) => number.as_u64(),
            serde_json::Value::String(text) => text.parse().ok(),
            serde_json::Value::Bool(value) => Some(u64::from(*value)),
            serde_json::Value::Null => None,
            _ => None,
        }
    }

    fn kind_string(&self) -> Option<String> {
        self.kind.as_ref().map(Self::value_string)
    }

    fn category_string(&self) -> Option<String> {
        self.category.as_ref().map(Self::value_string)
    }

    fn verdict_string(&self) -> Option<String> {
        self.verdict.as_ref().map(Self::value_string)
    }

    fn hit_count_i64(&self) -> Option<i64> {
        match self.hit_count.as_ref()? {
            serde_json::Value::Number(number) => number.as_i64(),
            serde_json::Value::String(text) => text.parse().ok(),
            serde_json::Value::Bool(value) => Some(i64::from(*value)),
            serde_json::Value::Null => None,
            _ => None,
        }
    }
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
