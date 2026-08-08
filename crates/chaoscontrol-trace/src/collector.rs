//! Owned eBPF KVM trace collector shell.
//!
//! The worker thread owns every libbpf object. Shutdown quiesces the producer,
//! drains the ring, snapshots producer counters, and then drops links and maps.

use std::fs::File;
use std::io::Read;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use libbpf_rs::skel::{OpenSkel, Skel, SkelBuilder};
use libbpf_rs::{MapCore, MapFlags, RingBufferBuilder};

use crate::events::{EventType, TraceEvent};
use crate::evidence::{
    admit_runtime_cohort, assess_target_binding, parse_raw_record, parse_tracepoint_layout,
    validate_target_observation, AdmissionStatus, AdmittedCaptureProfile, BuildIdentity,
    CleanupOutcome, CleanupStepStatus, ProducerAccounting, RawRecordError, RuntimeAdmission,
    RuntimeCohort, SourceProducerCounters, TargetBindingReport, TargetIdentity, TargetObservation,
    UserspaceAccounting,
};

#[allow(clippy::all)]
#[allow(dead_code)]
mod bpf {
    include!(concat!(env!("OUT_DIR"), "/kvm_trace.skel.rs"));
}

use bpf::*;

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const FINAL_DRAIN_INTERVAL: Duration = Duration::ZERO;
const DEFAULT_MAXIMUM_EVENTS: u64 = 1_000_000;
const MINIMUM_POLLS: u64 = 2;
const DEFAULT_MAXIMUM_POLLS: u64 = 1_000_000;
const START_CHANNEL_CLOSED: &str = "collector worker closed before startup completed";
const STOP_CHANNEL_CLOSED: &str = "collector worker closed before shutdown evidence was returned";
const WORKER_PANIC: &str = "collector worker panicked";
const PRODUCER_COUNTER_BYTES: usize = 3 * std::mem::size_of::<u64>();
const MAXIMUM_EXECUTABLE_BYTES: u64 = 128 * 1_024 * 1_024;
const MAXIMUM_BTF_BYTES: u64 = 128 * 1_024 * 1_024;
const MAXIMUM_PROC_RECORD_BYTES: u64 = 64 * 1_024;
const MAXIMUM_TRACEPOINT_FORMAT_BYTES: u64 = 1_024 * 1_024;
const START_TIME_FIELD_INDEX_AFTER_COMM: usize = 19;
const PROCESS_START_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.ebpf-trace.process-start.v1\0";
const EXECUTABLE_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.ebpf-trace.executable.v1\0";
const CGROUP_IDENTITY_DOMAIN: &[u8] = b"chaoscontrol.ebpf-trace.cgroup.v1\0";
const RUNTIME_BTF_PATH: &str = "/sys/kernel/btf/vmlinux";
const RUNTIME_KERNEL_RELEASE_PATH: &str = "/proc/sys/kernel/osrelease";
const TRACEFS_EVENTS_ROOT: &str = "/sys/kernel/tracing/events";
const EVENT_TYPE_COUNT: usize = 11;
const ALL_EVENT_TYPES: [u32; EVENT_TYPE_COUNT] = [
    EventType::KvmExit as u32,
    EventType::KvmEntry as u32,
    EventType::KvmPio as u32,
    EventType::KvmMmio as u32,
    EventType::KvmMsr as u32,
    EventType::KvmInjVirq as u32,
    EventType::KvmPicIrq as u32,
    EventType::KvmSetIrq as u32,
    EventType::KvmPageFault as u32,
    EventType::KvmCr as u32,
    EventType::KvmCpuid as u32,
];

#[derive(Debug, Default)]
struct CaptureState {
    events: Vec<TraceEvent>,
    userspace: UserspaceAccounting,
}

#[derive(Debug)]
struct WorkerConfig {
    target_pid: u32,
    maximum_events: u64,
    maximum_polls: u64,
    enabled_event_types: Vec<u32>,
}

#[derive(Debug)]
struct WorkerResult {
    producer: ProducerAccounting,
    cleanup: CleanupOutcome,
    error: Option<String>,
}

impl WorkerResult {
    fn startup_failure(error: impl Into<String>) -> Self {
        Self {
            producer: ProducerAccounting {
                available: false,
                sources: Vec::new(),
            },
            cleanup: CleanupOutcome {
                quiesce: CleanupStepStatus::NotAttempted,
                final_poll: CleanupStepStatus::NotAttempted,
                accounting_snapshot: CleanupStepStatus::NotAttempted,
                detach: CleanupStepStatus::NotAttempted,
                unpin: CleanupStepStatus::NotRequired,
                cleanup: CleanupStepStatus::Succeeded,
            },
            error: Some(error.into()),
        }
    }
}

/// Runtime configuration for the bounded collector shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectorBounds {
    pub maximum_events: u64,
    pub maximum_polls: u64,
}

