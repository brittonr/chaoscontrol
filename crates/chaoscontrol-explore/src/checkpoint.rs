//! Checkpoint save/load for resumable exploration campaigns.
//!
//! Saves the minimal state needed to resume exploration:
//! - Global coverage bitmap
//! - Bugs found so far
//! - Progress counters
//! - Configuration
//!
//! The frontier is NOT saved because it contains VM snapshots which are
//! complex to serialize. Instead, on resume we re-bootstrap and rebuild
//! the frontier, but carry forward the global coverage so we don't
//! re-explore known territory.

use crate::snapshot_store::SnapshotStore;

use snafu::Snafu;

use std::io::Write;

const MAX_CHECKPOINT_BUGS: usize = 4_096;
const MAX_SERIALIZABLE_FAULTS: usize = 4_096;
const MAX_CHECKPOINT_ROUND_HISTORY: usize = 1_000_000;
const MAX_CHECKPOINT_DEDUP_KEYS: usize = 65_536;
const DEFAULT_MEMORY_PRESSURE_DURATION_TICKS: u64 = 1;

const fn default_memory_pressure_duration_ticks() -> u64 {
    DEFAULT_MEMORY_PRESSURE_DURATION_TICKS
}

/// Errors from checkpoint operations.
#[derive(Debug, Snafu)]
pub enum CheckpointError {
    #[snafu(display("I/O error"), context(false))]
    Io { source: std::io::Error },

    #[snafu(display("JSON error"), context(false))]
    Json { source: serde_json::Error },

    #[snafu(display("snapshot store error"), context(false))]
    SnapshotStore {
        source: crate::snapshot_store::SnapshotStoreError,
    },

    #[snafu(display("checkpoint contains invalid bug identity"), context(false))]
    InvalidBugIdentity { source: BugSetIdentityError },

    #[snafu(display("checkpoint assertion report is invalid: {reason}"))]
    InvalidAssertionReport { reason: String },

    #[snafu(display("checkpoint bounds are invalid: {reason}"))]
    InvalidBounds { reason: String },

    #[snafu(display(
        "bug {bug_id} requires replay parent snapshot depth {replay_parent_depth}, but no parent snapshot is available"
    ))]
    MissingRequiredReplayParentSnapshot {
        bug_id: u64,
        replay_parent_depth: u32,
    },
}

/// Configuration subset needed to resume exploration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckpointConfig {
    pub num_vms: usize,
    pub kernel_path: String,
    pub initrd_path: Option<String>,
    pub seed: u64,
    pub branch_factor: usize,
    pub ticks_per_branch: u64,
    pub max_rounds: u64,
    pub max_frontier: usize,
    pub quantum: u64,
    pub coverage_gpa: u64,
    /// Optional disk image path (defaults to None for backward compat).
    #[serde(default)]
    pub disk_image_path: Option<String>,
    /// Bootstrap tick budget (defaults to 10000 for backward compat).
    #[serde(default = "default_bootstrap_budget")]
    pub bootstrap_budget: u64,
    /// Schedule diversity enabled.
    #[serde(default)]
    pub schedule_diversity: bool,
    /// Schedule mutation ratio (0.0 = disabled).
    #[serde(default)]
    pub schedule_mutation_ratio: f64,
    /// Rare-edge threshold for frontier scoring.
    #[serde(default)]
    pub rare_edge_threshold: Option<u8>,
    /// Rare-edge score multiplier.
    #[serde(default)]
    pub rare_edge_weight: Option<f64>,
    /// Stale rounds before havoc activates (0 = auto from stale_round_limit/2).
    #[serde(default)]
    pub havoc_after_stale: Option<u64>,
    /// Havoc mutation count range [min, max].
    #[serde(default)]
    pub havoc_mutations: Option<[u32; 2]>,
    /// Helical scenario config (if the run used one).
    #[serde(default)]
    pub scenario: Option<chaoscontrol_fault::scenario::ScenarioConfig>,
}

fn default_bootstrap_budget() -> u64 {
    10_000
}

/// Serializable fault representation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SerializableFault {
    NetworkPartition {
        side_a: Vec<usize>,
        side_b: Vec<usize>,
    },
    NetworkLatency {
        target: usize,
        latency_ns: u64,
    },
    PacketLoss {
        target: usize,
        rate_ppm: u32,
    },
    PacketCorruption {
        target: usize,
        rate_ppm: u32,
    },
    PacketReorder {
        target: usize,
        window_ns: u64,
    },
    NetworkJitter {
        target: usize,
        jitter_ns: u64,
    },
    NetworkBandwidth {
        target: usize,
        bytes_per_sec: u64,
    },
    PacketDuplicate {
        target: usize,
        rate_ppm: u32,
    },
    NetworkHeal,
    DiskReadError {
        target: usize,
        offset: u64,
    },
    DiskWriteError {
        target: usize,
        offset: u64,
    },
    DiskTornWrite {
        target: usize,
        offset: u64,
        bytes_written: usize,
    },
    DiskCorruption {
        target: usize,
        offset: u64,
        len: usize,
    },
    DiskFull {
        target: usize,
    },
    ProcessKill {
        target: usize,
    },
    ProcessRestart {
        target: usize,
    },
    ProcessPause {
        target: usize,
        duration_ns: u64,
    },
    ClockSkew {
        target: usize,
        offset_ns: i64,
    },
    ClockJump {
        target: usize,
        delta_ns: i64,
    },
    MemoryPressure {
        target: usize,
        limit_bytes: u64,
        #[serde(default = "default_memory_pressure_duration_ticks")]
        duration_ticks: u64,
    },
    InjectInterrupt {
        target: usize,
        irq: u32,
    },
    InjectNmi {
        target: usize,
        vcpu: usize,
    },
    DiskSlow {
        target: usize,
        delay_ns: u64,
    },
    DiskFsyncLie {
        target: usize,
    },
    DiskFsyncFlush {
        target: usize,
    },
    DiskPartialRead {
        target: usize,
        offset: u64,
        max_bytes: usize,
    },
    CpuBitflip {
        target: usize,
        vcpu: usize,
        register: ::chaoscontrol_fault::faults::GpRegister,
        bit: u8,
    },
    CpuStall {
        target: usize,
        vcpu: usize,
        duration_ticks: u64,
    },
    ClockFreeze {
        target: usize,
        duration_ticks: u64,
    },
    ClockJitter {
        target: usize,
        bound_tsc: u64,
    },
}

