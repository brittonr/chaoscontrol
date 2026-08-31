use crate::process::{ProcessFaultAction, ProcessFaultCommand};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PROCESS_MANIFEST_SCHEMA: &str = "chaoscontrol.process-manifest.v1";
pub const MULTIPROCESS_RECEIPT_SCHEMA: &str = "chaoscontrol.multiprocess-receipt.v1";
pub const PROCESS_CLAIM_SCOPE: &str = "declared-processes-only";
pub const MAX_PROCESSES: usize = 32;
pub const MAX_SHARED_DIRECTORIES: usize = 16;
pub const MAX_ARGUMENTS: usize = 64;
pub const MAX_ENVIRONMENT_FIELDS: usize = 64;
pub const MAX_TEXT_BYTES: usize = 256;
pub const MAX_PATH_BYTES: usize = 1024;
pub const MAX_RESTARTS: u32 = 64;
const HASH_DOMAIN: &[u8] = b"chaoscontrol.guest-process.v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedDeviceKind {
    Memory,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedDirectorySpec {
    pub id: String,
    pub path: String,
    pub device: SharedDeviceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartMode {
    Never,
    OnFailure,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestartPolicy {
    pub mode: RestartMode,
    pub max_restarts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessSpec {
    pub role: String,
    pub executable: String,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub shared_directories: Vec<String>,
    pub restart: RestartPolicy,
    pub instrumented: bool,
    pub transport_slot: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessManifest {
    pub schema: String,
    pub guest: String,
    pub shared_directories: Vec<SharedDirectorySpec>,
    pub processes: Vec<ProcessSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmittedProcess {
    pub identity: String,
    pub spec: ProcessSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmittedManifest {
    pub manifest_identity: String,
    pub guest: String,
    pub shared_directories: Vec<SharedDirectorySpec>,
    pub processes: Vec<AdmittedProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    InvalidSchema,
    ProcessLimit,
    SharedDirectoryLimit,
    InvalidGuest,
    InvalidRole,
    DuplicateRole,
    InvalidExecutable,
    ArgumentLimit,
    EnvironmentLimit,
    InvalidEnvironment,
    InvalidSharedDirectory,
    DuplicateSharedDirectory,
    UnknownSharedDirectory,
    InvalidRestartPolicy,
    MissingTransportSlot,
    UnexpectedTransportSlot,
    DuplicateTransportSlot,
}

pub fn admit_manifest(manifest: &ProcessManifest) -> Result<AdmittedManifest, ManifestError> {
    if manifest.schema != PROCESS_MANIFEST_SCHEMA {
        return Err(ManifestError::InvalidSchema);
    }
    validate_token(&manifest.guest).map_err(|()| ManifestError::InvalidGuest)?;
    if manifest.processes.len() > MAX_PROCESSES {
        return Err(ManifestError::ProcessLimit);
    }
    if manifest.shared_directories.len() > MAX_SHARED_DIRECTORIES {
        return Err(ManifestError::SharedDirectoryLimit);
    }

    let mut directory_ids = BTreeSet::new();
    for directory in &manifest.shared_directories {
        validate_token(&directory.id).map_err(|()| ManifestError::InvalidSharedDirectory)?;
        validate_absolute_path(&directory.path)
            .map_err(|()| ManifestError::InvalidSharedDirectory)?;
        if !directory_ids.insert(directory.id.clone()) {
            return Err(ManifestError::DuplicateSharedDirectory);
        }
    }

    let mut roles = BTreeSet::new();
    let mut slots = BTreeSet::new();
    let mut processes = Vec::with_capacity(manifest.processes.len());
    for process in &manifest.processes {
        validate_token(&process.role).map_err(|()| ManifestError::InvalidRole)?;
        if !roles.insert(process.role.clone()) {
            return Err(ManifestError::DuplicateRole);
        }
        validate_absolute_path(&process.executable)
            .map_err(|()| ManifestError::InvalidExecutable)?;
        if process.arguments.len() > MAX_ARGUMENTS {
            return Err(ManifestError::ArgumentLimit);
        }
        if process.environment.len() > MAX_ENVIRONMENT_FIELDS {
            return Err(ManifestError::EnvironmentLimit);
        }
        for argument in &process.arguments {
            validate_text(argument).map_err(|()| ManifestError::ArgumentLimit)?;
        }
        for (name, value) in &process.environment {
            validate_environment_name(name).map_err(|()| ManifestError::InvalidEnvironment)?;
            validate_text(value).map_err(|()| ManifestError::InvalidEnvironment)?;
        }
        for directory in &process.shared_directories {
            if !directory_ids.contains(directory) {
                return Err(ManifestError::UnknownSharedDirectory);
            }
        }
        if process.restart.max_restarts > MAX_RESTARTS {
            return Err(ManifestError::InvalidRestartPolicy);
        }
        match (process.instrumented, process.transport_slot) {
            (true, Some(slot)) if slots.insert(slot) => {}
            (true, Some(_)) => return Err(ManifestError::DuplicateTransportSlot),
            (true, None) => return Err(ManifestError::MissingTransportSlot),
            (false, Some(_)) => return Err(ManifestError::UnexpectedTransportSlot),
            (false, None) => {}
        }
        processes.push(AdmittedProcess {
            identity: process_identity(&manifest.guest, &process.role),
            spec: process.clone(),
        });
    }

    Ok(AdmittedManifest {
        manifest_identity: manifest_identity(manifest),
        guest: manifest.guest.clone(),
        shared_directories: manifest.shared_directories.clone(),
        processes,
    })
}

pub fn process_identity(guest: &str, role: &str) -> String {
    digest(&[b"process", guest.as_bytes(), role.as_bytes()])
}

pub fn manifest_identity(manifest: &ProcessManifest) -> String {
    let bytes = serde_json::to_vec(manifest).expect("process manifest serialization is infallible");
    digest(&[b"manifest", &bytes])
}

pub fn shared_directory_identity(directory: &SharedDirectorySpec) -> String {
    let device = match directory.device {
        SharedDeviceKind::Memory => b"memory".as_slice(),
        SharedDeviceKind::Block => b"block".as_slice(),
    };
    digest(&[
        b"shared-directory",
        directory.id.as_bytes(),
        directory.path.as_bytes(),
        device,
    ])
}

fn digest(parts: &[&[u8]]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(HASH_DOMAIN);
    for part in parts {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("b3:{}", hasher.finalize().to_hex())
}

fn validate_token(value: &str) -> Result<(), ()> {
    if value.is_empty() || value.len() > MAX_TEXT_BYTES {
        return Err(());
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        Ok(())
    } else {
        Err(())
    }
}

fn validate_text(value: &str) -> Result<(), ()> {
    if value.len() > MAX_TEXT_BYTES || value.chars().any(char::is_control) {
        Err(())
    } else {
        Ok(())
    }
}

fn validate_environment_name(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value.len() > MAX_TEXT_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        Err(())
    } else {
        Ok(())
    }
}

fn validate_absolute_path(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value.len() > MAX_PATH_BYTES
        || !value.starts_with('/')
        || value.split('/').any(|component| component == "..")
        || value.chars().any(char::is_control)
    {
        Err(())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Stopped,
    Running,
    Paused,
    Exited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRuntimeState {
    pub identity: String,
    pub role: String,
    pub status: ProcessStatus,
    pub generation: u32,
    pub restart_count: u32,
    pub resume_at_tick: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorState {
    pub manifest_identity: String,
    pub tick: u64,
    pub processes: BTreeMap<String, ProcessRuntimeState>,
    pub shared_directory_identities: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleKind {
    Spawned,
    Exited,
    Restarted,
    Killed,
    Paused,
    Resumed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessLifecycleEvent {
    pub process_identity: String,
    pub role: String,
    pub generation: u32,
    pub tick: u64,
    pub kind: LifecycleKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorEffectKind {
    Spawn,
    Kill,
    Pause,
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorEffect {
    pub process_identity: String,
    pub kind: SupervisorEffectKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorTransition {
    pub next: SupervisorState,
    pub effects: Vec<SupervisorEffect>,
    pub events: Vec<ProcessLifecycleEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorError {
    UnknownProcess,
    AmbiguousRole,
    InvalidState,
    RestartExhausted,
    TickOverflow,
}

impl SupervisorState {
    pub fn new(manifest: &AdmittedManifest) -> Self {
        let processes = manifest
            .processes
            .iter()
            .map(|process| {
                (
                    process.identity.clone(),
                    ProcessRuntimeState {
                        identity: process.identity.clone(),
                        role: process.spec.role.clone(),
                        status: ProcessStatus::Stopped,
                        generation: 0,
                        restart_count: 0,
                        resume_at_tick: None,
                    },
                )
            })
            .collect();
        let shared_directory_identities = manifest
            .shared_directories
            .iter()
            .map(|directory| (directory.id.clone(), shared_directory_identity(directory)))
            .collect();
        Self {
            manifest_identity: manifest.manifest_identity.clone(),
            tick: 0,
            processes,
            shared_directory_identities,
        }
    }

    pub fn start_all(&self) -> Result<SupervisorTransition, SupervisorError> {
        let mut next = self.clone();
        let mut effects = Vec::with_capacity(next.processes.len());
        let mut events = Vec::with_capacity(next.processes.len());
        for process in next.processes.values_mut() {
            if process.status != ProcessStatus::Stopped {
                return Err(SupervisorError::InvalidState);
            }
            process.status = ProcessStatus::Running;
            process.generation = process
                .generation
                .checked_add(1)
                .ok_or(SupervisorError::RestartExhausted)?;
            effects.push(SupervisorEffect {
                process_identity: process.identity.clone(),
                kind: SupervisorEffectKind::Spawn,
            });
            events.push(event(process, next.tick, LifecycleKind::Spawned));
        }
        Ok(SupervisorTransition {
            next,
            effects,
            events,
        })
    }

    pub fn observe_exit(
        &self,
        manifest: &AdmittedManifest,
        identity: &str,
        success: bool,
    ) -> Result<SupervisorTransition, SupervisorError> {
        let admitted = manifest
            .processes
            .iter()
            .find(|process| process.identity == identity)
            .ok_or(SupervisorError::UnknownProcess)?;
        let mut next = self.clone();
        let process = next
            .processes
            .get_mut(identity)
            .ok_or(SupervisorError::UnknownProcess)?;
        if !matches!(
            process.status,
            ProcessStatus::Running | ProcessStatus::Paused
        ) {
            return Err(SupervisorError::InvalidState);
        }
        process.status = ProcessStatus::Exited;
        process.resume_at_tick = None;
        let mut effects = Vec::new();
        let mut events = vec![event(process, next.tick, LifecycleKind::Exited)];
        let restart = match admitted.spec.restart.mode {
            RestartMode::Never => false,
            RestartMode::OnFailure => !success,
            RestartMode::Always => true,
        };
        if restart {
            if process.restart_count >= admitted.spec.restart.max_restarts {
                return Err(SupervisorError::RestartExhausted);
            }
            process.restart_count = process
                .restart_count
                .checked_add(1)
                .ok_or(SupervisorError::RestartExhausted)?;
            process.generation = process
                .generation
                .checked_add(1)
                .ok_or(SupervisorError::RestartExhausted)?;
            process.status = ProcessStatus::Running;
            effects.push(SupervisorEffect {
                process_identity: identity.to_string(),
                kind: SupervisorEffectKind::Spawn,
            });
            events.push(event(process, next.tick, LifecycleKind::Restarted));
        }
        Ok(SupervisorTransition {
            next,
            effects,
            events,
        })
    }

    pub fn apply_fault(
        &self,
        command: &ProcessFaultCommand,
    ) -> Result<SupervisorTransition, SupervisorError> {
        command
            .validate()
            .map_err(|_| SupervisorError::InvalidState)?;
        let identity = self.resolve_target(&command.target)?;
        let mut next = self.clone();
        let process = next
            .processes
            .get_mut(&identity)
            .ok_or(SupervisorError::UnknownProcess)?;
        let (effect_kind, event_kind) = match command.action {
            ProcessFaultAction::Kill => {
                if process.status != ProcessStatus::Running {
                    return Err(SupervisorError::InvalidState);
                }
                process.status = ProcessStatus::Exited;
                (SupervisorEffectKind::Kill, LifecycleKind::Killed)
            }
            ProcessFaultAction::Pause => {
                if process.status != ProcessStatus::Running {
                    return Err(SupervisorError::InvalidState);
                }
                let pause_ticks = command.pause_ticks.ok_or(SupervisorError::InvalidState)?;
                process.resume_at_tick = Some(
                    next.tick
                        .checked_add(pause_ticks)
                        .ok_or(SupervisorError::TickOverflow)?,
                );
                process.status = ProcessStatus::Paused;
                (SupervisorEffectKind::Pause, LifecycleKind::Paused)
            }
            ProcessFaultAction::Restart => {
                if !matches!(
                    process.status,
                    ProcessStatus::Running | ProcessStatus::Paused
                ) {
                    return Err(SupervisorError::InvalidState);
                }
                process.generation = process
                    .generation
                    .checked_add(1)
                    .ok_or(SupervisorError::RestartExhausted)?;
                process.restart_count = process
                    .restart_count
                    .checked_add(1)
                    .ok_or(SupervisorError::RestartExhausted)?;
                process.status = ProcessStatus::Running;
                process.resume_at_tick = None;
                (SupervisorEffectKind::Spawn, LifecycleKind::Restarted)
            }
        };
        let events = vec![event(process, next.tick, event_kind)];
        let mut effects = Vec::new();
        if command.action == ProcessFaultAction::Restart {
            effects.push(SupervisorEffect {
                process_identity: identity.clone(),
                kind: SupervisorEffectKind::Kill,
            });
        }
        effects.push(SupervisorEffect {
            process_identity: identity,
            kind: effect_kind,
        });
        Ok(SupervisorTransition {
            next,
            effects,
            events,
        })
    }

    pub fn advance_tick(&self) -> Result<SupervisorTransition, SupervisorError> {
        let mut next = self.clone();
        next.tick = next
            .tick
            .checked_add(1)
            .ok_or(SupervisorError::TickOverflow)?;
        let mut effects = Vec::new();
        let mut events = Vec::new();
        for process in next.processes.values_mut() {
            if process.status == ProcessStatus::Paused
                && process.resume_at_tick.is_some_and(|tick| tick <= next.tick)
            {
                process.status = ProcessStatus::Running;
                process.resume_at_tick = None;
                effects.push(SupervisorEffect {
                    process_identity: process.identity.clone(),
                    kind: SupervisorEffectKind::Resume,
                });
                events.push(event(process, next.tick, LifecycleKind::Resumed));
            }
        }
        Ok(SupervisorTransition {
            next,
            effects,
            events,
        })
    }

    fn resolve_target(&self, target: &str) -> Result<String, SupervisorError> {
        if self.processes.contains_key(target) {
            return Ok(target.to_string());
        }
        let mut matches = self
            .processes
            .values()
            .filter(|process| process.role == target)
            .map(|process| process.identity.clone());
        let first = matches.next().ok_or(SupervisorError::UnknownProcess)?;
        if matches.next().is_some() {
            return Err(SupervisorError::AmbiguousRole);
        }
        Ok(first)
    }
}

fn event(process: &ProcessRuntimeState, tick: u64, kind: LifecycleKind) -> ProcessLifecycleEvent {
    ProcessLifecycleEvent {
        process_identity: process.identity.clone(),
        role: process.role.clone(),
        generation: process.generation,
        tick,
        kind,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultiprocessReceipt {
    pub schema: String,
    pub manifest_identity: String,
    pub process_identities: Vec<String>,
    pub shared_directory_identities: Vec<String>,
    pub events: Vec<ProcessLifecycleEvent>,
    pub claim_scope: String,
}

pub fn make_receipt(
    manifest: &AdmittedManifest,
    events: Vec<ProcessLifecycleEvent>,
) -> MultiprocessReceipt {
    MultiprocessReceipt {
        schema: MULTIPROCESS_RECEIPT_SCHEMA.to_string(),
        manifest_identity: manifest.manifest_identity.clone(),
        process_identities: manifest
            .processes
            .iter()
            .map(|process| process.identity.clone())
            .collect(),
        shared_directory_identities: manifest
            .shared_directories
            .iter()
            .map(shared_directory_identity)
            .collect(),
        events,
        claim_scope: PROCESS_CLAIM_SCOPE.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptError {
    InvalidSchema,
    ManifestIdentityMismatch,
    ProcessIdentityMismatch,
    SharedDirectoryIdentityMismatch,
    EventScopeMismatch,
    ClaimOverreach,
}

pub fn validate_receipt(
    manifest: &AdmittedManifest,
    receipt: &MultiprocessReceipt,
) -> Result<(), ReceiptError> {
    if receipt.schema != MULTIPROCESS_RECEIPT_SCHEMA {
        return Err(ReceiptError::InvalidSchema);
    }
    if receipt.manifest_identity != manifest.manifest_identity {
        return Err(ReceiptError::ManifestIdentityMismatch);
    }
    let expected_processes = manifest
        .processes
        .iter()
        .map(|process| process.identity.clone())
        .collect::<BTreeSet<_>>();
    let actual_processes = receipt
        .process_identities
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if expected_processes != actual_processes
        || actual_processes.len() != receipt.process_identities.len()
    {
        return Err(ReceiptError::ProcessIdentityMismatch);
    }
    let expected_directories = manifest
        .shared_directories
        .iter()
        .map(shared_directory_identity)
        .collect::<BTreeSet<_>>();
    let actual_directories = receipt
        .shared_directory_identities
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if expected_directories != actual_directories
        || actual_directories.len() != receipt.shared_directory_identities.len()
    {
        return Err(ReceiptError::SharedDirectoryIdentityMismatch);
    }
    if receipt
        .events
        .iter()
        .any(|event| !expected_processes.contains(&event.process_identity))
    {
        return Err(ReceiptError::EventScopeMismatch);
    }
    if receipt.claim_scope != PROCESS_CLAIM_SCOPE {
        return Err(ReceiptError::ClaimOverreach);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESTART_LIMIT: u32 = 2;
    const PAUSE_TICKS: u64 = 2;

    fn manifest() -> ProcessManifest {
        ProcessManifest {
            schema: PROCESS_MANIFEST_SCHEMA.to_string(),
            guest: "storage-guest".to_string(),
            shared_directories: vec![SharedDirectorySpec {
                id: "data".to_string(),
                path: "/data".to_string(),
                device: SharedDeviceKind::Memory,
            }],
            processes: vec![
                ProcessSpec {
                    role: "writer".to_string(),
                    executable: "/bin/writer".to_string(),
                    arguments: vec!["--data".to_string(), "/data".to_string()],
                    environment: BTreeMap::new(),
                    shared_directories: vec!["data".to_string()],
                    restart: RestartPolicy {
                        mode: RestartMode::OnFailure,
                        max_restarts: RESTART_LIMIT,
                    },
                    instrumented: true,
                    transport_slot: Some(0),
                },
                ProcessSpec {
                    role: "checkpoint".to_string(),
                    executable: "/bin/checkpoint".to_string(),
                    arguments: Vec::new(),
                    environment: BTreeMap::new(),
                    shared_directories: vec!["data".to_string()],
                    restart: RestartPolicy {
                        mode: RestartMode::Always,
                        max_restarts: RESTART_LIMIT,
                    },
                    instrumented: true,
                    transport_slot: Some(1),
                },
            ],
        }
    }

    #[test]
    fn cooperating_processes_restart_without_replacing_shared_state() {
        let admitted = admit_manifest(&manifest()).unwrap();
        let started = SupervisorState::new(&admitted).start_all().unwrap();
        assert_eq!(started.effects.len(), admitted.processes.len());
        let shared_before = started.next.shared_directory_identities.clone();
        let writer = &admitted.processes[0].identity;
        let restarted = started.next.observe_exit(&admitted, writer, false).unwrap();
        assert_eq!(
            restarted.next.processes[writer].status,
            ProcessStatus::Running
        );
        assert_eq!(restarted.next.shared_directory_identities, shared_before);
    }

    #[test]
    fn role_faults_are_targeted_and_pause_resumes_at_exact_tick() {
        let admitted = admit_manifest(&manifest()).unwrap();
        let started = SupervisorState::new(&admitted).start_all().unwrap();
        let command = ProcessFaultCommand::new(
            "fault-1",
            "writer",
            ProcessFaultAction::Pause,
            Some(PAUSE_TICKS),
        )
        .unwrap();
        let paused = started.next.apply_fault(&command).unwrap();
        let writer = &admitted.processes[0].identity;
        let checkpoint = &admitted.processes[1].identity;
        assert_eq!(paused.next.processes[writer].status, ProcessStatus::Paused);
        assert_eq!(
            paused.next.processes[checkpoint].status,
            ProcessStatus::Running
        );
        let first = paused.next.advance_tick().unwrap();
        assert_eq!(first.next.processes[writer].status, ProcessStatus::Paused);
        let second = first.next.advance_tick().unwrap();
        assert_eq!(second.next.processes[writer].status, ProcessStatus::Running);
    }

    #[test]
    fn invalid_target_transport_collision_and_state_overclaim_fail_closed() {
        let mut invalid = manifest();
        invalid.processes[1].transport_slot = invalid.processes[0].transport_slot;
        assert_eq!(
            admit_manifest(&invalid),
            Err(ManifestError::DuplicateTransportSlot)
        );

        let admitted = admit_manifest(&manifest()).unwrap();
        let started = SupervisorState::new(&admitted).start_all().unwrap();
        let unknown =
            ProcessFaultCommand::new("fault-1", "missing", ProcessFaultAction::Kill, None).unwrap();
        assert_eq!(
            started.next.apply_fault(&unknown),
            Err(SupervisorError::UnknownProcess)
        );

        let mut receipt = make_receipt(&admitted, started.events);
        validate_receipt(&admitted, &receipt).unwrap();
        receipt.claim_scope = "whole-guest-isolation".to_string();
        assert_eq!(
            validate_receipt(&admitted, &receipt),
            Err(ReceiptError::ClaimOverreach)
        );
    }
}