impl Default for CollectorBounds {
    fn default() -> Self {
        Self {
            maximum_events: DEFAULT_MAXIMUM_EVENTS,
            maximum_polls: DEFAULT_MAXIMUM_POLLS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceTargetConfig {
    pub run_id: String,
    pub vmm_profile_ref: String,
}

#[derive(Debug)]
pub struct StableTargetHandle {
    pid: u32,
    pidfd: OwnedFd,
    expected: TargetIdentity,
}

impl StableTargetHandle {
    pub fn open(pid: u32, config: &EvidenceTargetConfig) -> Result<Self> {
        if pid == 0 {
            return Err(anyhow!("evidence target PID must be positive"));
        }
        let raw_fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if raw_fd < 0 {
            return Err(std::io::Error::last_os_error()).context("open evidence target pidfd");
        }
        let raw_fd = i32::try_from(raw_fd).context("pidfd does not fit RawFd")?;
        // SAFETY: `pidfd_open` returned a new owned descriptor on success.
        let pidfd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
        let facts = observe_process_facts(pid)?;
        Ok(Self {
            pid,
            pidfd,
            expected: TargetIdentity {
                run_id: config.run_id.clone(),
                tgid: pid,
                process_start_ref: facts.process_start_ref,
                executable_ref: facts.executable_ref,
                vmm_profile_ref: config.vmm_profile_ref.clone(),
                cgroup_ref: Some(facts.cgroup_ref),
            },
        })
    }

    pub fn expected(&self) -> &TargetIdentity {
        &self.expected
    }

    pub fn observe(&self) -> TargetObservation {
        if self.exited() {
            return self.exited_observation();
        }
        match observe_process_facts(self.pid) {
            Ok(facts) => TargetObservation {
                run_id: self.expected.run_id.clone(),
                tgid: self.pid,
                process_start_ref: facts.process_start_ref,
                exec_changed: facts.executable_ref != self.expected.executable_ref,
                executable_ref: facts.executable_ref,
                vmm_profile_ref: self.expected.vmm_profile_ref.clone(),
                cgroup_ref: Some(facts.cgroup_ref),
                exited: false,
            },
            Err(_) => self.exited_observation(),
        }
    }

    fn exited(&self) -> bool {
        let mut descriptor = libc::pollfd {
            fd: self.pidfd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut descriptor, 1, 0) };
        result > 0 && descriptor.revents & libc::POLLIN != 0
    }

    fn exited_observation(&self) -> TargetObservation {
        TargetObservation {
            run_id: self.expected.run_id.clone(),
            tgid: self.pid,
            process_start_ref: self.expected.process_start_ref.clone(),
            executable_ref: self.expected.executable_ref.clone(),
            vmm_profile_ref: self.expected.vmm_profile_ref.clone(),
            cgroup_ref: self.expected.cgroup_ref.clone(),
            exited: true,
            exec_changed: false,
        }
    }
}

#[derive(Debug)]
struct ProcessFacts {
    process_start_ref: String,
    executable_ref: String,
    cgroup_ref: String,
}

fn observe_process_facts(pid: u32) -> Result<ProcessFacts> {
    let proc_root = format!("/proc/{pid}");
    let stat = read_bounded(&format!("{proc_root}/stat"), MAXIMUM_PROC_RECORD_BYTES)?;
    let stat = String::from_utf8(stat).context("process stat is not UTF-8")?;
    let namespace =
        std::fs::read_link(format!("{proc_root}/ns/pid")).context("read target PID namespace")?;
    let process_start_ref =
        parse_process_start_identity(pid, &stat, namespace.to_string_lossy().as_ref())?;
    let executable = read_bounded(&format!("{proc_root}/exe"), MAXIMUM_EXECUTABLE_BYTES)?;
    let executable_ref = hash_identity(EXECUTABLE_IDENTITY_DOMAIN, &executable);
    let cgroup = read_bounded(&format!("{proc_root}/cgroup"), MAXIMUM_PROC_RECORD_BYTES)?;
    let cgroup_ref = hash_identity(CGROUP_IDENTITY_DOMAIN, &cgroup);
    Ok(ProcessFacts {
        process_start_ref,
        executable_ref,
        cgroup_ref,
    })
}

fn parse_process_start_identity(pid: u32, stat: &str, namespace: &str) -> Result<String> {
    let close_paren = stat
        .rfind(')')
        .ok_or_else(|| anyhow!("process stat lacks command terminator"))?;
    let fields: Vec<_> = stat[close_paren + 1..].split_whitespace().collect();
    let start_time = fields
        .get(START_TIME_FIELD_INDEX_AFTER_COMM)
        .ok_or_else(|| anyhow!("process stat lacks start-time field"))?;
    Ok(hash_identity(
        PROCESS_START_IDENTITY_DOMAIN,
        format!("{pid}\0{start_time}\0{namespace}").as_bytes(),
    ))
}

fn read_bounded(path: &str, maximum_bytes: u64) -> Result<Vec<u8>> {
    let file = File::open(path).with_context(|| format!("open {path}"))?;
    let read_limit = maximum_bytes
        .checked_add(1)
        .ok_or_else(|| anyhow!("read bound overflow"))?;
    let mut bytes = Vec::new();
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {path}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum_bytes {
        return Err(anyhow!("{path} exceeds bounded identity input"));
    }
    Ok(bytes)
}

fn hash_identity(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn raw_blake3_ref(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

pub fn observe_runtime_cohort(profile: &AdmittedCaptureProfile) -> Result<RuntimeCohort> {
    let kernel_release = String::from_utf8(read_bounded(
        RUNTIME_KERNEL_RELEASE_PATH,
        MAXIMUM_PROC_RECORD_BYTES,
    )?)
    .context("kernel release is not UTF-8")?
    .trim()
    .to_string();
    if kernel_release.is_empty() {
        return Err(anyhow!("kernel release is empty"));
    }
    let btf = read_bounded(RUNTIME_BTF_PATH, MAXIMUM_BTF_BYTES)?;
    let mut runtime_layouts = Vec::with_capacity(profile.profile.build.compiled_layouts.len());
    for expected in &profile.profile.build.compiled_layouts {
        let (group, name) = expected
            .tracepoint
            .split_once(':')
            .ok_or_else(|| anyhow!("tracepoint identity lacks group separator"))?;
        let path = format!("{TRACEFS_EVENTS_ROOT}/{group}/{name}/format");
        let format = String::from_utf8(read_bounded(&path, MAXIMUM_TRACEPOINT_FORMAT_BYTES)?)
            .with_context(|| format!("tracepoint format is not UTF-8: {path}"))?;
        runtime_layouts.push(parse_tracepoint_layout(&expected.tracepoint, &format)?);
    }
    Ok(RuntimeCohort {
        kernel_release,
        architecture: std::env::consts::ARCH.to_string(),
        btf_ref: raw_blake3_ref(&btf),
        runtime_layouts,
    })
}

/// Collects KVM tracepoint events for one TGID.
///
/// The BPF skeleton and callbacks never outlive their worker thread. `stop()`
/// is idempotent and retains explicit accounting and cleanup outcomes.
pub struct TraceCollector {
    target_pid: u32,
    bounds: CollectorBounds,
    enabled_event_types: Vec<u32>,
    state: Arc<Mutex<CaptureState>>,
    stop_requested: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    result_rx: Option<mpsc::Receiver<WorkerResult>>,
    producer_accounting: ProducerAccounting,
    cleanup_outcome: CleanupOutcome,
    worker_error: Option<String>,
    target_handle: Option<StableTargetHandle>,
    start_target: Option<TargetObservation>,
    end_target: Option<TargetObservation>,
    admitted_profile: Option<AdmittedCaptureProfile>,
    runtime_cohort: Option<RuntimeCohort>,
    runtime_admission: Option<RuntimeAdmission>,
}

impl std::fmt::Debug for TraceCollector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TraceCollector")
            .field("target_pid", &self.target_pid)
            .field("bounds", &self.bounds)
            .field("enabled_event_types", &self.enabled_event_types)
            .field("running", &self.worker.is_some())
            .field("producer_accounting", &self.producer_accounting)
            .field("cleanup_outcome", &self.cleanup_outcome)
            .field("worker_error", &self.worker_error)
            .field("stable_target", &self.target_handle.is_some())
            .field("start_target", &self.start_target)
            .field("end_target", &self.end_target)
            .field("runtime_admission", &self.runtime_admission)
            .finish_non_exhaustive()
    }
}

impl TraceCollector {
    pub fn new(target_pid: u32) -> Self {
        Self::with_bounds(target_pid, CollectorBounds::default())
    }

    pub fn with_bounds(target_pid: u32, bounds: CollectorBounds) -> Self {
        Self {
            target_pid,
            bounds,
            enabled_event_types: ALL_EVENT_TYPES.to_vec(),
            state: Arc::new(Mutex::new(CaptureState::default())),
            stop_requested: Arc::new(AtomicBool::new(false)),
            worker: None,
            result_rx: None,
            producer_accounting: ProducerAccounting {
                available: false,
                sources: Vec::new(),
            },
            cleanup_outcome: CleanupOutcome {
                quiesce: CleanupStepStatus::NotAttempted,
                final_poll: CleanupStepStatus::NotAttempted,
                accounting_snapshot: CleanupStepStatus::NotAttempted,
                detach: CleanupStepStatus::NotAttempted,
                unpin: CleanupStepStatus::NotRequired,
                cleanup: CleanupStepStatus::NotAttempted,
            },
            worker_error: None,
            target_handle: None,
            start_target: None,
            end_target: None,
            admitted_profile: None,
            runtime_cohort: None,
            runtime_admission: None,
        }
    }

    pub fn for_admitted_profile(profile: AdmittedCaptureProfile) -> Result<Self> {
        let compiled_build =
            BuildIdentity::compiled(profile.profile.build.compiled_layouts.clone());
        if compiled_build != profile.profile.build {
            return Err(anyhow!(
                "admitted profile build identity differs from the compiled loader"
            ));
        }
        let runtime_cohort = observe_runtime_cohort(&profile)?;
        let runtime_admission = admit_runtime_cohort(
            &profile,
            &runtime_cohort,
            &compiled_build.bpf_object_ref,
            &compiled_build.loader_ref,
        )?;
        if runtime_admission.status != AdmissionStatus::Accepted {
            return Err(anyhow!(
                "runtime cohort is not evidence eligible: {:?}",
                runtime_admission.blockers
            ));
        }
        let target_handle = StableTargetHandle::open(
            profile.profile.target.tgid,
            &EvidenceTargetConfig {
                run_id: profile.profile.target.run_id.clone(),
                vmm_profile_ref: profile.profile.target.vmm_profile_ref.clone(),
            },
        )?;
        if target_handle.expected() != &profile.profile.target {
            return Err(anyhow!(
                "observed stable target identity differs from admitted profile"
            ));
        }
        let start_target = target_handle.observe();
        validate_target_observation(target_handle.expected(), &start_target)
            .context("admit stable evidence target")?;
        let mut collector = Self::with_bounds(
            profile.profile.target.tgid,
            CollectorBounds {
                maximum_events: profile.profile.bounds.maximum_events,
                maximum_polls: profile.profile.bounds.maximum_polls,
            },
        );
        collector.target_handle = Some(target_handle);
        collector.start_target = Some(start_target);
        collector.enabled_event_types = profile.profile.enabled_event_types.clone();
        collector.runtime_cohort = Some(runtime_cohort);
        collector.runtime_admission = Some(runtime_admission);
        collector.admitted_profile = Some(profile);
        Ok(collector)
    }

    pub fn with_evidence_target(
        target_pid: u32,
        bounds: CollectorBounds,
        config: EvidenceTargetConfig,
    ) -> Result<Self> {
        let target_handle = StableTargetHandle::open(target_pid, &config)?;
        let start_target = target_handle.observe();
        validate_target_observation(target_handle.expected(), &start_target)
            .context("admit stable evidence target")?;
        let mut collector = Self::with_bounds(target_pid, bounds);
        collector.target_handle = Some(target_handle);
        collector.start_target = Some(start_target);
        Ok(collector)
    }

    /// Load, attach, and start the owned worker.
    pub fn start(&mut self) -> Result<()> {
        if self.worker.is_some() {
            return Err(anyhow!("collector is already running"));
        }
        if self.target_pid == 0 {
            return Err(anyhow!("target PID must be positive"));
        }
        if self.bounds.maximum_events == 0 {
            return Err(anyhow!("maximum_events must be positive"));
        }
        if self.bounds.maximum_polls < MINIMUM_POLLS {
            return Err(anyhow!("maximum_polls must be at least {MINIMUM_POLLS}"));
        }
        if let Some(handle) = &self.target_handle {
            let observed = handle.observe();
            validate_target_observation(handle.expected(), &observed)
                .context("evidence target changed before collector startup")?;
            self.start_target = Some(observed);
            self.end_target = None;
        }

        self.stop_requested.store(false, Ordering::Release);
        self.reset_capture_state()?;
        self.producer_accounting = ProducerAccounting {
            available: false,
            sources: Vec::new(),
        };
        self.cleanup_outcome = CleanupOutcome {
            quiesce: CleanupStepStatus::NotAttempted,
            final_poll: CleanupStepStatus::NotAttempted,
            accounting_snapshot: CleanupStepStatus::NotAttempted,
            detach: CleanupStepStatus::NotAttempted,
            unpin: CleanupStepStatus::NotRequired,
            cleanup: CleanupStepStatus::NotAttempted,
        };
        self.worker_error = None;

        let (start_tx, start_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let state = Arc::clone(&self.state);
        let stop_requested = Arc::clone(&self.stop_requested);
        let worker_config = WorkerConfig {
            target_pid: self.target_pid,
            maximum_events: self.bounds.maximum_events,
            maximum_polls: self.bounds.maximum_polls,
            enabled_event_types: self.enabled_event_types.clone(),
        };
        let worker = thread::Builder::new()
            .name("chaoscontrol-ebpf-trace".to_string())
            .spawn(move || {
                run_worker(worker_config, state, stop_requested, start_tx, result_tx);
            })
            .context("spawn eBPF collector worker")?;

        self.worker = Some(worker);
        self.result_rx = Some(result_rx);
        match start_rx.recv().map_err(|_| anyhow!(START_CHANNEL_CLOSED))? {
            Ok(()) => Ok(()),
            Err(error) => {
                self.finish_worker();
                Err(anyhow!(error))
            }
        }
    }

    /// Request bounded shutdown and retain the terminal accounting facts.
    pub fn stop(&mut self) -> Result<()> {
        if self.worker.is_none() {
            self.observe_end_target();
            return self
                .worker_error
                .as_ref()
                .map_or(Ok(()), |error| Err(anyhow!(error.clone())));
        }
        self.stop_requested.store(true, Ordering::Release);
        self.finish_worker();
        self.observe_end_target();
        self.worker_error
            .as_ref()
            .map_or(Ok(()), |error| Err(anyhow!(error.clone())))
    }

    pub fn events(&self) -> Vec<TraceEvent> {
        match self.state.lock() {
            Ok(state) => state.events.clone(),
            Err(_) => Vec::new(),
        }
    }

    pub fn drain(&self) -> Vec<TraceEvent> {
        match self.state.lock() {
            Ok(mut state) => std::mem::take(&mut state.events),
            Err(_) => Vec::new(),
        }
    }

    pub fn event_count(&self) -> usize {
        match self.state.lock() {
            Ok(state) => state.events.len(),
            Err(_) => 0,
        }
    }

    pub fn userspace_accounting(&self) -> UserspaceAccounting {
        match self.state.lock() {
            Ok(state) => state.userspace.clone(),
            Err(_) => UserspaceAccounting {
                lock_failures: 1,
                ..UserspaceAccounting::default()
            },
        }
    }

    pub fn producer_accounting(&self) -> ProducerAccounting {
        self.producer_accounting.clone()
    }

    pub fn cleanup_outcome(&self) -> CleanupOutcome {
        self.cleanup_outcome.clone()
    }

    pub fn admitted_profile(&self) -> Option<&AdmittedCaptureProfile> {
        self.admitted_profile.as_ref()
    }

    pub fn runtime_cohort(&self) -> Option<&RuntimeCohort> {
        self.runtime_cohort.as_ref()
    }

    pub fn runtime_admission(&self) -> Option<&RuntimeAdmission> {
        self.runtime_admission.as_ref()
    }

    pub fn expected_target(&self) -> Option<&TargetIdentity> {
        self.target_handle
            .as_ref()
            .map(StableTargetHandle::expected)
    }

    pub fn start_target(&self) -> Option<&TargetObservation> {
        self.start_target.as_ref()
    }

    pub fn end_target(&self) -> Option<&TargetObservation> {
        self.end_target.as_ref()
    }

    pub fn target_binding_report(&self) -> Option<TargetBindingReport> {
        let handle = self.target_handle.as_ref()?;
        let start = self.start_target.as_ref()?;
        let end = self.end_target.as_ref()?;
        Some(assess_target_binding(handle.expected(), start, end))
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    fn reset_capture_state(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("capture state lock is poisoned"))?;
        *state = CaptureState::default();
        Ok(())
    }

    fn observe_end_target(&mut self) {
        if let Some(handle) = &self.target_handle {
            self.end_target = Some(handle.observe());
        }
    }

    fn finish_worker(&mut self) {
        let received = self
            .result_rx
            .take()
            .and_then(|receiver| receiver.recv().ok());
        if let Some(result) = received {
            self.producer_accounting = result.producer;
            self.cleanup_outcome = result.cleanup;
            self.worker_error = result.error;
        } else {
            self.worker_error = Some(STOP_CHANNEL_CLOSED.to_string());
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                self.worker_error = Some(WORKER_PANIC.to_string());
            }
        }
    }
}

impl Drop for TraceCollector {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.stop_requested.store(true, Ordering::Release);
            self.finish_worker();
            self.observe_end_target();
        }
    }
}