impl From<&::chaoscontrol_fault::faults::Fault> for SerializableFault {
    fn from(fault: &::chaoscontrol_fault::faults::Fault) -> Self {
        match fault {
            ::chaoscontrol_fault::faults::Fault::NetworkPartition { side_a, side_b } => {
                SerializableFault::NetworkPartition {
                    side_a: side_a.clone(),
                    side_b: side_b.clone(),
                }
            }
            ::chaoscontrol_fault::faults::Fault::NetworkLatency { target, latency_ns } => {
                SerializableFault::NetworkLatency {
                    target: *target,
                    latency_ns: *latency_ns,
                }
            }
            ::chaoscontrol_fault::faults::Fault::PacketLoss { target, rate_ppm } => {
                SerializableFault::PacketLoss {
                    target: *target,
                    rate_ppm: *rate_ppm,
                }
            }
            ::chaoscontrol_fault::faults::Fault::PacketCorruption { target, rate_ppm } => {
                SerializableFault::PacketCorruption {
                    target: *target,
                    rate_ppm: *rate_ppm,
                }
            }
            ::chaoscontrol_fault::faults::Fault::PacketReorder { target, window_ns } => {
                SerializableFault::PacketReorder {
                    target: *target,
                    window_ns: *window_ns,
                }
            }
            ::chaoscontrol_fault::faults::Fault::NetworkJitter { target, jitter_ns } => {
                SerializableFault::NetworkJitter {
                    target: *target,
                    jitter_ns: *jitter_ns,
                }
            }
            ::chaoscontrol_fault::faults::Fault::NetworkBandwidth {
                target,
                bytes_per_sec,
            } => SerializableFault::NetworkBandwidth {
                target: *target,
                bytes_per_sec: *bytes_per_sec,
            },
            ::chaoscontrol_fault::faults::Fault::PacketDuplicate { target, rate_ppm } => {
                SerializableFault::PacketDuplicate {
                    target: *target,
                    rate_ppm: *rate_ppm,
                }
            }
            ::chaoscontrol_fault::faults::Fault::NetworkHeal => SerializableFault::NetworkHeal,
            ::chaoscontrol_fault::faults::Fault::DiskReadError { target, offset } => {
                SerializableFault::DiskReadError {
                    target: *target,
                    offset: *offset,
                }
            }
            ::chaoscontrol_fault::faults::Fault::DiskWriteError { target, offset } => {
                SerializableFault::DiskWriteError {
                    target: *target,
                    offset: *offset,
                }
            }
            ::chaoscontrol_fault::faults::Fault::DiskTornWrite {
                target,
                offset,
                bytes_written,
            } => SerializableFault::DiskTornWrite {
                target: *target,
                offset: *offset,
                bytes_written: *bytes_written,
            },
            ::chaoscontrol_fault::faults::Fault::DiskCorruption {
                target,
                offset,
                len,
            } => SerializableFault::DiskCorruption {
                target: *target,
                offset: *offset,
                len: *len,
            },
            ::chaoscontrol_fault::faults::Fault::DiskFull { target } => {
                SerializableFault::DiskFull { target: *target }
            }
            ::chaoscontrol_fault::faults::Fault::ProcessKill { target } => {
                SerializableFault::ProcessKill { target: *target }
            }
            ::chaoscontrol_fault::faults::Fault::ProcessRestart { target } => {
                SerializableFault::ProcessRestart { target: *target }
            }
            ::chaoscontrol_fault::faults::Fault::ProcessPause {
                target,
                duration_ns,
            } => SerializableFault::ProcessPause {
                target: *target,
                duration_ns: *duration_ns,
            },
            ::chaoscontrol_fault::faults::Fault::ClockSkew { target, offset_ns } => {
                SerializableFault::ClockSkew {
                    target: *target,
                    offset_ns: *offset_ns,
                }
            }
            ::chaoscontrol_fault::faults::Fault::ClockJump { target, delta_ns } => {
                SerializableFault::ClockJump {
                    target: *target,
                    delta_ns: *delta_ns,
                }
            }
            ::chaoscontrol_fault::faults::Fault::MemoryPressure {
                target,
                limit_bytes,
                duration_ticks,
            } => SerializableFault::MemoryPressure {
                target: *target,
                limit_bytes: *limit_bytes,
                duration_ticks: *duration_ticks,
            },
            ::chaoscontrol_fault::faults::Fault::InjectInterrupt { target, irq } => {
                SerializableFault::InjectInterrupt {
                    target: *target,
                    irq: *irq,
                }
            }
            ::chaoscontrol_fault::faults::Fault::InjectNmi { target, vcpu } => {
                SerializableFault::InjectNmi {
                    target: *target,
                    vcpu: *vcpu,
                }
            }
            ::chaoscontrol_fault::faults::Fault::DiskSlow { target, delay_ns } => {
                SerializableFault::DiskSlow {
                    target: *target,
                    delay_ns: *delay_ns,
                }
            }
            ::chaoscontrol_fault::faults::Fault::DiskFsyncLie { target } => {
                SerializableFault::DiskFsyncLie { target: *target }
            }
            ::chaoscontrol_fault::faults::Fault::DiskFsyncFlush { target } => {
                SerializableFault::DiskFsyncFlush { target: *target }
            }
            ::chaoscontrol_fault::faults::Fault::DiskPartialRead {
                target,
                offset,
                max_bytes,
            } => SerializableFault::DiskPartialRead {
                target: *target,
                offset: *offset,
                max_bytes: *max_bytes,
            },
            ::chaoscontrol_fault::faults::Fault::CpuBitflip {
                target,
                vcpu,
                register,
                bit,
            } => SerializableFault::CpuBitflip {
                target: *target,
                vcpu: *vcpu,
                register: *register,
                bit: *bit,
            },
            ::chaoscontrol_fault::faults::Fault::CpuStall {
                target,
                vcpu,
                duration_ticks,
            } => SerializableFault::CpuStall {
                target: *target,
                vcpu: *vcpu,
                duration_ticks: *duration_ticks,
            },
            ::chaoscontrol_fault::faults::Fault::ClockFreeze {
                target,
                duration_ticks,
            } => SerializableFault::ClockFreeze {
                target: *target,
                duration_ticks: *duration_ticks,
            },
            ::chaoscontrol_fault::faults::Fault::ClockJitter { target, bound_tsc } => {
                SerializableFault::ClockJitter {
                    target: *target,
                    bound_tsc: *bound_tsc,
                }
            }
        }
    }
}

