//! Exact persisted sessions. JSON metadata alone never admits restored execution.
//!
//! The pure validators bind configuration, recording, checkpoint and guest input
//! identities. The shell pins no-follow directory/file capabilities and reuses
//! the existing bounded exploration snapshot store's CBOR/zstd representation.

use crate::checkpoint::{Checkpoint, CheckpointStore};
use crate::event_query::TimelineFact;
use crate::recording::{validate_recording, Recorder, Recording};
use chaoscontrol_explore::snapshot_store::{
    decode_snapshot_artifact_with_limit, encode_snapshot_artifact_with_limits,
    ReplayParentSnapshotRef,
};
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::vm::VmConfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

pub const SESSION_SCHEMA_VERSION: u32 = 1;
pub const MAX_SESSION_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_SESSION_RECORDING_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_SESSION_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_SESSION_SNAPSHOT_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_SESSION_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_SESSION_CHECKPOINTS: usize = 128;
pub const MAX_SESSION_BRANCHES: usize = 128;
pub const MAX_SESSION_DECODED_SNAPSHOT_BYTES: u64 =
    chaoscontrol_explore::snapshot_store::MAX_DECOMPRESSED_SNAPSHOT_BYTES;
const MAX_SESSION_PATH_BYTES: usize = 4096;
const MANIFEST_NAME: &str = "session.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionLimits {
    pub max_manifest_bytes: u64,
    pub max_recording_bytes: u64,
    pub max_artifact_bytes: u64,
    pub max_snapshot_bytes: u64,
    pub max_total_bytes: u64,
    pub max_checkpoints: usize,
    pub max_branches: usize,
}
impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            max_manifest_bytes: MAX_SESSION_MANIFEST_BYTES,
            max_recording_bytes: MAX_SESSION_RECORDING_BYTES,
            max_artifact_bytes: MAX_SESSION_ARTIFACT_BYTES,
            max_snapshot_bytes: MAX_SESSION_SNAPSHOT_BYTES,
            max_total_bytes: MAX_SESSION_TOTAL_BYTES,
            max_checkpoints: MAX_SESSION_CHECKPOINTS,
            max_branches: MAX_SESSION_BRANCHES,
        }
    }
}
impl SessionLimits {
    pub fn validate(self) -> Result<(), SessionError> {
        let hard = Self::default();
        if self.max_manifest_bytes == 0
            || self.max_manifest_bytes > hard.max_manifest_bytes
            || self.max_recording_bytes == 0
            || self.max_recording_bytes > hard.max_recording_bytes
            || self.max_artifact_bytes == 0
            || self.max_artifact_bytes > hard.max_artifact_bytes
            || self.max_snapshot_bytes == 0
            || self.max_snapshot_bytes > hard.max_snapshot_bytes
            || self.max_total_bytes == 0
            || self.max_total_bytes > hard.max_total_bytes
            || self.max_checkpoints == 0
            || self.max_checkpoints > hard.max_checkpoints
            || self.max_branches == 0
            || self.max_branches > hard.max_branches
        {
            return Err(SessionError::Bound("session limits"));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum SessionError {
    MissingSession,
    MissingArtifact(String),
    MissingCheckpoint(u64),
    Bound(&'static str),
    Incompatible(String),
    Corrupt(String),
    Drift(String),
    Path(String),
    Io(std::io::Error),
}
impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSession => write!(f, "exact persisted session is missing"),
            Self::MissingArtifact(path) => write!(f, "session artifact is missing: {path}"),
            Self::MissingCheckpoint(id) => write!(f, "exact checkpoint {id} is missing"),
            Self::Bound(bound) => write!(f, "session exceeds bound: {bound}"),
            Self::Incompatible(reason) => write!(f, "incompatible session: {reason}"),
            Self::Corrupt(reason) => write!(f, "corrupt session: {reason}"),
            Self::Drift(path) => write!(f, "session artifact identity drift: {path}"),
            Self::Path(path) => write!(
                f,
                "session path is not a no-follow regular capability: {path}"
            ),
            Self::Io(error) => write!(f, "session I/O: {error}"),
        }
    }
}
impl std::error::Error for SessionError {}
impl From<std::io::Error> for SessionError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionArtifact {
    pub path: String,
    pub digest: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestArtifactRole {
    Kernel,
    Initrd,
    Disk,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestArtifact {
    pub role: GuestArtifactRole,
    pub artifact: SessionArtifact,
}

/// All controller configuration fields are retained. The schedule itself is
/// owned by the recording and its exact identity is checked, not regenerated.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionConfiguration {
    pub num_vms: usize,
    pub vm_config: VmConfig,
    pub kernel_path: String,
    pub initrd_path: Option<String>,
    pub disk_image_path: Option<String>,
    pub seed: u64,
    pub quantum: u64,
    pub bootstrap_budget: Option<u64>,
    pub base_core: Option<usize>,
    pub dlog_dir: Option<PathBuf>,
    pub random_faults: bool,
    pub schedule_identity: String,
}
impl SessionConfiguration {
    pub fn capture(config: &SimulationConfig) -> Result<Self, SessionError> {
        if config.schedule.snapshot().cursor() != 0 {
            return Err(SessionError::Incompatible(
                "configuration schedule cursor is not initial".into(),
            ));
        }
        Ok(Self {
            num_vms: config.num_vms,
            vm_config: config.vm_config.clone(),
            kernel_path: config.kernel_path.clone(),
            initrd_path: config.initrd_path.clone(),
            disk_image_path: config.disk_image_path.clone(),
            seed: config.seed,
            quantum: config.quantum,
            bootstrap_budget: config.bootstrap_budget,
            base_core: config.base_core,
            dlog_dir: config.dlog_dir.clone(),
            random_faults: config.random_faults,
            schedule_identity: digest_json(config.schedule.faults())?,
        })
    }
    pub fn simulation_config(
        &self,
        schedule: chaoscontrol_fault::schedule::FaultSchedule,
    ) -> Result<SimulationConfig, SessionError> {
        if digest_json(schedule.faults())? != self.schedule_identity
            || schedule.snapshot().cursor() != 0
        {
            return Err(SessionError::Incompatible(
                "configuration schedule identity".into(),
            ));
        }
        Ok(SimulationConfig {
            num_vms: self.num_vms,
            vm_config: self.vm_config.clone(),
            kernel_path: self.kernel_path.clone(),
            initrd_path: self.initrd_path.clone(),
            disk_image_path: self.disk_image_path.clone(),
            seed: self.seed,
            quantum: self.quantum,
            bootstrap_budget: self.bootstrap_budget,
            base_core: self.base_core,
            dlog_dir: self.dlog_dir.clone(),
            random_faults: self.random_faults,
            schedule,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCheckpoint {
    pub id: u64,
    pub tick: u64,
    pub artifact: SessionArtifact,
    pub snapshot_ref: ReplayParentSnapshotRef,
    pub serial_output: Vec<String>,
    pub events_since_last: Vec<crate::recording::RecordedEvent>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionBranch {
    pub branch_id: String,
    pub parent: Option<String>,
    pub checkpoint_id: u64,
    pub tick: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionManifest {
    pub schema_version: u32,
    pub session_id: String,
    pub config_identity: String,
    pub configuration: SessionConfiguration,
    pub recording: SessionArtifact,
    pub guest_artifacts: Vec<GuestArtifact>,
    pub checkpoints: Vec<SessionCheckpoint>,
    pub branches: Vec<SessionBranch>,
    pub retained_start_tick: u64,
    pub retained_end_tick: u64,
    pub limits: SessionLimits,
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    format!("b3:{}", blake3::hash(bytes).to_hex())
}
pub fn valid_identity(value: &str) -> bool {
    value.strip_prefix("b3:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
fn digest_json<T: Serialize + ?Sized>(value: &T) -> Result<String, SessionError> {
    let mut output = IdentityWriter {
        hasher: blake3::Hasher::new(),
        bytes: 0,
        exceeded: false,
    };
    serde_json::to_writer(&mut output, value).map_err(|error| {
        if output.exceeded {
            SessionError::Bound("identity serialization bytes")
        } else {
            SessionError::Corrupt(error.to_string())
        }
    })?;
    Ok(format!("b3:{}", output.hasher.finalize().to_hex()))
}
fn manifest_identity(manifest: &SessionManifest) -> Result<String, SessionError> {
    // A versioned ordered projection omits only its own identity. Borrow all
    // payloads rather than cloning checkpoint metadata just to clear one field.
    digest_json(&(
        "chaoscontrol.exact-session.v1",
        manifest.schema_version,
        &manifest.config_identity,
        &manifest.configuration,
        &manifest.recording,
        &manifest.guest_artifacts,
        &manifest.checkpoints,
        &manifest.branches,
        manifest.retained_start_tick,
        manifest.retained_end_tick,
        manifest.limits,
    ))
}

struct IdentityWriter {
    hasher: blake3::Hasher,
    bytes: u64,
    exceeded: bool,
}
impl Write for IdentityWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let next = self.bytes.saturating_add(bytes.len() as u64);
        if next > MAX_SESSION_MANIFEST_BYTES {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "identity serialization byte bound exceeded",
            ));
        }
        self.hasher.update(bytes);
        self.bytes = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Pure manifest admission. No path is opened and no VM is constructed.
// r[impl chaoscontrol.debug_query.moment]
// r[impl chaoscontrol.debug_query.evidence]
pub fn validate_manifest(
    manifest: &SessionManifest,
    limits: SessionLimits,
) -> Result<(), SessionError> {
    limits.validate()?;
    manifest.limits.validate()?;
    if manifest.schema_version != SESSION_SCHEMA_VERSION {
        return Err(SessionError::Incompatible("session schema".into()));
    }
    if !valid_identity(&manifest.session_id)
        || manifest_identity(manifest)? != manifest.session_id
        || digest_json(&manifest.configuration)? != manifest.config_identity
    {
        return Err(SessionError::Corrupt("manifest/config identity".into()));
    }
    if manifest.checkpoints.is_empty() {
        return Err(SessionError::MissingCheckpoint(0));
    }
    if manifest.checkpoints.len() > limits.max_checkpoints
        || manifest.checkpoints.len() > manifest.limits.max_checkpoints
    {
        return Err(SessionError::Bound("checkpoint count"));
    }
    if manifest.branches.is_empty()
        || manifest.branches.len() > limits.max_branches
        || manifest.branches.len() > manifest.limits.max_branches
    {
        return Err(SessionError::Bound("branch count"));
    }
    if manifest.retained_start_tick > manifest.retained_end_tick {
        return Err(SessionError::Corrupt("retained horizon".into()));
    }
    let mut paths = BTreeMap::<String, (String, u64)>::new();
    let mut total = 0u64;
    let mut admit_artifact = |artifact: &SessionArtifact, bound: u64| -> Result<(), SessionError> {
        validate_relative_path(&artifact.path)?;
        if !valid_identity(&artifact.digest) {
            return Err(SessionError::Corrupt("artifact digest".into()));
        }
        if artifact.bytes > bound {
            return Err(SessionError::Bound("artifact bytes"));
        }
        if let Some(prior) = paths.insert(
            artifact.path.clone(),
            (artifact.digest.clone(), artifact.bytes),
        ) {
            if prior != (artifact.digest.clone(), artifact.bytes) {
                return Err(SessionError::Corrupt("duplicate artifact path".into()));
            }
        } else {
            total = total
                .checked_add(artifact.bytes)
                .ok_or(SessionError::Bound("total bytes"))?;
        }
        Ok(())
    };
    if manifest.recording.path != "recording.json" {
        return Err(SessionError::Path(manifest.recording.path.clone()));
    }
    admit_artifact(
        &manifest.recording,
        limits
            .max_recording_bytes
            .min(manifest.limits.max_recording_bytes),
    )?;
    let config = &manifest.configuration;
    if config.num_vms == 0
        || config.vm_config.num_vcpus == 0
        || config.vm_config.memory_size == 0
        || config.quantum == 0
        || config.kernel_path.is_empty()
    {
        return Err(SessionError::Incompatible(
            "invalid controller configuration".into(),
        ));
    }
    let retained_memory = (config.vm_config.memory_size as u64)
        .checked_mul(config.num_vms as u64)
        .and_then(|bytes| bytes.checked_mul(manifest.checkpoints.len() as u64))
        .ok_or(SessionError::Bound("retained snapshot memory"))?;
    if retained_memory > limits.max_total_bytes || retained_memory > manifest.limits.max_total_bytes
    {
        return Err(SessionError::Bound("retained snapshot memory"));
    }
    let expected_roles = [
        Some(GuestArtifactRole::Kernel),
        config
            .initrd_path
            .as_ref()
            .map(|_| GuestArtifactRole::Initrd),
        config
            .disk_image_path
            .as_ref()
            .map(|_| GuestArtifactRole::Disk),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if manifest
        .guest_artifacts
        .iter()
        .map(|entry| entry.role.clone())
        .collect::<Vec<_>>()
        != expected_roles
    {
        return Err(SessionError::Corrupt("guest artifact inventory".into()));
    }
    for guest in &manifest.guest_artifacts {
        if !guest.artifact.path.starts_with("artifacts/") {
            return Err(SessionError::Path(guest.artifact.path.clone()));
        }
        admit_artifact(
            &guest.artifact,
            limits
                .max_artifact_bytes
                .min(manifest.limits.max_artifact_bytes),
        )?;
    }
    let mut checkpoints = BTreeMap::new();
    let mut prior_tick = None;
    for checkpoint in &manifest.checkpoints {
        if checkpoints.insert(checkpoint.id, checkpoint).is_some()
            || prior_tick.is_some_and(|tick| checkpoint.tick < tick)
            || checkpoint.tick < manifest.retained_start_tick
            || checkpoint.tick > manifest.retained_end_tick
        {
            return Err(SessionError::Corrupt("checkpoint order/horizon".into()));
        }
        prior_tick = Some(checkpoint.tick);
        if checkpoint.artifact.path != checkpoint.snapshot_ref.path
            || !checkpoint.artifact.path.starts_with("snapshots/")
        {
            return Err(SessionError::Corrupt("checkpoint path".into()));
        }
        chaoscontrol_explore::snapshot_store::validate_ref_shape(&checkpoint.snapshot_ref)
            .map_err(|error| SessionError::Incompatible(error.to_string()))?;
        admit_artifact(
            &checkpoint.artifact,
            limits
                .max_snapshot_bytes
                .min(manifest.limits.max_snapshot_bytes),
        )?;
    }
    let manifest_bytes = bounded_json(
        manifest,
        limits
            .max_manifest_bytes
            .min(manifest.limits.max_manifest_bytes),
        "manifest bytes",
    )?
    .len() as u64;
    total = total
        .checked_add(manifest_bytes)
        .ok_or(SessionError::Bound("total bytes"))?;
    if total > limits.max_total_bytes || total > manifest.limits.max_total_bytes {
        return Err(SessionError::Bound("total bytes"));
    }
    let mut branches = BTreeMap::new();
    let mut roots = 0;
    for branch in &manifest.branches {
        if !valid_identity(&branch.branch_id)
            || branches.insert(branch.branch_id.as_str(), branch).is_some()
        {
            return Err(SessionError::Corrupt("branch identity".into()));
        }
        let checkpoint = checkpoints
            .get(&branch.checkpoint_id)
            .ok_or(SessionError::MissingCheckpoint(branch.checkpoint_id))?;
        if checkpoint.tick != branch.tick {
            return Err(SessionError::Corrupt("branch checkpoint tick".into()));
        }
        if branch.parent.is_none() {
            roots += 1;
        }
    }
    if roots != 1 {
        return Err(SessionError::Corrupt("branch root inventory".into()));
    }
    for branch in &manifest.branches {
        let mut seen = BTreeSet::new();
        let mut cursor = branch;
        while let Some(parent_id) = &cursor.parent {
            if !seen.insert(cursor.branch_id.as_str()) {
                return Err(SessionError::Corrupt("branch cycle".into()));
            }
            let parent = branches
                .get(parent_id.as_str())
                .ok_or_else(|| SessionError::Corrupt("missing branch parent".into()))?;
            if parent.tick > cursor.tick {
                return Err(SessionError::Corrupt("branch parent horizon".into()));
            }
            cursor = parent;
        }
    }
    Ok(())
}

fn validate_configuration(
    recording: &Recording,
    config: &SessionConfiguration,
) -> Result<(), SessionError> {
    let recorded = &recording.config;
    if recorded.num_vms != config.num_vms
        || recorded.vm_memory_size != config.vm_config.memory_size
        || recorded.tsc_khz != config.vm_config.cpu.tsc_khz
        || recorded.kernel_path != config.kernel_path
        || recorded.initrd_path != config.initrd_path
        || recorded.disk_image_path != config.disk_image_path
        || recorded.quantum != config.quantum
        || recording.seed != config.seed
        || recorded.random_faults != config.random_faults
        || digest_json(recording.schedule.faults())? != config.schedule_identity
    {
        return Err(SessionError::Incompatible(
            "recording/controller configuration mismatch".into(),
        ));
    }
    Ok(())
}
fn validate_checkpoint(
    recording: &Recording,
    checkpoint: &Checkpoint,
    config: &SessionConfiguration,
) -> Result<(), SessionError> {
    let snapshot = crate::replay::validated_checkpoint_snapshot(recording, checkpoint)
        .map_err(|error| SessionError::Corrupt(error.to_string()))?;
    snapshot
        .validate_assertion_identity(config.num_vms)
        .map_err(SessionError::Incompatible)?;
    SimulationController::validate_pending_snapshot(snapshot)
        .map_err(|error| SessionError::Incompatible(error.to_string()))?;
    for (vm, _) in &snapshot.vm_snapshots {
        vm.validate_kernel_restore(config.vm_config.num_vcpus)
            .map_err(|error| SessionError::Incompatible(error.to_string()))?;
        vm.memory
            .validate_for_guest_size(config.vm_config.memory_size)
            .map_err(SessionError::Incompatible)?;
    }
    Ok(())
}

/// An admitted exact session owns a pinned directory and loaded exact snapshots.
/// Fields are private: metadata deserialization cannot construct this capability.
pub struct ExactSession {
    root: File,
    manifest: SessionManifest,
    recording: Recording,
    limits: SessionLimits,
}
impl ExactSession {
    pub fn manifest(&self) -> &SessionManifest {
        &self.manifest
    }
    pub fn recording(&self) -> &Recording {
        &self.recording
    }
    pub fn recording_identity(&self) -> &str {
        &self.manifest.recording.digest
    }
    pub fn checkpoint_identity(&self, id: u64) -> Result<&str, SessionError> {
        self.manifest
            .checkpoints
            .iter()
            .find(|checkpoint| checkpoint.id == id)
            .map(|checkpoint| checkpoint.artifact.digest.as_str())
            .ok_or(SessionError::MissingCheckpoint(id))
    }
    /// Revalidate retained artifacts before any KVM allocation, then restore the
    /// exact snapshot. No VM round, boot fallback or state inference is used.
    pub fn restore_controller(
        &self,
        checkpoint_id: u64,
    ) -> Result<RestoredController, SessionError> {
        self.revalidate_retained()?;
        let checkpoint = self
            .recording
            .checkpoints
            .get(checkpoint_id)
            .ok_or(SessionError::MissingCheckpoint(checkpoint_id))?;
        let snapshot = checkpoint
            .snapshot
            .as_ref()
            .ok_or(SessionError::MissingCheckpoint(checkpoint_id))?;
        let mut config = self
            .manifest
            .configuration
            .simulation_config(self.recording.schedule.clone())?;
        // Host output paths are not execution inputs; restored work must not
        // overwrite the original run's logs or create ambient directories.
        config.dlog_dir = None;
        config.vm_config.dlog_path = None;
        let mut artifacts = Vec::new();
        for guest in &self.manifest.guest_artifacts {
            let file = sealed_verified_artifact(
                &self.root,
                &guest.artifact,
                self.limits.max_artifact_bytes,
            )?;
            artifacts.push(file);
        }
        config.vm_config.disk_image_path = config.disk_image_path.clone();
        let role_file = |role: GuestArtifactRole| {
            self.manifest
                .guest_artifacts
                .iter()
                .position(|artifact| artifact.role == role)
                .map(|index| &artifacts[index])
        };
        let kernel = role_file(GuestArtifactRole::Kernel)
            .ok_or_else(|| SessionError::Incompatible("kernel capability missing".into()))?;
        let mut controller = SimulationController::new_from_files(
            config,
            kernel,
            role_file(GuestArtifactRole::Initrd),
            role_file(GuestArtifactRole::Disk),
        )
        .map_err(|error| SessionError::Incompatible(error.to_string()))?;
        controller.start_fault_run_at(
            self.recording.schedule.clone(),
            self.recording.fault_run_sequence,
        );
        controller
            .restore_all(snapshot)
            .map_err(|error| SessionError::Incompatible(error.to_string()))?;
        let tick = controller.tick();
        for vm in 0..self.manifest.configuration.num_vms {
            controller.vm_mut(vm).bind_diagnostic_tick(tick);
        }
        Ok(RestoredController {
            controller,
            _artifacts: artifacts,
            session_identity: self.manifest.session_id.clone(),
            checkpoint_identity: self.checkpoint_identity(checkpoint_id)?.to_string(),
            checkpoint_tick: checkpoint.tick,
            selected_tick: checkpoint.tick,
        })
    }
    fn revalidate_retained(&self) -> Result<(), SessionError> {
        let bytes = read_bounded(
            open_relative(&self.root, MANIFEST_NAME)?,
            self.limits.max_manifest_bytes,
            MANIFEST_NAME,
        )?;
        let manifest: SessionManifest = serde_json::from_slice(&bytes)
            .map_err(|error| SessionError::Corrupt(error.to_string()))?;
        validate_manifest(&manifest, self.limits)?;
        if manifest.session_id != self.manifest.session_id {
            return Err(SessionError::Drift(MANIFEST_NAME.into()));
        }
        verify_streamed_artifact(
            &self.root,
            &self.manifest.recording,
            self.limits.max_recording_bytes,
            &mut std::io::sink(),
        )?;
        for checkpoint in &self.manifest.checkpoints {
            verify_streamed_artifact(
                &self.root,
                &checkpoint.artifact,
                self.limits.max_snapshot_bytes,
                &mut std::io::sink(),
            )?;
        }
        Ok(())
    }
    /// Restore the selected checkpoint and replay only the retained exact
    /// interval to a tick moment. All existing stage/control/SMP checks apply.
    pub fn restore_moment(
        &self,
        moment: &crate::moment::MomentContext,
    ) -> Result<RestoredController, SessionError> {
        moment
            .validate()
            .map_err(|error| SessionError::Incompatible(error.to_string()))?;
        if moment.session_id.as_deref() != Some(self.manifest.session_id.as_str())
            || moment.retained_start_tick != self.manifest.retained_start_tick
            || moment.retained_end_tick != self.manifest.retained_end_tick
            || !moment.source_ids.contains(&self.manifest.recording.digest)
        {
            return Err(SessionError::Incompatible("moment/session binding".into()));
        }
        let checkpoint = self
            .manifest
            .checkpoints
            .iter()
            .find(|checkpoint| {
                Some(checkpoint.artifact.digest.as_str()) == moment.checkpoint_id.as_deref()
            })
            .ok_or(SessionError::MissingCheckpoint(0))?;
        if checkpoint.tick > moment.tick {
            return Err(SessionError::Incompatible(
                "moment precedes checkpoint".into(),
            ));
        }
        if let Some(branch_id) = &moment.branch_id {
            if !self.manifest.branches.iter().any(|branch| {
                &branch.branch_id == branch_id
                    && branch.tick == moment.tick
                    && branch.checkpoint_id == checkpoint.id
            }) {
                return Err(SessionError::Incompatible("moment branch binding".into()));
            }
        }
        let restored = self.restore_controller(checkpoint.id)?;
        if checkpoint.tick == moment.tick {
            return Ok(restored);
        }
        let RestoredController {
            controller,
            _artifacts,
            session_identity,
            checkpoint_identity,
            checkpoint_tick,
            selected_tick: _,
        } = restored;
        let mut runner = crate::replay::RealSimulationRunner::from_controller(controller);
        crate::replay::advance_exact_runner(&self.recording, &mut runner, moment.tick)
            .map_err(|error| SessionError::Incompatible(error.to_string()))?;
        let mut controller = runner.into_controller();
        let tick = controller.tick();
        for vm in 0..self.manifest.configuration.num_vms {
            controller.vm_mut(vm).bind_diagnostic_tick(tick);
        }
        Ok(RestoredController {
            controller,
            _artifacts,
            session_identity,
            checkpoint_identity,
            checkpoint_tick,
            selected_tick: moment.tick,
        })
    }
    pub fn branch_graph_facts(&self) -> Vec<TimelineFact> {
        let branch_ids: BTreeMap<_, _> = self
            .manifest
            .branches
            .iter()
            .enumerate()
            .map(|(index, branch)| (branch.branch_id.as_str(), index as u64))
            .collect();
        let mut facts =
            Vec::with_capacity(self.manifest.branches.len() + self.manifest.checkpoints.len());
        for branch in &self.manifest.branches {
            facts.push(TimelineFact {
                id: branch_ids[branch.branch_id.as_str()],
                parent: branch.parent.as_deref().map(|parent| branch_ids[parent]),
                tick: branch.tick,
                kind: "branch".into(),
                label: branch.branch_id.clone(),
            });
        }
        let offset = self.manifest.branches.len() as u64;
        for (index, checkpoint) in self.manifest.checkpoints.iter().enumerate() {
            // Captures form one retained execution sequence. Do not infer a
            // fork from neighboring timestamps or marker labels.
            let parent = if index == 0 {
                self.manifest
                    .branches
                    .iter()
                    .find(|branch| branch.parent.is_none())
                    .map(|branch| branch_ids[branch.branch_id.as_str()])
            } else {
                Some(offset + index as u64 - 1)
            };
            facts.push(TimelineFact {
                id: offset + index as u64,
                parent,
                tick: checkpoint.tick,
                kind: "checkpoint".into(),
                label: checkpoint.artifact.digest.clone(),
            });
        }
        facts
    }
}

/// Owns verified guest-input capabilities and the immutable admitted selection.
/// Controller restart uses its retained initial snapshot, never a path fallback.
pub struct RestoredController {
    controller: SimulationController,
    _artifacts: Vec<File>,
    session_identity: String,
    checkpoint_identity: String,
    checkpoint_tick: u64,
    selected_tick: u64,
}
impl RestoredController {
    pub fn controller(&self) -> &SimulationController {
        &self.controller
    }
    pub fn controller_mut(&mut self) -> &mut SimulationController {
        &mut self.controller
    }
    pub fn session_identity(&self) -> &str {
        &self.session_identity
    }
    pub fn checkpoint_identity(&self) -> &str {
        &self.checkpoint_identity
    }
    pub fn checkpoint_tick(&self) -> u64 {
        self.checkpoint_tick
    }
    pub fn selected_tick(&self) -> u64 {
        self.selected_tick
    }
}
impl Deref for RestoredController {
    type Target = SimulationController;
    fn deref(&self) -> &Self::Target {
        &self.controller
    }
}
impl DerefMut for RestoredController {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.controller
    }
}

/// Persist controller-captured checkpoints with their retained guest provenance.
pub fn save_recorder(
    recorder: &Recorder,
    controller: &SimulationController,
    directory: &Path,
    limits: SessionLimits,
) -> Result<SessionManifest, SessionError> {
    save_recorder_with_branches(recorder, controller, directory, limits, &[])
}

/// Creates a new session directory; never overwrites an existing session. The
/// manifest is published last, so partial writes never masquerade as exact state.
/// Arbitrary `Recording` values and metadata cannot establish this capability.
pub fn save_recorder_with_branches(
    recorder: &Recorder,
    controller: &SimulationController,
    directory: &Path,
    limits: SessionLimits,
    branches: &[SessionBranch],
) -> Result<SessionManifest, SessionError> {
    if !recorder.matches_checkpoint_guest_artifacts(controller.loaded_guest_artifact_identities()) {
        return Err(SessionError::Incompatible(
            "recorded checkpoint/controller guest-input provenance mismatch".into(),
        ));
    }
    let recording = recorder.recording();
    if recording.total_ticks != controller.tick() {
        return Err(SessionError::Incompatible(
            "recording/controller horizon mismatch".into(),
        ));
    }
    let config = controller.config();
    limits.validate()?;
    validate_recording(recording).map_err(|error| SessionError::Corrupt(format!("{error:?}")))?;
    let configuration = SessionConfiguration::capture(config)?;
    validate_configuration(recording, &configuration)?;
    if recording.checkpoints.is_empty() {
        return Err(SessionError::MissingCheckpoint(0));
    }
    if recording.checkpoints.len() > limits.max_checkpoints {
        return Err(SessionError::Bound("checkpoint count"));
    }
    let retained_memory = (config.vm_config.memory_size as u64)
        .checked_mul(config.num_vms as u64)
        .and_then(|bytes| bytes.checked_mul(recording.checkpoints.len() as u64))
        .ok_or(SessionError::Bound("retained snapshot memory"))?;
    if retained_memory > limits.max_total_bytes {
        return Err(SessionError::Bound("retained snapshot memory"));
    }
    for checkpoint in recording.checkpoints.all() {
        validate_checkpoint(recording, checkpoint, &configuration)?;
    }
    let recording_bytes = bounded_json(recording, limits.max_recording_bytes, "recording bytes")?;
    let mut retained_bytes = recording_bytes.len() as u64;
    if retained_bytes > limits.max_total_bytes {
        return Err(SessionError::Bound("total bytes"));
    }
    let root = create_directory(directory)?;
    mkdir_child(&root, "artifacts")?;
    mkdir_child(&root, "snapshots")?;
    let recording_artifact = write_artifact(&root, "recording.json", &recording_bytes)?;
    let mut guest_artifacts = Vec::new();
    for (role, path) in [
        (GuestArtifactRole::Kernel, Some(config.kernel_path.as_str())),
        (GuestArtifactRole::Initrd, config.initrd_path.as_deref()),
        (GuestArtifactRole::Disk, config.disk_image_path.as_deref()),
    ] {
        if let Some(path) = path {
            let input = open_file_path(Path::new(path))?;
            let temporary = format!("artifacts/.capture-{}", guest_artifacts.len());
            let (temporary_dir, temporary_name) = relative_parent(&root, &temporary)?;
            let mut retained = open_at(
                &temporary_dir,
                std::ffi::OsStr::new(&temporary_name),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            )?;
            let mut loaded_hash = blake3::Hasher::new();
            loaded_hash.update(chaoscontrol_vmm::controller::GUEST_ARTIFACT_IDENTITY_DOMAIN);
            let requires_loaded_identity = role != GuestArtifactRole::Disk;
            let remaining = limits
                .max_total_bytes
                .checked_sub(retained_bytes)
                .ok_or(SessionError::Bound("total bytes"))?;
            let (digest, bytes) = stream_copy(
                &input,
                &mut retained,
                limits.max_artifact_bytes.min(remaining),
                path,
                if requires_loaded_identity {
                    Some(&mut loaded_hash)
                } else {
                    None
                },
            )?;
            if requires_loaded_identity {
                let index = usize::from(role == GuestArtifactRole::Initrd);
                if !controller.matches_loaded_guest_artifact_identity(
                    index,
                    loaded_hash.finalize().as_bytes(),
                ) {
                    return Err(SessionError::Drift(path.into()));
                }
            }
            if role == GuestArtifactRole::Disk {
                validate_disk_base(recording, &retained)?;
            }
            retained.sync_all()?;
            let relative = format!("artifacts/{}.bin", &digest[3..]);
            let duplicate = guest_artifacts
                .iter()
                .any(|entry: &GuestArtifact| entry.artifact.path == relative);
            publish_staged_artifact(&root, &temporary, &relative, duplicate)?;
            if !duplicate {
                retained_bytes = retained_bytes
                    .checked_add(bytes)
                    .ok_or(SessionError::Bound("total bytes"))?;
            }
            let artifact = SessionArtifact {
                path: relative,
                digest,
                bytes,
            };
            guest_artifacts.push(GuestArtifact { role, artifact });
        }
    }
    let mut checkpoints = Vec::new();
    let mut decoded_payload_bytes = 0u64;
    for checkpoint in recording.checkpoints.all() {
        let snapshot = checkpoint
            .snapshot
            .as_ref()
            .ok_or(SessionError::MissingCheckpoint(checkpoint.id))?;
        let remaining = limits
            .max_total_bytes
            .checked_sub(retained_bytes)
            .ok_or(SessionError::Bound("total bytes"))?;
        if remaining == 0 {
            return Err(SessionError::Bound("total bytes"));
        }
        let decoded_remaining = limits
            .max_total_bytes
            .saturating_sub(decoded_payload_bytes)
            .min(MAX_SESSION_DECODED_SNAPSHOT_BYTES);
        let (reference, bytes, decoded_bytes) = encode_snapshot_artifact_with_limits(
            snapshot,
            0,
            remaining.min(limits.max_snapshot_bytes),
            decoded_remaining,
        )
        .map_err(snapshot_codec_error)?;
        decoded_payload_bytes = decoded_payload_bytes
            .checked_add(decoded_bytes)
            .ok_or(SessionError::Bound("decoded snapshot bytes"))?;
        if !checkpoints
            .iter()
            .any(|prior: &SessionCheckpoint| prior.artifact.path == reference.path)
        {
            retained_bytes = retained_bytes
                .checked_add(bytes.len() as u64)
                .ok_or(SessionError::Bound("total bytes"))?;
            write_artifact(&root, &reference.path, &bytes)?;
        }
        checkpoints.push(SessionCheckpoint {
            id: checkpoint.id,
            tick: checkpoint.tick,
            artifact: SessionArtifact {
                path: reference.path.clone(),
                digest: digest_bytes(&bytes),
                bytes: bytes.len() as u64,
            },
            snapshot_ref: reference,
            serial_output: checkpoint.serial_output.clone(),
            events_since_last: checkpoint.events_since_last.clone(),
        });
    }
    let branches = if branches.is_empty() {
        vec![SessionBranch {
            branch_id: digest_bytes(format!("root:{}", recording_artifact.digest).as_bytes()),
            parent: None,
            checkpoint_id: checkpoints[0].id,
            tick: checkpoints[0].tick,
        }]
    } else {
        branches.to_vec()
    };
    let mut manifest = SessionManifest {
        schema_version: SESSION_SCHEMA_VERSION,
        session_id: String::new(),
        config_identity: digest_json(&configuration)?,
        configuration,
        recording: recording_artifact,
        guest_artifacts,
        checkpoints,
        branches,
        retained_start_tick: 0,
        retained_end_tick: recording.total_ticks,
        limits,
    };
    manifest.session_id = manifest_identity(&manifest)?;
    validate_manifest(&manifest, limits)?;
    let bytes = bounded_json(&manifest, limits.max_manifest_bytes, "manifest bytes")?;
    write_artifact(&root, MANIFEST_NAME, &bytes)?;
    root.sync_all()?;
    Ok(manifest)
}

fn validate_disk_base(recording: &Recording, file: &File) -> Result<(), SessionError> {
    for checkpoint in recording.checkpoints.all() {
        let snapshot = checkpoint
            .snapshot
            .as_ref()
            .ok_or(SessionError::MissingCheckpoint(checkpoint.id))?;
        for (vm, _) in &snapshot.vm_snapshots {
            let mut found = false;
            for device in &vm.virtio_snapshots {
                if let chaoscontrol_vmm::snapshot::VirtioBackendSnapshot::Block(block) =
                    &device.backend
                {
                    if !block.matches_base_image_file(file)? {
                        return Err(SessionError::Drift("disk snapshot base".into()));
                    }
                    found = true;
                }
            }
            if !found {
                return Err(SessionError::Incompatible(
                    "configured disk has no retained backend".into(),
                ));
            }
        }
    }
    Ok(())
}

pub fn load_session(directory: &Path, limits: SessionLimits) -> Result<ExactSession, SessionError> {
    limits.validate()?;
    let root = open_directory(directory).map_err(|error| {
        if matches!(&error, SessionError::Io(source) if source.kind() == std::io::ErrorKind::NotFound) {
            SessionError::MissingSession
        } else { error }
    })?;
    load_from_directory(root, limits)
}
fn load_from_directory(root: File, limits: SessionLimits) -> Result<ExactSession, SessionError> {
    let manifest_file = open_relative(&root, MANIFEST_NAME).map_err(|error| match error {
        SessionError::MissingArtifact(_) => SessionError::MissingSession,
        other => other,
    })?;
    let manifest_bytes = read_bounded(manifest_file, limits.max_manifest_bytes, MANIFEST_NAME)?;
    let manifest: SessionManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| SessionError::Corrupt(error.to_string()))?;
    validate_manifest(&manifest, limits)?;
    let recording_bytes = read_verified(&root, &manifest.recording, limits.max_recording_bytes)?;
    let mut recording: Recording = serde_json::from_slice(&recording_bytes)
        .map_err(|error| SessionError::Corrupt(error.to_string()))?;
    validate_configuration(&recording, &manifest.configuration)?;
    if recording.total_ticks != manifest.retained_end_tick || manifest.retained_start_tick != 0 {
        return Err(SessionError::Corrupt("recording horizon".into()));
    }
    for guest in &manifest.guest_artifacts {
        verify_streamed_artifact(
            &root,
            &guest.artifact,
            limits.max_artifact_bytes,
            &mut std::io::sink(),
        )?;
    }
    let mut store = CheckpointStore::new();
    let mut decoded_payload_bytes = 0u64;
    for checkpoint in &manifest.checkpoints {
        let bytes = read_verified(&root, &checkpoint.artifact, limits.max_snapshot_bytes)?;
        let decoded_remaining = limits
            .max_total_bytes
            .min(manifest.limits.max_total_bytes)
            .saturating_sub(decoded_payload_bytes)
            .min(MAX_SESSION_DECODED_SNAPSHOT_BYTES);
        let (artifact, decoded_bytes) = decode_snapshot_artifact_with_limit(
            &checkpoint.snapshot_ref,
            &bytes,
            decoded_remaining,
        )
        .map_err(snapshot_codec_error)?;
        decoded_payload_bytes = decoded_payload_bytes
            .checked_add(decoded_bytes)
            .ok_or(SessionError::Bound("decoded snapshot bytes"))?;
        if artifact.tick != checkpoint.tick
            || artifact.vm_count != manifest.configuration.num_vms
            || artifact.replay_parent_depth != 0
        {
            return Err(SessionError::Corrupt(
                "snapshot envelope/checkpoint mismatch".into(),
            ));
        }
        store.push(Checkpoint {
            id: checkpoint.id,
            tick: checkpoint.tick,
            snapshot: Some(artifact.snapshot),
            serial_output: checkpoint.serial_output.clone(),
            events_since_last: checkpoint.events_since_last.clone(),
        });
    }
    recording.checkpoints = store;
    validate_recording(&recording).map_err(|error| SessionError::Corrupt(format!("{error:?}")))?;
    for checkpoint in recording.checkpoints.all() {
        validate_checkpoint(&recording, checkpoint, &manifest.configuration)?;
    }
    Ok(ExactSession {
        root,
        manifest,
        recording,
        limits,
    })
}
fn snapshot_codec_error(
    error: chaoscontrol_explore::snapshot_store::SnapshotStoreError,
) -> SessionError {
    use chaoscontrol_explore::snapshot_store::SnapshotStoreError;
    match error {
        SnapshotStoreError::TooLarge { .. } | SnapshotStoreError::DecompressedTooLarge { .. } => {
            SessionError::Bound("snapshot codec bytes")
        }
        other => SessionError::Corrupt(other.to_string()),
    }
}
fn bounded_json<T: Serialize + ?Sized>(
    value: &T,
    limit: u64,
    name: &'static str,
) -> Result<Vec<u8>, SessionError> {
    let mut output = BoundedWriter {
        bytes: Vec::new(),
        limit,
        exceeded: false,
    };
    serde_json::to_writer(&mut output, value).map_err(|error| {
        if output.exceeded {
            SessionError::Bound(name)
        } else {
            SessionError::Corrupt(error.to_string())
        }
    })?;
    Ok(output.bytes)
}
struct BoundedWriter {
    bytes: Vec<u8>,
    limit: u64,
    exceeded: bool,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if (self.bytes.len() as u64).saturating_add(bytes.len() as u64) > self.limit {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "session serialization byte bound exceeded",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn validate_relative_path(path: &str) -> Result<(), SessionError> {
    if path.is_empty()
        || path.len() > MAX_SESSION_PATH_BYTES
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || path.as_bytes().contains(&0)
        || path.contains("//")
    {
        return Err(SessionError::Path(path.into()));
    }
    Ok(())
}
fn c_path(path: &std::ffi::OsStr) -> Result<CString, SessionError> {
    CString::new(path.as_bytes())
        .map_err(|_| SessionError::Path(path.to_string_lossy().into_owned()))
}
fn open_at(directory: &File, name: &std::ffi::OsStr, flags: i32) -> Result<File, SessionError> {
    let name = c_path(name)?;
    // SAFETY: directory and NUL-terminated name remain live; returned fd is uniquely owned.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn open_directory(path: &Path) -> Result<File, SessionError> {
    if path.as_os_str().as_bytes().len() > MAX_SESSION_PATH_BYTES {
        return Err(SessionError::Bound("path bytes"));
    }
    let mut directory = File::open(if path.is_absolute() { "/" } else { "." })?;
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => (),
            Component::Normal(name) => {
                directory = open_at(&directory, name, libc::O_RDONLY | libc::O_DIRECTORY)?
            }
            _ => return Err(SessionError::Path(path.display().to_string())),
        }
    }
    Ok(directory)
}
fn create_directory(path: &Path) -> Result<File, SessionError> {
    let name = path
        .file_name()
        .ok_or_else(|| SessionError::Path(path.display().to_string()))?;
    let parent = open_directory(path.parent().unwrap_or_else(|| Path::new(".")))?;
    mkdir_child(
        &parent,
        name.to_str()
            .ok_or_else(|| SessionError::Path(path.display().to_string()))?,
    )?;
    open_at(&parent, name, libc::O_RDONLY | libc::O_DIRECTORY)
}
fn mkdir_child(directory: &File, name: &str) -> Result<(), SessionError> {
    validate_relative_path(name)?;
    let name = c_path(std::ffi::OsStr::new(name))?;
    // SAFETY: valid pinned parent and NUL-terminated relative path.
    if unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
fn open_file_path(path: &Path) -> Result<File, SessionError> {
    let directory = open_directory(path.parent().unwrap_or_else(|| Path::new(".")))?;
    let name = path
        .file_name()
        .ok_or_else(|| SessionError::Path(path.display().to_string()))?;
    let file = open_at(&directory, name, libc::O_RDONLY)?;
    if !file.metadata()?.is_file() {
        return Err(SessionError::Path(path.display().to_string()));
    }
    Ok(file)
}
fn relative_parent(root: &File, path: &str) -> Result<(File, String), SessionError> {
    validate_relative_path(path)?;
    let path = Path::new(path);
    let mut directory = root.try_clone()?;
    if let Some(parent) = path.parent() {
        for name in parent.components() {
            directory = open_at(
                &directory,
                name.as_os_str(),
                libc::O_RDONLY | libc::O_DIRECTORY,
            )?;
        }
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| SessionError::Path(path.display().to_string()))?;
    Ok((directory, name.into()))
}
fn open_relative(root: &File, path: &str) -> Result<File, SessionError> {
    let (directory, name) = relative_parent(root, path).map_err(|error| {
        if matches!(&error, SessionError::Io(source) if source.kind() == std::io::ErrorKind::NotFound) {
            SessionError::MissingArtifact(path.into())
        } else { error }
    })?;
    let file = open_at(&directory, std::ffi::OsStr::new(&name), libc::O_RDONLY).map_err(|error| {
        if matches!(&error, SessionError::Io(source) if source.kind() == std::io::ErrorKind::NotFound) {
            SessionError::MissingArtifact(path.into())
        } else { error }
    })?;
    if !file.metadata()?.is_file() {
        return Err(SessionError::Path(path.into()));
    }
    Ok(file)
}
fn read_bounded(mut file: File, limit: u64, path: &str) -> Result<Vec<u8>, SessionError> {
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(SessionError::Path(path.into()));
    }
    if before.len() > limit {
        return Err(SessionError::Bound("file bytes"));
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(SessionError::Bound("file bytes"));
    }
    if bytes.len() as u64 != before.len() || file.metadata()?.len() != before.len() {
        return Err(SessionError::Drift(path.into()));
    }
    Ok(bytes)
}
fn read_verified(
    root: &File,
    artifact: &SessionArtifact,
    limit: u64,
) -> Result<Vec<u8>, SessionError> {
    let bytes = read_bounded(open_relative(root, &artifact.path)?, limit, &artifact.path)?;
    if bytes.len() as u64 != artifact.bytes || digest_bytes(&bytes) != artifact.digest {
        return Err(SessionError::Drift(artifact.path.clone()));
    }
    Ok(bytes)
}
fn write_artifact(root: &File, path: &str, bytes: &[u8]) -> Result<SessionArtifact, SessionError> {
    let (directory, name) = relative_parent(root, path)?;
    let mut file = open_at(
        &directory,
        std::ffi::OsStr::new(&name),
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
    )?;
    file.write_all(bytes)?;
    file.sync_all()?;
    directory.sync_all()?;
    Ok(SessionArtifact {
        path: path.into(),
        digest: digest_bytes(bytes),
        bytes: bytes.len() as u64,
    })
}

fn new_artifact_memfd() -> Result<File, SessionError> {
    let name = c_path(std::ffi::OsStr::new("chaoscontrol-session-input"))?;
    // SAFETY: NUL-terminated name; a successful result is a newly owned fd.
    let fd =
        unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: memfd_create returned a uniquely owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn seal_artifact(file: File) -> Result<File, SessionError> {
    // SAFETY: fcntl acts on our live private descriptor; no pointers are used.
    let result = unsafe {
        libc::fcntl(
            file.as_raw_fd(),
            libc::F_ADD_SEALS,
            libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL,
        )
    };
    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(file)
}
fn sealed_verified_artifact(
    root: &File,
    artifact: &SessionArtifact,
    limit: u64,
) -> Result<File, SessionError> {
    let mut file = new_artifact_memfd()?;
    verify_streamed_artifact(root, artifact, limit, &mut file)?;
    seal_artifact(file)
}
#[cfg(test)]
fn sealed_artifact(bytes: &[u8]) -> Result<File, SessionError> {
    let mut file = new_artifact_memfd()?;
    file.write_all(bytes)?;
    seal_artifact(file)
}

fn verify_streamed_artifact(
    root: &File,
    artifact: &SessionArtifact,
    limit: u64,
    sink: &mut impl Write,
) -> Result<(), SessionError> {
    let input = open_relative(root, &artifact.path)?;
    let (digest, bytes) = stream_copy(&input, sink, limit, &artifact.path, None)?;
    if digest != artifact.digest || bytes != artifact.bytes {
        return Err(SessionError::Drift(artifact.path.clone()));
    }
    Ok(())
}
fn stream_copy(
    input: &File,
    sink: &mut impl Write,
    limit: u64,
    path: &str,
    mut extra_hash: Option<&mut blake3::Hasher>,
) -> Result<(String, u64), SessionError> {
    use std::os::unix::fs::FileExt;
    const BUFFER_BYTES: usize = 64 * 1024;
    let metadata = input.metadata()?;
    if !metadata.is_file() {
        return Err(SessionError::Path(path.into()));
    }
    if metadata.len() > limit {
        return Err(SessionError::Bound("guest artifact bytes"));
    }
    let mut buffer = [0; BUFFER_BYTES];
    let mut hasher = blake3::Hasher::new();
    let mut offset = 0;
    while offset < metadata.len() {
        let count = ((metadata.len() - offset) as usize).min(BUFFER_BYTES);
        let read = input.read_at(&mut buffer[..count], offset)?;
        if read == 0 {
            return Err(SessionError::Drift(path.into()));
        }
        sink.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        if let Some(hash) = extra_hash.as_deref_mut() {
            hash.update(&buffer[..read]);
        }
        offset += read as u64;
    }
    let mut sentinel = [0];
    if input.read_at(&mut sentinel, offset)? != 0 || input.metadata()?.len() != metadata.len() {
        return Err(SessionError::Drift(path.into()));
    }
    Ok((format!("b3:{}", hasher.finalize().to_hex()), offset))
}
fn publish_staged_artifact(
    root: &File,
    temporary: &str,
    destination: &str,
    duplicate: bool,
) -> Result<(), SessionError> {
    let (source_dir, source_name) = relative_parent(root, temporary)?;
    let source = c_path(std::ffi::OsStr::new(&source_name))?;
    let result = if duplicate {
        // SAFETY: this is the newly created staging leaf in our pinned directory.
        unsafe { libc::unlinkat(source_dir.as_raw_fd(), source.as_ptr(), 0) }
    } else {
        let (destination_dir, destination_name) = relative_parent(root, destination)?;
        let destination = c_path(std::ffi::OsStr::new(&destination_name))?;
        // SAFETY: live pinned directory handles and NUL-terminated leaf names;
        // no existing artifact can be replaced.
        unsafe {
            libc::renameat2(
                source_dir.as_raw_fd(),
                source.as_ptr(),
                destination_dir.as_raw_fd(),
                destination.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        }
    };
    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    source_dir.sync_all()?;
    Ok(())
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