fn run_worker(
    config: WorkerConfig,
    state: Arc<Mutex<CaptureState>>,
    stop_requested: Arc<AtomicBool>,
    start_tx: mpsc::SyncSender<Result<(), String>>,
    result_tx: mpsc::SyncSender<WorkerResult>,
) {
    match run_attached_worker(config, state, stop_requested, &start_tx) {
        Ok(worker_result) => {
            let _ = result_tx.send(worker_result);
        }
        Err(error) => {
            let message = format!("{error:#}");
            let _ = start_tx.try_send(Err(message.clone()));
            let _ = result_tx.send(WorkerResult::startup_failure(message));
        }
    }
}

fn run_attached_worker(
    config: WorkerConfig,
    state: Arc<Mutex<CaptureState>>,
    stop_requested: Arc<AtomicBool>,
    start_tx: &mpsc::SyncSender<Result<(), String>>,
) -> Result<WorkerResult> {
    let WorkerConfig {
        target_pid,
        maximum_events,
        maximum_polls,
        enabled_event_types,
    } = config;
    let mut open_object = MaybeUninit::uninit();
    let builder = KvmTraceSkelBuilder::default();
    let mut open_skel = builder
        .open(&mut open_object)
        .context("open BPF skeleton")?;
    configure_program_autoload(&mut open_skel, &enabled_event_types);
    let mut skel = open_skel.load().context("load BPF skeleton")?;

    let key = 0_u32.to_ne_bytes();
    skel.maps
        .target_pid
        .update(&key, &target_pid.to_ne_bytes(), MapFlags::ANY)
        .context("set target PID")?;
    for event_type in enabled_event_types {
        skel.maps
            .enabled_event_types
            .update(&event_type.to_ne_bytes(), &[1_u8], MapFlags::ANY)
            .with_context(|| format!("enable event type {event_type}"))?;
    }
    skel.attach().context("attach KVM tracepoints")?;

    let callback_state = Arc::clone(&state);
    let mut ring_builder = RingBufferBuilder::new();
    ring_builder
        .add(&skel.maps.events, move |data| {
            record_callback(&callback_state, data, target_pid, maximum_events);
            0
        })
        .context("register ring-buffer callback")?;
    let ring_buffer = ring_builder.build().context("build ring buffer")?;

    start_tx
        .send(Ok(()))
        .map_err(|_| anyhow!("collector owner closed during startup"))?;

    let regular_poll_limit = maximum_polls.saturating_sub(1);
    let mut completed_polls = 0_u64;
    while !stop_requested.load(Ordering::Acquire) && completed_polls < regular_poll_limit {
        increment_poll(&state);
        completed_polls = completed_polls.saturating_add(1);
        if ring_buffer.poll(POLL_INTERVAL).is_err() {
            increment_poll_failure(&state);
        }
    }
    let poll_bound_reached = !stop_requested.load(Ordering::Acquire);

    let mut cleanup = CleanupOutcome {
        quiesce: CleanupStepStatus::Failed,
        final_poll: CleanupStepStatus::NotAttempted,
        accounting_snapshot: CleanupStepStatus::NotAttempted,
        detach: CleanupStepStatus::NotAttempted,
        unpin: CleanupStepStatus::NotRequired,
        cleanup: CleanupStepStatus::NotAttempted,
    };
    cleanup.quiesce = if skel
        .maps
        .target_pid
        .update(&key, &0_u32.to_ne_bytes(), MapFlags::ANY)
        .is_ok()
    {
        CleanupStepStatus::Succeeded
    } else {
        CleanupStepStatus::Failed
    };

    if let Ok(mut capture) = state.lock() {
        capture.userspace.final_drain_attempted = true;
    }
    increment_poll(&state);
    cleanup.final_poll = if ring_buffer.poll(FINAL_DRAIN_INTERVAL).is_ok() {
        if let Ok(mut capture) = state.lock() {
            capture.userspace.final_drain_succeeded = true;
        }
        CleanupStepStatus::Succeeded
    } else {
        increment_poll_failure(&state);
        CleanupStepStatus::Failed
    };

    let producer = read_producer_accounting(&skel.maps.producer_counters);
    cleanup.accounting_snapshot = if producer.available {
        CleanupStepStatus::Succeeded
    } else {
        CleanupStepStatus::Failed
    };

    drop(ring_buffer);
    drop(skel);
    cleanup.detach = CleanupStepStatus::Succeeded;
    cleanup.cleanup = CleanupStepStatus::Succeeded;

    Ok(WorkerResult {
        producer,
        cleanup,
        error: poll_bound_reached.then(|| "maximum_polls reached before stop".to_string()),
    })
}