impl From<&SerializableFault> for ::chaoscontrol_fault::faults::Fault {
    fn from(fault: &SerializableFault) -> Self {
        match fault {
            SerializableFault::NetworkPartition { side_a, side_b } => {
                ::chaoscontrol_fault::faults::Fault::NetworkPartition {
                    side_a: side_a.clone(),
                    side_b: side_b.clone(),
                }
            }
            SerializableFault::NetworkLatency { target, latency_ns } => {
                ::chaoscontrol_fault::faults::Fault::NetworkLatency {
                    target: *target,
                    latency_ns: *latency_ns,
                }
            }
            SerializableFault::PacketLoss { target, rate_ppm } => {
                ::chaoscontrol_fault::faults::Fault::PacketLoss {
                    target: *target,
                    rate_ppm: *rate_ppm,
                }
            }
            SerializableFault::PacketCorruption { target, rate_ppm } => {
                ::chaoscontrol_fault::faults::Fault::PacketCorruption {
                    target: *target,
                    rate_ppm: *rate_ppm,
                }
            }
            SerializableFault::PacketReorder { target, window_ns } => {
                ::chaoscontrol_fault::faults::Fault::PacketReorder {
                    target: *target,
                    window_ns: *window_ns,
                }
            }
            SerializableFault::NetworkJitter { target, jitter_ns } => {
                ::chaoscontrol_fault::faults::Fault::NetworkJitter {
                    target: *target,
                    jitter_ns: *jitter_ns,
                }
            }
            SerializableFault::NetworkBandwidth {
                target,
                bytes_per_sec,
            } => ::chaoscontrol_fault::faults::Fault::NetworkBandwidth {
                target: *target,
                bytes_per_sec: *bytes_per_sec,
            },
            SerializableFault::PacketDuplicate { target, rate_ppm } => {
                ::chaoscontrol_fault::faults::Fault::PacketDuplicate {
                    target: *target,
                    rate_ppm: *rate_ppm,
                }
            }
            SerializableFault::NetworkHeal => ::chaoscontrol_fault::faults::Fault::NetworkHeal,
            SerializableFault::DiskReadError { target, offset } => {
                ::chaoscontrol_fault::faults::Fault::DiskReadError {
                    target: *target,
                    offset: *offset,
                }
            }
            SerializableFault::DiskWriteError { target, offset } => {
                ::chaoscontrol_fault::faults::Fault::DiskWriteError {
                    target: *target,
                    offset: *offset,
                }
            }
            SerializableFault::DiskTornWrite {
                target,
                offset,
                bytes_written,
            } => ::chaoscontrol_fault::faults::Fault::DiskTornWrite {
                target: *target,
                offset: *offset,
                bytes_written: *bytes_written,
            },
            SerializableFault::DiskCorruption {
                target,
                offset,
                len,
            } => ::chaoscontrol_fault::faults::Fault::DiskCorruption {
                target: *target,
                offset: *offset,
                len: *len,
            },
            SerializableFault::DiskFull { target } => {
                ::chaoscontrol_fault::faults::Fault::DiskFull { target: *target }
            }
            SerializableFault::ProcessKill { target } => {
                ::chaoscontrol_fault::faults::Fault::ProcessKill { target: *target }
            }
            SerializableFault::ProcessRestart { target } => {
                ::chaoscontrol_fault::faults::Fault::ProcessRestart { target: *target }
            }
            SerializableFault::ProcessPause {
                target,
                duration_ns,
            } => ::chaoscontrol_fault::faults::Fault::ProcessPause {
                target: *target,
                duration_ns: *duration_ns,
            },
            SerializableFault::ClockSkew { target, offset_ns } => {
                ::chaoscontrol_fault::faults::Fault::ClockSkew {
                    target: *target,
                    offset_ns: *offset_ns,
                }
            }
            SerializableFault::ClockJump { target, delta_ns } => {
                ::chaoscontrol_fault::faults::Fault::ClockJump {
                    target: *target,
                    delta_ns: *delta_ns,
                }
            }
            SerializableFault::MemoryPressure {
                target,
                limit_bytes,
                duration_ticks,
            } => ::chaoscontrol_fault::faults::Fault::MemoryPressure {
                target: *target,
                limit_bytes: *limit_bytes,
                duration_ticks: *duration_ticks,
            },
            SerializableFault::InjectInterrupt { target, irq } => {
                ::chaoscontrol_fault::faults::Fault::InjectInterrupt {
                    target: *target,
                    irq: *irq,
                }
            }
            SerializableFault::InjectNmi { target, vcpu } => {
                ::chaoscontrol_fault::faults::Fault::InjectNmi {
                    target: *target,
                    vcpu: *vcpu,
                }
            }
            SerializableFault::DiskSlow { target, delay_ns } => {
                ::chaoscontrol_fault::faults::Fault::DiskSlow {
                    target: *target,
                    delay_ns: *delay_ns,
                }
            }
            SerializableFault::DiskFsyncLie { target } => {
                ::chaoscontrol_fault::faults::Fault::DiskFsyncLie { target: *target }
            }
            SerializableFault::DiskFsyncFlush { target } => {
                ::chaoscontrol_fault::faults::Fault::DiskFsyncFlush { target: *target }
            }
            SerializableFault::DiskPartialRead {
                target,
                offset,
                max_bytes,
            } => ::chaoscontrol_fault::faults::Fault::DiskPartialRead {
                target: *target,
                offset: *offset,
                max_bytes: *max_bytes,
            },
            SerializableFault::CpuBitflip {
                target,
                vcpu,
                register,
                bit,
            } => ::chaoscontrol_fault::faults::Fault::CpuBitflip {
                target: *target,
                vcpu: *vcpu,
                register: *register,
                bit: *bit,
            },
            SerializableFault::CpuStall {
                target,
                vcpu,
                duration_ticks,
            } => ::chaoscontrol_fault::faults::Fault::CpuStall {
                target: *target,
                vcpu: *vcpu,
                duration_ticks: *duration_ticks,
            },
            SerializableFault::ClockFreeze {
                target,
                duration_ticks,
            } => ::chaoscontrol_fault::faults::Fault::ClockFreeze {
                target: *target,
                duration_ticks: *duration_ticks,
            },
            SerializableFault::ClockJitter { target, bound_tsc } => {
                ::chaoscontrol_fault::faults::Fault::ClockJitter {
                    target: *target,
                    bound_tsc: *bound_tsc,
                }
            }
        }
    }
}

/// Serializable scheduled fault.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerializableScheduledFault {
    pub time_ns: u64,
    pub fault: SerializableFault,
    pub label: Option<String>,
}

/// Serializable fault schedule.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerializableSchedule {
    pub faults: Vec<SerializableScheduledFault>,
}

impl From<&::chaoscontrol_fault::schedule::FaultSchedule> for SerializableSchedule {
    fn from(schedule: &::chaoscontrol_fault::schedule::FaultSchedule) -> Self {
        let faults = schedule
            .faults()
            .iter()
            .map(|sf| SerializableScheduledFault {
                time_ns: sf.time_ns,
                fault: (&sf.fault).into(),
                label: sf.label.clone(),
            })
            .collect();

        SerializableSchedule { faults }
    }
}

impl From<&SerializableSchedule> for ::chaoscontrol_fault::schedule::FaultSchedule {
    fn from(sched: &SerializableSchedule) -> Self {
        let mut schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        for sf in &sched.faults {
            let mut fault =
                ::chaoscontrol_fault::schedule::ScheduledFault::new(sf.time_ns, (&sf.fault).into());
            if let Some(ref label) = sf.label {
                fault = fault.with_label(label.clone());
            }
            schedule.add(fault);
        }
        schedule
    }
}

/// Serializable bug report.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerializableBug {
    pub bug_id: u64,
    /// Non-authoritative compact alias for display and filtering.
    pub assertion_id: u64,
    /// Exact replay authority. Missing identity is legacy diagnostic input only.
    #[serde(
        default = "no_assertion_identity",
        deserialize_with = "crate::non_null_option::deserialize",
        skip_serializing_if = "Option::is_none"
    )]
    pub assertion_identity: Option<chaoscontrol_protocol::admission::AssertionEvidenceIdentity>,
    /// Process-local fallback binding. Its presence is mandatory for fallback descriptors.
    #[serde(default = "no_fallback_scope", skip_serializing_if = "Option::is_none")]
    pub fallback_scope: Option<chaoscontrol_protocol::fallback::FallbackAssertionScope>,
    pub assertion_location: String,
    pub schedule: SerializableSchedule,
    pub tick: u64,
    /// Depth of the frontier parent snapshot needed to replay this bug.
    ///
    /// Depth 0 can be replayed from the normal bootstrap snapshot. Depth > 0
    /// means the fault schedule alone is incomplete; replay needs the saved
    /// branch snapshot context.
    #[serde(default)]
    pub replay_parent_depth: u32,
    /// Durable replay parent snapshot artifact reference for parent-context replay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_parent_snapshot_ref: Option<crate::snapshot_store::ReplayParentSnapshotRef>,
    /// Dedup key: hash of (assertion fingerprint, sorted fault type names).
    #[serde(default)]
    pub dedup_key: Option<u64>,
    /// Schedule variant used when this bug was found.
    #[serde(default)]
    pub schedule_variant: Option<chaoscontrol_vmm::scheduler::ScheduleVariant>,
    /// Helical scenario config that generated the schedule (if any).
    #[serde(default)]
    pub scenario_config: Option<chaoscontrol_fault::scenario::ScenarioConfig>,
    /// Materialized phase summary (if a scenario was used).
    #[serde(default)]
    pub scenario_summary: Option<chaoscontrol_fault::scenario::PhaseSummary>,
}

fn no_assertion_identity() -> Option<chaoscontrol_protocol::admission::AssertionEvidenceIdentity> {
    None
}

fn no_fallback_scope() -> Option<chaoscontrol_protocol::fallback::FallbackAssertionScope> {
    None
}

impl SerializableBug {
    pub fn require_replay_identity(
        &self,
    ) -> Result<
        &chaoscontrol_protocol::admission::AssertionEvidenceIdentity,
        crate::bug::identity::BugIdentityError,
    > {
        let identity = crate::bug::identity::validate_carrier(
            self.assertion_id,
            self.assertion_identity.as_ref(),
        )?;
        crate::bug::identity::validate_fallback_scope(identity, self.fallback_scope.as_ref())?;
        Ok(identity)
    }
}

#[derive(Debug, Snafu)]
#[snafu(display("bug {bug_id} has invalid replay identity: {source}"))]
pub struct BugSetIdentityError {
    pub bug_id: u64,
    pub source: crate::bug::identity::BugIdentityError,
}