fn configure_program_autoload(skel: &mut OpenKvmTraceSkel<'_>, enabled_event_types: &[u32]) {
    let enabled = |event_type: EventType| enabled_event_types.contains(&(event_type as u32));
    skel.progs
        .trace_kvm_exit
        .set_autoload(enabled(EventType::KvmExit));
    skel.progs
        .trace_kvm_entry
        .set_autoload(enabled(EventType::KvmEntry));
    skel.progs
        .trace_kvm_pio
        .set_autoload(enabled(EventType::KvmPio));
    skel.progs
        .trace_kvm_mmio
        .set_autoload(enabled(EventType::KvmMmio));
    skel.progs
        .trace_kvm_msr
        .set_autoload(enabled(EventType::KvmMsr));
    skel.progs
        .trace_kvm_inj_virq
        .set_autoload(enabled(EventType::KvmInjVirq));
    skel.progs
        .trace_kvm_pic_set_irq
        .set_autoload(enabled(EventType::KvmPicIrq));
    skel.progs
        .trace_kvm_set_irq
        .set_autoload(enabled(EventType::KvmSetIrq));
    skel.progs
        .trace_kvm_page_fault
        .set_autoload(enabled(EventType::KvmPageFault));
    skel.progs
        .trace_kvm_cr
        .set_autoload(enabled(EventType::KvmCr));
    skel.progs
        .trace_kvm_cpuid
        .set_autoload(enabled(EventType::KvmCpuid));
}

fn record_callback(
    state: &Arc<Mutex<CaptureState>>,
    data: &[u8],
    target_pid: u32,
    maximum_events: u64,
) {
    let mut capture = match state.lock() {
        Ok(capture) => capture,
        Err(poisoned) => {
            let mut capture = poisoned.into_inner();
            capture.userspace.lock_failures = capture.userspace.lock_failures.saturating_add(1);
            capture
        }
    };
    capture.userspace.received_records = capture.userspace.received_records.saturating_add(1);
    let capture_index = capture.userspace.accepted_records;
    match parse_raw_record(data, capture_index, target_pid, maximum_events) {
        Ok(event) => {
            capture.events.push(event);
            capture.userspace.accepted_records =
                capture.userspace.accepted_records.saturating_add(1);
        }
        Err(error) => increment_raw_error(&mut capture.userspace, &error),
    }
}

fn increment_raw_error(accounting: &mut UserspaceAccounting, error: &RawRecordError) {
    let counter = match error {
        RawRecordError::Size | RawRecordError::RecordSize => &mut accounting.malformed_size,
        RawRecordError::Version => &mut accounting.wrong_version,
        RawRecordError::UnknownDiscriminant => &mut accounting.unknown_discriminant,
        RawRecordError::Target => &mut accounting.parse_failed,
        RawRecordError::EventBound => &mut accounting.over_bound_drops,
    };
    *counter = counter.saturating_add(1);
}