pub fn validate_bug_set(
    bugs: &[SerializableBug],
    assertion_report: Option<&chaoscontrol_fault::oracle::OracleReport>,
) -> Result<(), BugSetIdentityError> {
    let Some(first_bug) = bugs.first() else {
        return Ok(());
    };
    let report = assertion_report.ok_or(BugSetIdentityError {
        bug_id: first_bug.bug_id,
        source: crate::bug::identity::BugIdentityError::ReportMismatch,
    })?;
    for bug in bugs {
        crate::corpus::BugReport::try_from(bug).map_err(|source| BugSetIdentityError {
            bug_id: bug.bug_id,
            source,
        })?;
        crate::bug::identity::resolve_restored_report(
            bug.assertion_id,
            bug.assertion_identity.as_ref(),
            report,
        )
        .map_err(|source| BugSetIdentityError {
            bug_id: bug.bug_id,
            source,
        })?;
    }
    Ok(())
}

pub fn replay_bug_set(
    bugs: &[SerializableBug],
    assertion_report: Option<&chaoscontrol_fault::oracle::OracleReport>,
) -> Result<Vec<crate::corpus::BugReport>, BugSetIdentityError> {
    validate_bug_set(bugs, assertion_report)?;
    bugs.iter()
        .map(|bug| {
            crate::corpus::BugReport::try_from(bug).map_err(|source| BugSetIdentityError {
                bug_id: bug.bug_id,
                source,
            })
        })
        .collect()
}

impl From<&crate::corpus::BugReport> for SerializableBug {
    fn from(bug: &crate::corpus::BugReport) -> Self {
        SerializableBug {
            bug_id: bug.bug_id,
            assertion_id: bug.assertion_id,
            assertion_identity: Some(bug.assertion_identity.clone()),
            fallback_scope: bug.fallback_scope.clone(),
            assertion_location: bug.assertion_location.clone(),
            schedule: (&bug.schedule).into(),
            tick: bug.tick,
            replay_parent_depth: bug.replay_parent_depth,
            replay_parent_snapshot_ref: bug.replay_parent_snapshot_ref.clone(),
            dedup_key: Some(bug.dedup_key),
            schedule_variant: bug.schedule_variant.clone(),
            scenario_config: bug.scenario_config.clone(),
            scenario_summary: bug.scenario_summary.clone(),
        }
    }
}

impl TryFrom<&SerializableBug> for crate::corpus::BugReport {
    type Error = crate::bug::identity::BugIdentityError;

    fn try_from(bug: &SerializableBug) -> Result<Self, Self::Error> {
        let assertion_identity = bug.require_replay_identity()?.clone();
        if bug.assertion_location.is_empty()
            || bug.schedule.faults.len() > MAX_SERIALIZABLE_FAULTS
            || (bug.replay_parent_depth > 0) != bug.replay_parent_snapshot_ref.is_some()
        {
            return Err(crate::bug::identity::BugIdentityError::MalformedCarrier);
        }
        Ok(Self {
            bug_id: bug.bug_id,
            assertion_id: bug.assertion_id,
            assertion_identity,
            fallback_scope: bug.fallback_scope.clone(),
            assertion_location: bug.assertion_location.clone(),
            schedule: (&bug.schedule).into(),
            snapshot: None,
            tick: bug.tick,
            replay_parent_depth: bug.replay_parent_depth,
            replay_parent_snapshot_ref: bug.replay_parent_snapshot_ref.clone(),
            dedup_key: bug.dedup_key.unwrap_or(0),
            schedule_variant: bug.schedule_variant.clone(),
            scenario_config: bug.scenario_config.clone(),
            scenario_summary: bug.scenario_summary.clone(),
        })
    }
}

/// Complete checkpoint — everything needed to resume exploration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationCheckpoint {
    pub config: CheckpointConfig,
    pub global_coverage: Vec<u8>,
    pub bugs: Vec<SerializableBug>,
    /// Accepted report that admits every retained bug identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion_report: Option<chaoscontrol_fault::oracle::OracleReport>,
    pub rounds_completed: u64,
    pub total_branches_run: u64,
    pub total_edges: usize,
    pub seed: u64,
    /// Per-round exploration history (optional for backward compat).
    #[serde(default)]
    pub round_history: Option<Vec<crate::explorer::RoundHistory>>,
    /// Dedup keys for bugs already seen (optional for backward compat).
    #[serde(default)]
    pub seen_dedup_keys: Option<Vec<u64>>,
    /// Helical scenario config (if the run used one).
    #[serde(default)]
    pub scenario: Option<chaoscontrol_fault::scenario::ScenarioConfig>,
    /// Materialized phase summary (if a scenario was used).
    #[serde(default)]
    pub scenario_summary: Option<chaoscontrol_fault::scenario::PhaseSummary>,
}