fn increment_poll(state: &Arc<Mutex<CaptureState>>) {
    let mut capture = match state.lock() {
        Ok(capture) => capture,
        Err(poisoned) => {
            let mut capture = poisoned.into_inner();
            capture.userspace.lock_failures = capture.userspace.lock_failures.saturating_add(1);
            capture
        }
    };
    capture.userspace.polls = capture.userspace.polls.saturating_add(1);
}

fn increment_poll_failure(state: &Arc<Mutex<CaptureState>>) {
    let mut capture = match state.lock() {
        Ok(capture) => capture,
        Err(poisoned) => {
            let mut capture = poisoned.into_inner();
            capture.userspace.lock_failures = capture.userspace.lock_failures.saturating_add(1);
            capture
        }
    };
    capture.userspace.poll_failures = capture.userspace.poll_failures.saturating_add(1);
}

fn read_producer_accounting(map: &impl MapCore) -> ProducerAccounting {
    let key = 0_u32.to_ne_bytes();
    let Ok(Some(values)) = map.lookup_percpu(&key, MapFlags::ANY) else {
        return ProducerAccounting {
            available: false,
            sources: Vec::new(),
        };
    };
    let mut sources = Vec::new();
    for (source_cpu, value) in values.iter().enumerate() {
        if value.len() < PRODUCER_COUNTER_BYTES {
            return ProducerAccounting {
                available: false,
                sources: Vec::new(),
            };
        }
        let eligible_attempts = read_counter(value, 0);
        let submitted_records = read_counter(value, std::mem::size_of::<u64>());
        let reservation_drops = read_counter(value, 2 * std::mem::size_of::<u64>());
        if eligible_attempts != 0 || submitted_records != 0 || reservation_drops != 0 {
            let Ok(source_cpu) = u32::try_from(source_cpu) else {
                return ProducerAccounting {
                    available: false,
                    sources: Vec::new(),
                };
            };
            sources.push(SourceProducerCounters {
                source_cpu,
                eligible_attempts,
                submitted_records,
                reservation_drops,
            });
        }
    }
    ProducerAccounting {
        available: true,
        sources,
    }
}

fn read_counter(value: &[u8], offset: usize) -> u64 {
    let mut bytes = [0_u8; std::mem::size_of::<u64>()];
    bytes.copy_from_slice(&value[offset..offset + std::mem::size_of::<u64>()]);
    u64::from_ne_bytes(bytes)
}

/// Compatibility configuration for the interactive debug collector.
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    pub target_pid: u32,
    pub poll_timeout: Duration,
}

impl CollectorConfig {
    pub fn current_process() -> Self {
        Self {
            target_pid: std::process::id(),
            poll_timeout: POLL_INTERVAL,
        }
    }

    pub fn for_pid(pid: u32) -> Self {
        Self {
            target_pid: pid,
            poll_timeout: POLL_INTERVAL,
        }
    }
}

/// Compatibility shell for the existing interactive CLI.
///
/// This API emits debug traces. Evidence-capable callers must also retain the
/// typed profile, admission, accounting, target, and cleanup records.
pub struct Collector {
    inner: TraceCollector,
    config: CollectorConfig,
    total_events: u64,
    poll_count: u64,
}

impl Collector {
    pub fn attach(config: CollectorConfig) -> Result<Self> {
        if unsafe { libc::geteuid() } != 0 {
            return Err(anyhow!("root capability is required for BPF attachment"));
        }
        let mut inner = TraceCollector::new(config.target_pid);
        inner.start()?;
        Ok(Self {
            inner,
            config,
            total_events: 0,
            poll_count: 0,
        })
    }

    pub fn poll(&mut self) -> Result<usize> {
        let before = self.inner.userspace_accounting().accepted_records;
        thread::sleep(self.config.poll_timeout);
        let after = self.inner.userspace_accounting().accepted_records;
        let new_events = after.saturating_sub(before);
        self.total_events = self.total_events.saturating_add(new_events);
        self.poll_count = self.poll_count.saturating_add(1);
        usize::try_from(new_events).map_err(|_| anyhow!("new event count exceeds usize"))
    }

    pub fn stop(&mut self) -> Result<()> {
        self.inner.stop()
    }

    pub fn userspace_accounting(&self) -> UserspaceAccounting {
        self.inner.userspace_accounting()
    }

    pub fn producer_accounting(&self) -> ProducerAccounting {
        self.inner.producer_accounting()
    }

    pub fn cleanup_outcome(&self) -> CleanupOutcome {
        self.inner.cleanup_outcome()
    }

    pub fn drain(&mut self) -> Vec<TraceEvent> {
        self.inner.drain()
    }

    pub fn peek(&self) -> Vec<TraceEvent> {
        self.inner.events()
    }

    pub fn total_events(&self) -> u64 {
        self.total_events
    }

    pub fn poll_count(&self) -> u64 {
        self.poll_count
    }

    pub fn target_pid(&self) -> u32 {
        self.config.target_pid
    }
}

/// Legacy debug trace. It is not an evidence manifest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceLog {
    pub pid: u32,
    pub events: Vec<TraceEvent>,
    pub metadata: TraceMetadata,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraceMetadata {
    pub kernel_version: String,
    pub cpu_model: String,
    pub start_time: String,
}

impl TraceMetadata {
    pub fn gather() -> Self {
        let kernel_version = std::fs::read_to_string("/proc/version")
            .unwrap_or_default()
            .trim()
            .to_string();
        let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .find(|line| line.starts_with("model name"))
            .unwrap_or("unknown")
            .to_string();
        let mut timestamp = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            libc::clock_gettime(libc::CLOCK_REALTIME, &mut timestamp);
        }
        Self {
            kernel_version,
            cpu_model,
            start_time: format!("{}s_{}ns", timestamp.tv_sec, timestamp.tv_nsec),
        }
    }
}