/// Save a checkpoint to a JSON file.
pub fn save_checkpoint<P: AsRef<std::path::Path>>(
    path: P,
    checkpoint: &ExplorationCheckpoint,
) -> Result<(), CheckpointError> {
    validate_checkpoint(checkpoint)?;
    let bytes = serde_json::to_vec_pretty(checkpoint)?;
    if bytes.len() as u64 > crate::bounded_json::MAX_CHECKPOINT_BYTES {
        return Err(CheckpointError::InvalidBounds {
            reason: "serialized checkpoint exceeds the byte limit".to_string(),
        });
    }
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    ::std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// Load a checkpoint from a JSON file.
pub fn load_checkpoint<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<ExplorationCheckpoint, CheckpointError> {
    let json = crate::bounded_json::read_checkpoint(path.as_ref())?;
    let checkpoint = serde_json::from_str(&json)?;
    validate_checkpoint(&checkpoint)?;
    Ok(checkpoint)
}

pub fn load_serializable_bug<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<SerializableBug, CheckpointError> {
    load_serializable_bug_artifact(path).map(|(bug, _bytes)| bug)
}

pub fn load_serializable_bug_artifact<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<(SerializableBug, Vec<u8>), CheckpointError> {
    let json = crate::bounded_json::read_checkpoint(path.as_ref())?;
    let bug: SerializableBug = serde_json::from_str(&json)?;
    crate::corpus::BugReport::try_from(&bug).map_err(|source| {
        CheckpointError::InvalidBugIdentity {
            source: BugSetIdentityError {
                bug_id: bug.bug_id,
                source,
            },
        }
    })?;
    Ok((bug, json.into_bytes()))
}

fn validate_checkpoint(checkpoint: &ExplorationCheckpoint) -> Result<(), CheckpointError> {
    if checkpoint.global_coverage.len() > crate::coverage::MAP_SIZE {
        return Err(CheckpointError::InvalidBounds {
            reason: "global coverage exceeds the bitmap size".to_string(),
        });
    }
    if checkpoint.bugs.len() > MAX_CHECKPOINT_BUGS {
        return Err(CheckpointError::InvalidBounds {
            reason: "bug count exceeds the checkpoint limit".to_string(),
        });
    }
    if checkpoint
        .round_history
        .as_ref()
        .is_some_and(|history| history.len() > MAX_CHECKPOINT_ROUND_HISTORY)
    {
        return Err(CheckpointError::InvalidBounds {
            reason: "round history exceeds the checkpoint limit".to_string(),
        });
    }
    if checkpoint
        .seen_dedup_keys
        .as_ref()
        .is_some_and(|keys| keys.len() > MAX_CHECKPOINT_DEDUP_KEYS)
    {
        return Err(CheckpointError::InvalidBounds {
            reason: "dedup key count exceeds the checkpoint limit".to_string(),
        });
    }
    if let Some(report) = checkpoint.assertion_report.as_ref() {
        chaoscontrol_fault::oracle_validation::validate_oracle_report_claim(report).map_err(
            |error| CheckpointError::InvalidAssertionReport {
                reason: format!("{error:?}"),
            },
        )?;
    }
    validate_bug_set(&checkpoint.bugs, checkpoint.assertion_report.as_ref())?;
    Ok(())
}

#[derive(Debug, Snafu)]
pub enum CheckpointBugExportError {
    #[snafu(display("checkpoint error"), context(false))]
    Checkpoint { source: CheckpointError },

    #[snafu(display("I/O error"), context(false))]
    Io { source: std::io::Error },

    #[snafu(display("JSON error"), context(false))]
    Json { source: serde_json::Error },

    #[snafu(display("snapshot store error"), context(false))]
    SnapshotStore {
        source: crate::snapshot_store::SnapshotStoreError,
    },

    #[snafu(display("checkpoint bug {bug_id} cannot export for replay: {reason}"))]
    InvalidAssertionIdentity { bug_id: u64, reason: String },

    #[snafu(display("bug artifact already exists: {}", path.display()))]
    AlreadyExists { path: std::path::PathBuf },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckpointBugExportFilter {
    pub assertion_id: Option<u64>,
    pub min_replay_parent_depth: Option<u32>,
    pub max_bugs: Option<usize>,
}

impl CheckpointBugExportFilter {
    fn matches(&self, bug: &SerializableBug) -> bool {
        if let Some(assertion_id) = self.assertion_id {
            if bug.assertion_id != assertion_id {
                return false;
            }
        }
        if let Some(min_depth) = self.min_replay_parent_depth {
            if bug.replay_parent_depth < min_depth {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointBugExportSummary {
    pub bugs_scanned: usize,
    pub bugs_matched: usize,
    pub bugs_written: usize,
    pub snapshot_refs_validated: usize,
}

/// Export checkpoint-held bug reports as `bug_N.json` artifacts.
///
/// This is a finalization path for interrupted/resumed campaigns where bugs
/// reached `checkpoint.json` before the normal end-of-run artifact writer ran.
/// Existing replay parent snapshot refs are validated against the checkpoint store.
/// Matched snapshots are copied into the output store with the same exact ref.
pub fn export_checkpoint_bugs<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(
    checkpoint_path: P,
    output_dir: Q,
    overwrite: bool,
) -> Result<CheckpointBugExportSummary, CheckpointBugExportError> {
    export_checkpoint_bugs_with_filter(
        checkpoint_path,
        output_dir,
        overwrite,
        CheckpointBugExportFilter::default(),
    )
}

/// Export only checkpoint-held bugs matching the provided filter.
///
/// Filtered exports preserve the checkpoint bug index in `bug_N.json` filenames.
/// The loader validates every bug identity and reference shape before filtering.
/// Snapshot artifact bytes are loaded only for matched bugs.
pub fn export_checkpoint_bugs_with_filter<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(
    checkpoint_path: P,
    output_dir: Q,
    overwrite: bool,
    filter: CheckpointBugExportFilter,
) -> Result<CheckpointBugExportSummary, CheckpointBugExportError> {
    let checkpoint_path = checkpoint_path.as_ref();
    let checkpoint = load_checkpoint(checkpoint_path)?;
    validate_bug_set(&checkpoint.bugs, checkpoint.assertion_report.as_ref()).map_err(|error| {
        CheckpointBugExportError::InvalidAssertionIdentity {
            bug_id: error.bug_id,
            reason: error.source.to_string(),
        }
    })?;
    let source_dir = checkpoint_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let output_dir = output_dir.as_ref();
    ::std::fs::create_dir_all(output_dir)?;
    let source_snapshot_store = crate::snapshot_store::FileSnapshotStore::new(source_dir);
    let output_snapshot_store = crate::snapshot_store::FileSnapshotStore::new(output_dir);

    let mut bugs_matched = 0;
    let mut bugs_written = 0;
    let mut snapshot_refs_validated = 0;
    for (index, bug) in checkpoint.bugs.iter().enumerate() {
        if let Some(max_bugs) = filter.max_bugs {
            if bugs_written >= max_bugs {
                break;
            }
        }
        if !filter.matches(bug) {
            continue;
        }
        bugs_matched += 1;

        if let Some(reference) = bug.replay_parent_snapshot_ref.as_ref() {
            let artifact = source_snapshot_store.get_snapshot_artifact(reference)?;
            if artifact.replay_parent_depth != bug.replay_parent_depth {
                return Err(CheckpointBugExportError::SnapshotStore {
                    source: crate::snapshot_store::SnapshotStoreError::MetadataMismatch {
                        field: "replay_parent_depth",
                    },
                });
            }
            let output_reference = output_snapshot_store
                .put_snapshot(&artifact.snapshot, artifact.replay_parent_depth)?;
            if output_reference != *reference {
                return Err(CheckpointBugExportError::SnapshotStore {
                    source: crate::snapshot_store::SnapshotStoreError::MetadataMismatch {
                        field: "exported_reference",
                    },
                });
            }
            snapshot_refs_validated += 1;
        }

        let path = output_dir.join(format!("bug_{index}.json"));
        if path.exists() && !overwrite {
            return Err(CheckpointBugExportError::AlreadyExists { path });
        }
        let json = serde_json::to_string_pretty(bug)?;
        ::std::fs::write(path, json)?;
        bugs_written += 1;
    }

    Ok(CheckpointBugExportSummary {
        bugs_scanned: checkpoint.bugs.len(),
        bugs_matched,
        bugs_written,
        snapshot_refs_validated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nickel_bug_fixture_round_trips_through_rust_type() {
        let json = include_str!("../../../contracts/evidence/fixtures/valid/bug-report.valid.json");
        let bug: SerializableBug = serde_json::from_str(json).unwrap();
        assert_eq!(bug.bug_id, 0);
        assert_eq!(bug.assertion_id, 1_205_943_209);
        assert!(!bug.assertion_location.is_empty());
        assert!(!bug.schedule.faults.is_empty());

        let roundtrip = serde_json::to_string(&bug).unwrap();
        let reparsed: SerializableBug = serde_json::from_str(&roundtrip).unwrap();
        assert_eq!(bug.assertion_id, reparsed.assertion_id);
        assert_eq!(bug.tick, reparsed.tick);
    }

    #[test]
    fn standalone_bug_loader_accepts_a_complete_identity() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("bug.json");
        let bug = minimal_bug(1);
        ::std::fs::write(&path, serde_json::to_vec(&bug).expect("serialize bug"))
            .expect("write bug fixture");

        let loaded = load_serializable_bug(&path).expect("complete bug loads");
        assert_eq!(loaded.bug_id, bug.bug_id);
        assert_eq!(loaded.assertion_identity, bug.assertion_identity);
    }

    #[test]
    fn standalone_bug_loader_rejects_an_oversized_schedule() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("bug.json");
        let mut bug = minimal_bug(1);
        let fault = SerializableScheduledFault {
            time_ns: 0,
            fault: SerializableFault::NetworkLatency {
                target: 0,
                latency_ns: 1,
            },
            label: None,
        };
        bug.schedule.faults = vec![fault; MAX_SERIALIZABLE_FAULTS + 1];
        ::std::fs::write(&path, serde_json::to_vec(&bug).expect("serialize bug"))
            .expect("write bug fixture");

        assert!(matches!(
            load_serializable_bug(&path),
            Err(CheckpointError::InvalidBugIdentity { .. })
        ));
    }

    #[test]
    fn nickel_checkpoint_fixture_round_trips_through_rust_type() {
        let json = include_str!("../../../dogfood-results/raft-20260506-095025/checkpoint.json");
        let checkpoint: ExplorationCheckpoint = serde_json::from_str(json).unwrap();
        assert_eq!(checkpoint.config.num_vms, 3);
        assert_eq!(checkpoint.rounds_completed, 19);
        assert_eq!(checkpoint.bugs.len(), 1);

        let roundtrip = serde_json::to_string(&checkpoint).unwrap();
        let reparsed: ExplorationCheckpoint = serde_json::from_str(&roundtrip).unwrap();
        assert_eq!(checkpoint.seed, reparsed.seed);
        assert_eq!(checkpoint.total_branches_run, reparsed.total_branches_run);
        assert_eq!(
            checkpoint.bugs[0].assertion_id,
            reparsed.bugs[0].assertion_id
        );
    }

    #[test]
    fn test_serialize_fault() {
        let fault = ::chaoscontrol_fault::faults::Fault::NetworkPartition {
            side_a: vec![0, 1],
            side_b: vec![2, 3],
        };
        let serializable: SerializableFault = (&fault).into();
        let roundtrip: ::chaoscontrol_fault::faults::Fault = (&serializable).into();
        assert_eq!(fault, roundtrip);
    }

    #[test]
    fn test_serialize_schedule() {
        let mut schedule = ::chaoscontrol_fault::schedule::FaultSchedule::new();
        schedule.add(::chaoscontrol_fault::schedule::ScheduledFault::new(
            1000,
            ::chaoscontrol_fault::faults::Fault::NetworkHeal,
        ));
        schedule.add(::chaoscontrol_fault::schedule::ScheduledFault::new(
            2000,
            ::chaoscontrol_fault::faults::Fault::ProcessKill { target: 0 },
        ));

        let serializable: SerializableSchedule = (&schedule).into();
        assert_eq!(serializable.faults.len(), 2);

        let roundtrip: ::chaoscontrol_fault::schedule::FaultSchedule = (&serializable).into();
        assert_eq!(roundtrip.total(), 2);
    }

    #[test]
    fn test_checkpoint_roundtrip() {
        let checkpoint = ExplorationCheckpoint {
            config: CheckpointConfig {
                num_vms: 2,
                kernel_path: "/path/to/kernel".to_string(),
                initrd_path: Some("/path/to/initrd".to_string()),
                seed: 42,
                branch_factor: 8,
                ticks_per_branch: 1000,
                max_rounds: 100,
                max_frontier: 50,
                quantum: 100,
                coverage_gpa: 0x1000000,
                disk_image_path: None,
                bootstrap_budget: 10_000,
                schedule_diversity: false,
                schedule_mutation_ratio: 0.0,
                rare_edge_threshold: None,
                rare_edge_weight: None,
                havoc_after_stale: None,
                havoc_mutations: None,
                scenario: None,
            },
            global_coverage: vec![1, 2, 3, 4, 5],
            bugs: vec![],
            assertion_report: None,
            rounds_completed: 10,
            total_branches_run: 80,
            total_edges: 1234,
            seed: 42,
            round_history: None,
            seen_dedup_keys: None,
            scenario: None,
            scenario_summary: None,
        };

        let json = serde_json::to_string(&checkpoint).unwrap();
        let roundtrip: ExplorationCheckpoint = serde_json::from_str(&json).unwrap();

        assert_eq!(checkpoint.config.num_vms, roundtrip.config.num_vms);
        assert_eq!(checkpoint.rounds_completed, roundtrip.rounds_completed);
        assert_eq!(checkpoint.total_edges, roundtrip.total_edges);
        assert_eq!(checkpoint.global_coverage, roundtrip.global_coverage);
    }

    #[test]
    fn test_save_load_checkpoint() {
        let tempdir = std::env::temp_dir();
        let path = tempdir.join("test_checkpoint.json");

        let checkpoint = ExplorationCheckpoint {
            config: CheckpointConfig {
                num_vms: 3,
                kernel_path: "kernel".to_string(),
                initrd_path: None,
                seed: 123,
                branch_factor: 4,
                ticks_per_branch: 500,
                max_rounds: 50,
                max_frontier: 25,
                quantum: 50,
                coverage_gpa: 0x2000000,
                disk_image_path: None,
                bootstrap_budget: 10_000,
                schedule_diversity: false,
                schedule_mutation_ratio: 0.0,
                rare_edge_threshold: None,
                rare_edge_weight: None,
                havoc_after_stale: None,
                havoc_mutations: None,
                scenario: None,
            },
            global_coverage: vec![10, 20, 30],
            bugs: vec![],
            assertion_report: None,
            rounds_completed: 5,
            total_branches_run: 20,
            total_edges: 567,
            seed: 123,
            round_history: None,
            seen_dedup_keys: None,
            scenario: None,
            scenario_summary: None,
        };

        save_checkpoint(&path, &checkpoint).unwrap();
        let loaded = load_checkpoint(&path).unwrap();

        assert_eq!(checkpoint.config.num_vms, loaded.config.num_vms);
        assert_eq!(checkpoint.rounds_completed, loaded.rounds_completed);
        assert_eq!(checkpoint.global_coverage, loaded.global_coverage);

        // Cleanup
        let _ = ::std::fs::remove_file(&path);
    }

    #[test]
    fn test_checkpoint_round_history_roundtrip() {
        use crate::explorer::RoundHistory;

        let history = vec![
            RoundHistory {
                round: 1,
                branches_run: 8,
                new_edges: 42,
                cumulative_edges: 42,
                bugs_found: 0,
                cumulative_bugs: 0,
                frontier_size: 3,
                corpus_size: 3,
                restore_ms: 0.0,
                run_ms: 0.0,
                snapshot_ms: 0.0,
                coverage_ms: 0.0,
                wall_clock_seconds: 0.0,
            },
            RoundHistory {
                round: 2,
                branches_run: 8,
                new_edges: 10,
                cumulative_edges: 52,
                bugs_found: 1,
                cumulative_bugs: 1,
                frontier_size: 5,
                corpus_size: 5,
                restore_ms: 0.0,
                run_ms: 0.0,
                snapshot_ms: 0.0,
                coverage_ms: 0.0,
                wall_clock_seconds: 0.0,
            },
        ];

        let checkpoint = ExplorationCheckpoint {
            config: CheckpointConfig {
                num_vms: 2,
                kernel_path: "k".to_string(),
                initrd_path: None,
                seed: 1,
                branch_factor: 8,
                ticks_per_branch: 1000,
                max_rounds: 10,
                max_frontier: 50,
                quantum: 100,
                coverage_gpa: 0xE0000,
                disk_image_path: None,
                bootstrap_budget: 10_000,
                schedule_diversity: false,
                schedule_mutation_ratio: 0.0,
                rare_edge_threshold: None,
                rare_edge_weight: None,
                havoc_after_stale: None,
                havoc_mutations: None,
                scenario: None,
            },
            global_coverage: vec![],
            bugs: vec![],
            assertion_report: None,
            rounds_completed: 2,
            total_branches_run: 16,
            total_edges: 52,
            seed: 1,
            round_history: Some(history.clone()),
            seen_dedup_keys: None,
            scenario: None,
            scenario_summary: None,
        };

        let json = serde_json::to_string(&checkpoint).unwrap();
        let loaded: ExplorationCheckpoint = serde_json::from_str(&json).unwrap();

        let restored = loaded.round_history.unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].round, 1);
        assert_eq!(restored[0].new_edges, 42);
        assert_eq!(restored[1].cumulative_bugs, 1);
    }

    #[test]
    fn test_checkpoint_backward_compat_no_round_history() {
        // Simulate loading a checkpoint from before round_history was added
        let json = r#"{
            "config": {
                "num_vms": 2,
                "kernel_path": "k",
                "initrd_path": null,
                "seed": 1,
                "branch_factor": 8,
                "ticks_per_branch": 1000,
                "max_rounds": 10,
                "max_frontier": 50,
                "quantum": 100,
                "coverage_gpa": 917504
            },
            "global_coverage": [],
            "bugs": [],
            "rounds_completed": 5,
            "total_branches_run": 40,
            "total_edges": 100,
            "seed": 1
        }"#;

        let checkpoint: ExplorationCheckpoint = serde_json::from_str(json).unwrap();
        assert_eq!(checkpoint.rounds_completed, 5);
        assert!(checkpoint.round_history.is_none());
    }

    #[test]
    fn test_checkpoint_scenario_metadata_roundtrip() {
        use chaoscontrol_fault::scenario::{materialize, ScenarioConfig, ScenarioFamily};

        let sc_config = ScenarioConfig::new(ScenarioFamily::VolatileWriteRing, 3, 500, 4);
        let materialized = materialize(&sc_config, 42);

        let checkpoint = ExplorationCheckpoint {
            config: CheckpointConfig {
                num_vms: 3,
                kernel_path: "k".to_string(),
                initrd_path: None,
                seed: 42,
                branch_factor: 8,
                ticks_per_branch: 1000,
                max_rounds: 50,
                max_frontier: 25,
                quantum: 100,
                coverage_gpa: 0xE0000,
                disk_image_path: None,
                bootstrap_budget: 10_000,
                schedule_diversity: false,
                schedule_mutation_ratio: 0.0,
                rare_edge_threshold: None,
                rare_edge_weight: None,
                havoc_after_stale: None,
                havoc_mutations: None,
                scenario: Some(sc_config.clone()),
            },
            global_coverage: vec![],
            bugs: vec![SerializableBug {
                bug_id: 1,
                assertion_id: 50,
                assertion_identity: Some(crate::test_support::assertion_identity(50)),
                fallback_scope: None,
                assertion_location: "test.rs:1".into(),
                schedule: SerializableSchedule { faults: Vec::new() },
                tick: 1000,
                replay_parent_depth: 0,
                replay_parent_snapshot_ref: None,
                dedup_key: Some(0xBB),
                schedule_variant: None,
                scenario_config: Some(sc_config.clone()),
                scenario_summary: Some(materialized.summary.clone()),
            }],
            assertion_report: None,
            rounds_completed: 5,
            total_branches_run: 40,
            total_edges: 100,
            seed: 42,
            round_history: None,
            seen_dedup_keys: None,
            scenario: Some(sc_config),
            scenario_summary: Some(materialized.summary),
        };

        let json = serde_json::to_string_pretty(&checkpoint).unwrap();
        let loaded: ExplorationCheckpoint = serde_json::from_str(&json).unwrap();

        // Verify checkpoint-level scenario metadata survived
        let loaded_sc = loaded.scenario.unwrap();
        assert_eq!(loaded_sc.family, ScenarioFamily::VolatileWriteRing);
        assert_eq!(loaded_sc.turns, 4);
        assert_eq!(loaded_sc.phase_ticks, 500);
        assert_eq!(loaded_sc.num_vms, 3);

        let loaded_summary = loaded.scenario_summary.unwrap();
        assert!(!loaded_summary.phases.is_empty());

        // Verify bug-level scenario metadata survived
        let bug = &loaded.bugs[0];
        let bug_sc = bug.scenario_config.as_ref().unwrap();
        assert_eq!(bug_sc.family, ScenarioFamily::VolatileWriteRing);
        assert!(bug.scenario_summary.is_some());
    }

    #[test]
    fn test_checkpoint_backward_compat_no_scenario() {
        // Simulate loading a checkpoint from before scenario fields were added
        let json = r#"{
            "config": {
                "num_vms": 2,
                "kernel_path": "k",
                "initrd_path": null,
                "seed": 1,
                "branch_factor": 8,
                "ticks_per_branch": 1000,
                "max_rounds": 10,
                "max_frontier": 50,
                "quantum": 100,
                "coverage_gpa": 917504
            },
            "global_coverage": [],
            "bugs": [],
            "rounds_completed": 5,
            "total_branches_run": 40,
            "total_edges": 100,
            "seed": 1
        }"#;

        let checkpoint: ExplorationCheckpoint = serde_json::from_str(json).unwrap();
        assert!(checkpoint.scenario.is_none());
        assert!(checkpoint.scenario_summary.is_none());
        assert!(checkpoint.config.scenario.is_none());
    }

    fn minimal_checkpoint_with_bugs(mut bugs: Vec<SerializableBug>) -> ExplorationCheckpoint {
        let mut descriptors = std::collections::BTreeMap::new();
        for bug in &bugs {
            if let Some(identity) = bug.assertion_identity.as_ref() {
                descriptors.insert(identity.fingerprint, identity.descriptor.clone());
            }
        }
        let assertion_report = if descriptors.is_empty() {
            None
        } else {
            let descriptors = descriptors.into_values().collect::<Vec<_>>();
            let token = chaoscontrol_protocol::admission::token_for_descriptors(&descriptors)
                .expect("catalog token");
            let mut builder =
                chaoscontrol_protocol::admission::CatalogBuilder::begin(descriptors.len())
                    .expect("catalog begins");
            for descriptor in descriptors {
                builder.insert(descriptor).expect("descriptor inserts");
            }
            let catalog = builder.complete(token).expect("catalog completes");
            for bug in &mut bugs {
                let Some(identity) = bug.assertion_identity.as_ref() else {
                    continue;
                };
                let admitted = catalog
                    .assertions
                    .get(&identity.fingerprint)
                    .expect("bug descriptor admitted");
                bug.assertion_identity = Some(
                    chaoscontrol_protocol::admission::AssertionEvidenceIdentity::from_admitted(
                        admitted, token,
                    )
                    .expect("evidence identity"),
                );
            }
            let mut oracle = chaoscontrol_fault::oracle::PropertyOracle::new();
            oracle.activate_catalog(catalog).expect("catalog activates");
            Some(oracle.report())
        };

        ExplorationCheckpoint {
            config: CheckpointConfig {
                num_vms: 2,
                kernel_path: "k".to_string(),
                initrd_path: None,
                seed: 1,
                branch_factor: 8,
                ticks_per_branch: 1000,
                max_rounds: 10,
                max_frontier: 50,
                quantum: 100,
                coverage_gpa: 0xE0000,
                disk_image_path: None,
                bootstrap_budget: 10_000,
                schedule_diversity: false,
                schedule_mutation_ratio: 0.0,
                rare_edge_threshold: None,
                rare_edge_weight: None,
                havoc_after_stale: None,
                havoc_mutations: None,
                scenario: None,
            },
            global_coverage: vec![],
            bugs,
            assertion_report,
            rounds_completed: 1,
            total_branches_run: 8,
            total_edges: 10,
            seed: 1,
            round_history: None,
            seen_dedup_keys: None,
            scenario: None,
            scenario_summary: None,
        }
    }

    const TEST_EXPORT_ALIAS: u64 = 1_806_003_755;

    fn write_untrusted_checkpoint(path: &std::path::Path, checkpoint: &ExplorationCheckpoint) {
        let bytes = serde_json::to_vec_pretty(checkpoint).expect("checkpoint fixture JSON");
        ::std::fs::write(path, bytes).expect("write untrusted checkpoint fixture");
    }

    fn minimal_bug(bug_id: u64) -> SerializableBug {
        let assertion_id = bug_id + 100;
        SerializableBug {
            bug_id,
            assertion_id,
            assertion_identity: Some(crate::test_support::assertion_identity(assertion_id)),
            fallback_scope: None,
            assertion_location: "test.rs:1".into(),
            schedule: SerializableSchedule { faults: Vec::new() },
            tick: 1000,
            replay_parent_depth: 0,
            replay_parent_snapshot_ref: None,
            dedup_key: Some(bug_id),
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
        }
    }

    fn set_assertion_alias(bug: &mut SerializableBug, assertion_id: u64) {
        bug.assertion_id = assertion_id;
        bug.assertion_identity = Some(crate::test_support::assertion_identity(assertion_id));
    }

    #[test]
    fn invalid_checkpoint_does_not_replace_a_valid_checkpoint() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let valid = minimal_checkpoint_with_bugs(Vec::new());
        save_checkpoint(&path, &valid).expect("valid checkpoint writes");
        let before = ::std::fs::read(&path).expect("valid checkpoint bytes");
        let mut invalid = minimal_checkpoint_with_bugs(vec![minimal_bug(1)]);
        invalid.assertion_report = None;

        assert!(save_checkpoint(&path, &invalid).is_err());
        assert_eq!(
            ::std::fs::read(&path).expect("retained checkpoint bytes"),
            before
        );
    }

    #[cfg(unix)]
    #[test]
    fn checkpoint_save_replaces_a_symlink_without_following_it() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let target = directory.path().join("target.json");
        ::std::fs::write(&target, b"retain target").expect("target fixture");
        symlink(&target, &path).expect("checkpoint symlink");

        save_checkpoint(&path, &minimal_checkpoint_with_bugs(Vec::new()))
            .expect("valid checkpoint replaces symlink");
        assert_eq!(
            ::std::fs::read(&target).expect("target bytes"),
            b"retain target"
        );
        assert!(!::std::fs::symlink_metadata(&path)
            .expect("checkpoint metadata")
            .file_type()
            .is_symlink());
    }

    #[test]
    fn load_rejects_bug_without_admitting_report() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let mut checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(1)]);
        checkpoint.assertion_report = None;
        assert!(save_checkpoint(&path, &checkpoint).is_err());
        assert!(!path.exists());
        write_untrusted_checkpoint(&path, &checkpoint);

        assert!(matches!(
            load_checkpoint(&path),
            Err(CheckpointError::InvalidBugIdentity { .. })
        ));
    }

    #[test]
    fn load_rejects_catalog_token_substitution() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let mut checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(1)]);
        checkpoint.bugs[0]
            .assertion_identity
            .as_mut()
            .expect("identity")
            .catalog_token = chaoscontrol_protocol::identity::AssertionFingerprint::ZERO;
        assert!(save_checkpoint(&path, &checkpoint).is_err());
        write_untrusted_checkpoint(&path, &checkpoint);

        assert!(matches!(
            load_checkpoint(&path),
            Err(CheckpointError::InvalidBugIdentity { .. })
        ));
    }

    #[test]
    fn export_checkpoint_bugs_writes_indexed_bug_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        let checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(41), minimal_bug(99)]);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        let summary = export_checkpoint_bugs(&checkpoint_path, &out, true).unwrap();

        assert_eq!(summary.bugs_scanned, 2);
        assert_eq!(summary.bugs_matched, 2);
        assert_eq!(summary.bugs_written, 2);
        assert_eq!(summary.snapshot_refs_validated, 0);
        let bug0: SerializableBug =
            serde_json::from_str(&::std::fs::read_to_string(out.join("bug_0.json")).unwrap())
                .unwrap();
        let bug1: SerializableBug =
            serde_json::from_str(&::std::fs::read_to_string(out.join("bug_1.json")).unwrap())
                .unwrap();
        assert_eq!(bug0.bug_id, 41);
        assert_eq!(bug1.bug_id, 99);
    }

    #[test]
    fn export_checkpoint_bugs_filters_targeted_snapshot_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        let skipped_assertion = minimal_bug(1);
        let mut selected = minimal_bug(2);
        set_assertion_alias(&mut selected, TEST_EXPORT_ALIAS);
        selected.replay_parent_depth = 2;
        let skipped_depth = minimal_bug(3);
        let checkpoint =
            minimal_checkpoint_with_bugs(vec![skipped_assertion, selected, skipped_depth]);
        write_untrusted_checkpoint(&checkpoint_path, &checkpoint);

        let summary = export_checkpoint_bugs_with_filter(
            &checkpoint_path,
            &out,
            true,
            CheckpointBugExportFilter {
                assertion_id: Some(TEST_EXPORT_ALIAS),
                min_replay_parent_depth: Some(1),
                max_bugs: None,
            },
        )
        .unwrap_err();

        assert!(matches!(
            summary,
            CheckpointBugExportError::Checkpoint {
                source: CheckpointError::InvalidBugIdentity {
                    source: BugSetIdentityError { bug_id: 2, .. }
                }
            }
        ));
        assert!(!out.join("bug_0.json").exists());
        assert!(!out.join("bug_2.json").exists());
    }

    #[test]
    fn export_checkpoint_bugs_filter_rejects_unmatched_missing_snapshot_ref() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        let mut skipped = minimal_bug(7);
        set_assertion_alias(&mut skipped, 1);
        skipped.replay_parent_depth = 2;
        let mut selected = minimal_bug(8);
        set_assertion_alias(&mut selected, TEST_EXPORT_ALIAS);
        selected.replay_parent_depth = 0;
        let checkpoint = minimal_checkpoint_with_bugs(vec![skipped, selected]);
        write_untrusted_checkpoint(&checkpoint_path, &checkpoint);

        let summary = export_checkpoint_bugs_with_filter(
            &checkpoint_path,
            &out,
            true,
            CheckpointBugExportFilter {
                assertion_id: Some(TEST_EXPORT_ALIAS),
                min_replay_parent_depth: None,
                max_bugs: None,
            },
        )
        .expect_err("an invalid unmatched bug rejects the complete checkpoint");

        assert!(matches!(
            summary,
            CheckpointBugExportError::Checkpoint {
                source: CheckpointError::InvalidBugIdentity {
                    source: BugSetIdentityError { bug_id: 7, .. }
                }
            }
        ));
        assert!(!out.exists());
    }

    #[test]
    fn export_checkpoint_bugs_filter_stops_after_max_bugs() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        let mut first = minimal_bug(1);
        set_assertion_alias(&mut first, TEST_EXPORT_ALIAS);
        let mut second = minimal_bug(2);
        set_assertion_alias(&mut second, TEST_EXPORT_ALIAS);
        let checkpoint = minimal_checkpoint_with_bugs(vec![first, second]);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        let summary = export_checkpoint_bugs_with_filter(
            &checkpoint_path,
            &out,
            true,
            CheckpointBugExportFilter {
                assertion_id: Some(TEST_EXPORT_ALIAS),
                min_replay_parent_depth: None,
                max_bugs: Some(1),
            },
        )
        .unwrap();

        assert_eq!(summary.bugs_scanned, 2);
        assert_eq!(summary.bugs_matched, 1);
        assert_eq!(summary.bugs_written, 1);
        assert!(out.join("bug_0.json").exists());
        assert!(!out.join("bug_1.json").exists());
    }

    #[test]
    fn export_rejects_complete_carrier_when_unmatched_bug_is_legacy() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        let valid = minimal_bug(1);
        let mut legacy = minimal_bug(2);
        legacy.assertion_identity = None;
        let checkpoint = minimal_checkpoint_with_bugs(vec![valid, legacy]);
        write_untrusted_checkpoint(&checkpoint_path, &checkpoint);

        let error = export_checkpoint_bugs_with_filter(
            &checkpoint_path,
            &out,
            true,
            CheckpointBugExportFilter {
                assertion_id: None,
                min_replay_parent_depth: None,
                max_bugs: Some(1),
            },
        )
        .expect_err("invalid neighbor rejects the complete carrier");

        assert!(matches!(
            error,
            CheckpointBugExportError::Checkpoint {
                source: CheckpointError::InvalidBugIdentity {
                    source: BugSetIdentityError { bug_id: 2, .. }
                }
            }
        ));
        assert!(!out.exists());
    }

    #[test]
    fn export_checkpoint_bugs_rejects_inconsistent_snapshot_reference() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let mut bug = minimal_bug(7);
        bug.replay_parent_depth = 2;
        let checkpoint = minimal_checkpoint_with_bugs(vec![bug]);
        write_untrusted_checkpoint(&checkpoint_path, &checkpoint);

        let err = export_checkpoint_bugs(&checkpoint_path, dir.path(), true).unwrap_err();
        assert!(matches!(
            err,
            CheckpointBugExportError::Checkpoint {
                source: CheckpointError::InvalidBugIdentity {
                    source: BugSetIdentityError { bug_id: 7, .. }
                }
            }
        ));
    }
}