impl TraceLog {
    pub fn new(pid: u32, events: Vec<TraceEvent>) -> Self {
        Self {
            pid,
            events,
            metadata: TraceMetadata::gather(),
        }
    }

    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    pub fn load(path: &str) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(std::io::Error::other)
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn summary(&self) -> std::collections::BTreeMap<String, usize> {
        let mut counts = std::collections::BTreeMap::new();
        for event in &self.events {
            *counts
                .entry(event.event_type().name().to_string())
                .or_insert(0) += 1;
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventType, RAW_EVENT_SCHEMA_VERSION, RAW_EVENT_SIZE};

    const TEST_PID: u32 = 73;
    const TEST_SOURCE: u32 = 4;
    const SMALL_EVENT_BOUND: u64 = 1;
    const CALLBACK_EVENT_BOUND: u64 = 2;
    const TEST_PROFILE_REF: &str =
        "blake3:0000000000000000000000000000000000000000000000000000000000000000";

    fn raw_record(event_type: u32) -> Vec<u8> {
        let mut bytes = vec![0_u8; usize::from(RAW_EVENT_SIZE)];
        bytes[16..20].copy_from_slice(&event_type.to_ne_bytes());
        bytes[20..24].copy_from_slice(&TEST_PID.to_ne_bytes());
        bytes[24..28].copy_from_slice(&TEST_SOURCE.to_ne_bytes());
        bytes[28..30].copy_from_slice(&RAW_EVENT_SCHEMA_VERSION.to_ne_bytes());
        bytes[30..32].copy_from_slice(&RAW_EVENT_SIZE.to_ne_bytes());
        bytes
    }

    #[test]
    fn callback_accepts_valid_records_and_counts_unknown_records() {
        let state = Arc::new(Mutex::new(CaptureState::default()));
        record_callback(
            &state,
            &raw_record(EventType::KvmExit as u32),
            TEST_PID,
            CALLBACK_EVENT_BOUND,
        );
        record_callback(
            &state,
            &raw_record(u32::MAX),
            TEST_PID,
            CALLBACK_EVENT_BOUND,
        );
        let capture = state.lock().expect("capture");
        assert_eq!(capture.events.len(), 1);
        assert_eq!(capture.userspace.received_records, 2);
        assert_eq!(capture.userspace.accepted_records, 1);
        assert_eq!(capture.userspace.unknown_discriminant, 1);
    }

    #[test]
    fn callback_enforces_event_bound_without_panicking() {
        let state = Arc::new(Mutex::new(CaptureState::default()));
        let record = raw_record(EventType::KvmExit as u32);
        record_callback(&state, &record, TEST_PID, SMALL_EVENT_BOUND);
        record_callback(&state, &record, TEST_PID, SMALL_EVENT_BOUND);
        let capture = state.lock().expect("capture");
        assert_eq!(capture.events.len(), 1);
        assert_eq!(capture.userspace.over_bound_drops, 1);
    }

    #[test]
    fn profile_event_filter_controls_program_autoload_before_kernel_effects() {
        let mut open_object = MaybeUninit::uninit();
        let mut skel = KvmTraceSkelBuilder::default()
            .open(&mut open_object)
            .expect("open BPF object");
        configure_program_autoload(&mut skel, &[EventType::KvmExit as u32]);
        assert!(skel.progs.trace_kvm_exit.autoload());
        assert!(!skel.progs.trace_kvm_entry.autoload());
        assert!(!skel.progs.trace_kvm_set_irq.autoload());
    }

    #[test]
    fn invalid_collector_configuration_fails_before_privileged_effects() {
        let mut zero_pid = TraceCollector::new(0);
        assert!(zero_pid.start().is_err());
        let mut zero_event_bound = TraceCollector::with_bounds(
            TEST_PID,
            CollectorBounds {
                maximum_events: 0,
                maximum_polls: DEFAULT_MAXIMUM_POLLS,
            },
        );
        assert!(zero_event_bound.start().is_err());
        let mut zero_poll_bound = TraceCollector::with_bounds(
            TEST_PID,
            CollectorBounds {
                maximum_events: DEFAULT_MAXIMUM_EVENTS,
                maximum_polls: 0,
            },
        );
        assert!(zero_poll_bound.start().is_err());
    }

    #[test]
    fn process_start_identity_parser_handles_command_spaces_and_malformed_stat() {
        let mut fields = vec!["field"; START_TIME_FIELD_INDEX_AFTER_COMM + 1];
        fields[START_TIME_FIELD_INDEX_AFTER_COMM] = "start-time";
        let stat = format!("{TEST_PID} (command with spaces) {}", fields.join(" "));
        let first = parse_process_start_identity(TEST_PID, &stat, "pid:[test]")
            .expect("process start identity");
        let second = parse_process_start_identity(TEST_PID, &stat, "pid:[test]")
            .expect("repeat process start identity");
        assert_eq!(first, second);
        assert!(parse_process_start_identity(TEST_PID, "malformed", "pid:[test]").is_err());
    }

    #[test]
    fn evidence_collector_rejects_profile_before_effects_when_build_identity_drifts() {
        let profile: crate::evidence::CaptureProfile = serde_json::from_str(include_str!(
            "../../../contracts/evidence/fixtures/valid/ebpf-trace-capture-profile.valid.json"
        ))
        .expect("profile fixture");
        let admitted = crate::evidence::admit_capture_profile(&profile).expect("admitted profile");
        let error = TraceCollector::for_admitted_profile(admitted)
            .expect_err("fixture build identity must differ from the current compiled object");
        assert!(error.to_string().contains("compiled loader"));
    }

    #[test]
    fn stable_target_handle_binds_start_executable_and_cgroup_facts() {
        let handle = StableTargetHandle::open(
            std::process::id(),
            &EvidenceTargetConfig {
                run_id: "collector-target-test".to_string(),
                vmm_profile_ref: TEST_PROFILE_REF.to_string(),
            },
        )
        .expect("stable target handle");
        let observed = handle.observe();
        validate_target_observation(handle.expected(), &observed).expect("stable target");
        assert!(handle.expected().cgroup_ref.is_some());
        assert!(!observed.exited);
        assert!(!observed.exec_changed);
        assert!(StableTargetHandle::open(
            0,
            &EvidenceTargetConfig {
                run_id: "invalid".to_string(),
                vmm_profile_ref: TEST_PROFILE_REF.to_string(),
            },
        )
        .is_err());
    }

    #[test]
    fn stop_is_idempotent_before_start() {
        let mut collector = TraceCollector::new(TEST_PID);
        collector.stop().expect("first stop");
        collector.stop().expect("second stop");
        assert!(!collector.is_running());
    }

    #[test]
    fn source_has_no_intentional_lifetime_leak() {
        let source = include_str!("collector.rs");
        assert!(!source.contains(&["Box", "::", "leak"].concat()));
        assert!(!source.contains(&["mem", "::", "transmute"].concat()));
    }
}
