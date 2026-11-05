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

use crate::assertion_checkpoints::AssertionCheckpointSelectorState;
use crate::corpus::BugReport;
use crate::explorer::FrontierPolicy;
use crate::snapshot_store::{FileSnapshotStore, ReplayParentSnapshotRef, SnapshotStore};
use chaoscontrol_fault::faults::{Fault, GpRegister};
use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_CHECKPOINT_BUGS: usize = 4_096;
const MAX_SERIALIZABLE_FAULTS: usize = 4_096;
const MAX_CHECKPOINT_ROUND_HISTORY: usize = 1_000_000;
const MAX_CHECKPOINT_DEDUP_KEYS: usize = 65_536;
const DEFAULT_MEMORY_PRESSURE_DURATION_TICKS: u64 = 1;
/// Bytes in a BLAKE3 campaign journal segment digest.
pub const CAMPAIGN_JOURNAL_DIGEST_BYTES: usize = 32;
const HEX_CHARS_PER_BYTE: usize = 2;
/// Lowercase hex characters in an encoded campaign journal segment digest.
pub const CAMPAIGN_JOURNAL_DIGEST_HEX_LEN: usize =
    CAMPAIGN_JOURNAL_DIGEST_BYTES * HEX_CHARS_PER_BYTE;
const LOWER_HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
const HEX_RADIX: u32 = 16;
const BITS_PER_NIBBLE: u32 = 4;
const LOW_NIBBLE_MASK: u8 = 0x0f;

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

    #[snafu(display("campaign journal segment digest is malformed: {reason}"))]
    InvalidCampaignJournalDigest { reason: &'static str },

    #[snafu(display(
        "checkpoint records the legacy-diagnostic frontier policy but claims release_eligible = true; diagnostic rollback outputs are never release eligible"
    ))]
    DiagnosticClaimsReleaseEligibility,
}

/// Binds a checkpoint to the last committed settlement segment of the
/// Campaign selection journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignJournalBinding {
    /// Journal epoch.
    pub epoch: u32,
    /// Sequence number of the last committed settlement segment.
    pub segment_seq: u64,
    /// Lowercase hex BLAKE3 digest of that segment (64 characters).
    pub segment_digest: String,
}

impl CampaignJournalBinding {
    /// Bind a segment by its raw BLAKE3 digest.
    #[must_use]
    pub fn new(epoch: u32, segment_seq: u64, digest: [u8; CAMPAIGN_JOURNAL_DIGEST_BYTES]) -> Self {
        let mut segment_digest = String::with_capacity(CAMPAIGN_JOURNAL_DIGEST_HEX_LEN);
        for byte in digest {
            let high = usize::from(byte >> BITS_PER_NIBBLE);
            let low = usize::from(byte & LOW_NIBBLE_MASK);
            segment_digest.push(char::from(LOWER_HEX_DIGITS[high]));
            segment_digest.push(char::from(LOWER_HEX_DIGITS[low]));
        }
        Self {
            epoch,
            segment_seq,
            segment_digest,
        }
    }

    /// Decode the segment digest; rejects anything but 64 lowercase hex characters.
    pub fn digest_bytes(&self) -> Result<[u8; CAMPAIGN_JOURNAL_DIGEST_BYTES], CheckpointError> {
        let encoded = self.segment_digest.as_bytes();
        if encoded.len() != CAMPAIGN_JOURNAL_DIGEST_HEX_LEN {
            return Err(CheckpointError::InvalidCampaignJournalDigest {
                reason: "expected 64 lowercase hex characters",
            });
        }
        let mut digest = [0_u8; CAMPAIGN_JOURNAL_DIGEST_BYTES];
        let (pairs, _) = encoded.as_chunks::<HEX_CHARS_PER_BYTE>();
        for (byte, &[high, low]) in digest.iter_mut().zip(pairs) {
            *byte = (lower_hex_value(high)? << BITS_PER_NIBBLE) | lower_hex_value(low)?;
        }
        Ok(digest)
    }
}

fn lower_hex_value(character: u8) -> Result<u8, CheckpointError> {
    if character.is_ascii_uppercase() {
        return Err(CheckpointError::InvalidCampaignJournalDigest {
            reason: "uppercase hex is not canonical",
        });
    }
    char::from(character)
        .to_digit(HEX_RADIX)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or(CheckpointError::InvalidCampaignJournalDigest {
            reason: "non-hex character",
        })
}

/// Configuration subset needed to resume exploration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    pub num_vms: usize,
    /// vCPUs per VM (defaults to 1 for checkpoints written before SMP runs).
    #[serde(default = "default_num_vcpus")]
    pub num_vcpus: usize,
    pub kernel_path: String,
    pub initrd_path: Option<String>,
    /// Extra guest kernel command-line parameters (defaults to none).
    #[serde(default)]
    pub extra_cmdline: Option<String>,
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
    /// Frontier policy of the run. Omitted for the default Campaign policy,
    /// so checkpoints written before the field existed load as Campaign.
    #[serde(default, skip_serializing_if = "FrontierPolicy::is_campaign")]
    pub frontier_policy: FrontierPolicy,
    /// Whether the run injected seed-derived random faults. Always written,
    /// so an opt-out is recorded as an explicit `false`.
    #[serde(default = "legacy_random_faults")]
    pub random_faults: bool,
}

fn default_num_vcpus() -> usize {
    1
}

fn default_bootstrap_budget() -> u64 {
    10_000
}

/// `random_faults` of a record written before the setting existed.
///
/// Those runs built their fault engine with random injection hard-coded off,
/// so their records load as off and replay the way the run executed.
const fn legacy_random_faults() -> bool {
    false
}

/// Serializable fault representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    NetworkDirectional {
        node: usize,
        peer: usize,
        direction: chaoscontrol_fault::mechanisms::NetworkDirection,
        disruption: chaoscontrol_fault::mechanisms::NetworkDisruption,
        delay_ns: u64,
        queue_limit: u32,
    },
    NetworkDirectionalHeal {
        node: usize,
        peer: usize,
    },
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
    DiskFlushError {
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
        register: GpRegister,
        bit: u8,
    },
    CpuStall {
        target: usize,
        vcpu: usize,
        duration_ticks: u64,
    },
    CpuThrottle {
        target: usize,
        budget_units: u64,
        duration_ticks: u64,
    },
    CustomEffect {
        effect_id: String,
        version: u32,
        parameters: std::collections::BTreeMap<String, String>,
    },
    ThreadPause {
        target: usize,
        thread: chaoscontrol_fault::thread_pause::LogicalThreadId,
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

impl From<&Fault> for SerializableFault {
    fn from(fault: &Fault) -> Self {
        match fault {
            Fault::NetworkPartition { side_a, side_b } => SerializableFault::NetworkPartition {
                side_a: side_a.clone(),
                side_b: side_b.clone(),
            },
            Fault::NetworkLatency { target, latency_ns } => SerializableFault::NetworkLatency {
                target: *target,
                latency_ns: *latency_ns,
            },
            Fault::PacketLoss { target, rate_ppm } => SerializableFault::PacketLoss {
                target: *target,
                rate_ppm: *rate_ppm,
            },
            Fault::PacketCorruption { target, rate_ppm } => SerializableFault::PacketCorruption {
                target: *target,
                rate_ppm: *rate_ppm,
            },
            Fault::PacketReorder { target, window_ns } => SerializableFault::PacketReorder {
                target: *target,
                window_ns: *window_ns,
            },
            Fault::NetworkJitter { target, jitter_ns } => SerializableFault::NetworkJitter {
                target: *target,
                jitter_ns: *jitter_ns,
            },
            Fault::NetworkBandwidth {
                target,
                bytes_per_sec,
            } => SerializableFault::NetworkBandwidth {
                target: *target,
                bytes_per_sec: *bytes_per_sec,
            },
            Fault::PacketDuplicate { target, rate_ppm } => SerializableFault::PacketDuplicate {
                target: *target,
                rate_ppm: *rate_ppm,
            },
            Fault::NetworkHeal => SerializableFault::NetworkHeal,
            Fault::NetworkDirectional {
                node,
                peer,
                direction,
                disruption,
                delay_ns,
                queue_limit,
            } => SerializableFault::NetworkDirectional {
                node: *node,
                peer: *peer,
                direction: *direction,
                disruption: *disruption,
                delay_ns: *delay_ns,
                queue_limit: *queue_limit,
            },
            Fault::NetworkDirectionalHeal { node, peer } => {
                SerializableFault::NetworkDirectionalHeal {
                    node: *node,
                    peer: *peer,
                }
            }
            Fault::DiskReadError { target, offset } => SerializableFault::DiskReadError {
                target: *target,
                offset: *offset,
            },
            Fault::DiskWriteError { target, offset } => SerializableFault::DiskWriteError {
                target: *target,
                offset: *offset,
            },
            Fault::DiskTornWrite {
                target,
                offset,
                bytes_written,
            } => SerializableFault::DiskTornWrite {
                target: *target,
                offset: *offset,
                bytes_written: *bytes_written,
            },
            Fault::DiskCorruption {
                target,
                offset,
                len,
            } => SerializableFault::DiskCorruption {
                target: *target,
                offset: *offset,
                len: *len,
            },
            Fault::DiskFull { target } => SerializableFault::DiskFull { target: *target },
            Fault::ProcessKill { target } => SerializableFault::ProcessKill { target: *target },
            Fault::ProcessRestart { target } => {
                SerializableFault::ProcessRestart { target: *target }
            }
            Fault::ProcessPause {
                target,
                duration_ns,
            } => SerializableFault::ProcessPause {
                target: *target,
                duration_ns: *duration_ns,
            },
            Fault::ClockSkew { target, offset_ns } => SerializableFault::ClockSkew {
                target: *target,
                offset_ns: *offset_ns,
            },
            Fault::ClockJump { target, delta_ns } => SerializableFault::ClockJump {
                target: *target,
                delta_ns: *delta_ns,
            },
            Fault::MemoryPressure {
                target,
                limit_bytes,
                duration_ticks,
            } => SerializableFault::MemoryPressure {
                target: *target,
                limit_bytes: *limit_bytes,
                duration_ticks: *duration_ticks,
            },
            Fault::InjectInterrupt { target, irq } => SerializableFault::InjectInterrupt {
                target: *target,
                irq: *irq,
            },
            Fault::InjectNmi { target, vcpu } => SerializableFault::InjectNmi {
                target: *target,
                vcpu: *vcpu,
            },
            Fault::DiskSlow { target, delay_ns } => SerializableFault::DiskSlow {
                target: *target,
                delay_ns: *delay_ns,
            },
            Fault::DiskFlushError { target, delay_ns } => SerializableFault::DiskFlushError {
                target: *target,
                delay_ns: *delay_ns,
            },
            Fault::DiskFsyncLie { target } => SerializableFault::DiskFsyncLie { target: *target },
            Fault::DiskFsyncFlush { target } => {
                SerializableFault::DiskFsyncFlush { target: *target }
            }
            Fault::DiskPartialRead {
                target,
                offset,
                max_bytes,
            } => SerializableFault::DiskPartialRead {
                target: *target,
                offset: *offset,
                max_bytes: *max_bytes,
            },
            Fault::CpuBitflip {
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
            Fault::CpuStall {
                target,
                vcpu,
                duration_ticks,
            } => SerializableFault::CpuStall {
                target: *target,
                vcpu: *vcpu,
                duration_ticks: *duration_ticks,
            },
            Fault::CpuThrottle {
                target,
                budget_units,
                duration_ticks,
            } => SerializableFault::CpuThrottle {
                target: *target,
                budget_units: *budget_units,
                duration_ticks: *duration_ticks,
            },
            Fault::CustomEffect {
                effect_id,
                version,
                parameters,
            } => SerializableFault::CustomEffect {
                effect_id: effect_id.clone(),
                version: *version,
                parameters: parameters.clone(),
            },
            Fault::ThreadPause {
                target,
                thread,
                duration_ticks,
            } => SerializableFault::ThreadPause {
                target: *target,
                thread: thread.clone(),
                duration_ticks: *duration_ticks,
            },
            Fault::ClockFreeze {
                target,
                duration_ticks,
            } => SerializableFault::ClockFreeze {
                target: *target,
                duration_ticks: *duration_ticks,
            },
            Fault::ClockJitter { target, bound_tsc } => SerializableFault::ClockJitter {
                target: *target,
                bound_tsc: *bound_tsc,
            },
        }
    }
}

impl From<&SerializableFault> for Fault {
    fn from(fault: &SerializableFault) -> Self {
        match fault {
            SerializableFault::NetworkPartition { side_a, side_b } => Fault::NetworkPartition {
                side_a: side_a.clone(),
                side_b: side_b.clone(),
            },
            SerializableFault::NetworkLatency { target, latency_ns } => Fault::NetworkLatency {
                target: *target,
                latency_ns: *latency_ns,
            },
            SerializableFault::PacketLoss { target, rate_ppm } => Fault::PacketLoss {
                target: *target,
                rate_ppm: *rate_ppm,
            },
            SerializableFault::PacketCorruption { target, rate_ppm } => Fault::PacketCorruption {
                target: *target,
                rate_ppm: *rate_ppm,
            },
            SerializableFault::PacketReorder { target, window_ns } => Fault::PacketReorder {
                target: *target,
                window_ns: *window_ns,
            },
            SerializableFault::NetworkJitter { target, jitter_ns } => Fault::NetworkJitter {
                target: *target,
                jitter_ns: *jitter_ns,
            },
            SerializableFault::NetworkBandwidth {
                target,
                bytes_per_sec,
            } => Fault::NetworkBandwidth {
                target: *target,
                bytes_per_sec: *bytes_per_sec,
            },
            SerializableFault::PacketDuplicate { target, rate_ppm } => Fault::PacketDuplicate {
                target: *target,
                rate_ppm: *rate_ppm,
            },
            SerializableFault::NetworkHeal => Fault::NetworkHeal,
            SerializableFault::NetworkDirectional {
                node,
                peer,
                direction,
                disruption,
                delay_ns,
                queue_limit,
            } => Fault::NetworkDirectional {
                node: *node,
                peer: *peer,
                direction: *direction,
                disruption: *disruption,
                delay_ns: *delay_ns,
                queue_limit: *queue_limit,
            },
            SerializableFault::NetworkDirectionalHeal { node, peer } => {
                Fault::NetworkDirectionalHeal {
                    node: *node,
                    peer: *peer,
                }
            }
            SerializableFault::DiskReadError { target, offset } => Fault::DiskReadError {
                target: *target,
                offset: *offset,
            },
            SerializableFault::DiskWriteError { target, offset } => Fault::DiskWriteError {
                target: *target,
                offset: *offset,
            },
            SerializableFault::DiskTornWrite {
                target,
                offset,
                bytes_written,
            } => Fault::DiskTornWrite {
                target: *target,
                offset: *offset,
                bytes_written: *bytes_written,
            },
            SerializableFault::DiskCorruption {
                target,
                offset,
                len,
            } => Fault::DiskCorruption {
                target: *target,
                offset: *offset,
                len: *len,
            },
            SerializableFault::DiskFull { target } => Fault::DiskFull { target: *target },
            SerializableFault::ProcessKill { target } => Fault::ProcessKill { target: *target },
            SerializableFault::ProcessRestart { target } => {
                Fault::ProcessRestart { target: *target }
            }
            SerializableFault::ProcessPause {
                target,
                duration_ns,
            } => Fault::ProcessPause {
                target: *target,
                duration_ns: *duration_ns,
            },
            SerializableFault::ClockSkew { target, offset_ns } => Fault::ClockSkew {
                target: *target,
                offset_ns: *offset_ns,
            },
            SerializableFault::ClockJump { target, delta_ns } => Fault::ClockJump {
                target: *target,
                delta_ns: *delta_ns,
            },
            SerializableFault::MemoryPressure {
                target,
                limit_bytes,
                duration_ticks,
            } => Fault::MemoryPressure {
                target: *target,
                limit_bytes: *limit_bytes,
                duration_ticks: *duration_ticks,
            },
            SerializableFault::InjectInterrupt { target, irq } => Fault::InjectInterrupt {
                target: *target,
                irq: *irq,
            },
            SerializableFault::InjectNmi { target, vcpu } => Fault::InjectNmi {
                target: *target,
                vcpu: *vcpu,
            },
            SerializableFault::DiskSlow { target, delay_ns } => Fault::DiskSlow {
                target: *target,
                delay_ns: *delay_ns,
            },
            SerializableFault::DiskFlushError { target, delay_ns } => Fault::DiskFlushError {
                target: *target,
                delay_ns: *delay_ns,
            },
            SerializableFault::DiskFsyncLie { target } => Fault::DiskFsyncLie { target: *target },
            SerializableFault::DiskFsyncFlush { target } => {
                Fault::DiskFsyncFlush { target: *target }
            }
            SerializableFault::DiskPartialRead {
                target,
                offset,
                max_bytes,
            } => Fault::DiskPartialRead {
                target: *target,
                offset: *offset,
                max_bytes: *max_bytes,
            },
            SerializableFault::CpuBitflip {
                target,
                vcpu,
                register,
                bit,
            } => Fault::CpuBitflip {
                target: *target,
                vcpu: *vcpu,
                register: *register,
                bit: *bit,
            },
            SerializableFault::CpuStall {
                target,
                vcpu,
                duration_ticks,
            } => Fault::CpuStall {
                target: *target,
                vcpu: *vcpu,
                duration_ticks: *duration_ticks,
            },
            SerializableFault::CpuThrottle {
                target,
                budget_units,
                duration_ticks,
            } => Fault::CpuThrottle {
                target: *target,
                budget_units: *budget_units,
                duration_ticks: *duration_ticks,
            },
            SerializableFault::CustomEffect {
                effect_id,
                version,
                parameters,
            } => Fault::CustomEffect {
                effect_id: effect_id.clone(),
                version: *version,
                parameters: parameters.clone(),
            },
            SerializableFault::ThreadPause {
                target,
                thread,
                duration_ticks,
            } => Fault::ThreadPause {
                target: *target,
                thread: thread.clone(),
                duration_ticks: *duration_ticks,
            },
            SerializableFault::ClockFreeze {
                target,
                duration_ticks,
            } => Fault::ClockFreeze {
                target: *target,
                duration_ticks: *duration_ticks,
            },
            SerializableFault::ClockJitter { target, bound_tsc } => Fault::ClockJitter {
                target: *target,
                bound_tsc: *bound_tsc,
            },
        }
    }
}

/// Serializable scheduled fault.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerializableScheduledFault {
    pub time_ns: u64,
    pub fault: SerializableFault,
    pub label: Option<String>,
}

/// Serializable fault schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerializableSchedule {
    pub faults: Vec<SerializableScheduledFault>,
}

impl From<&FaultSchedule> for SerializableSchedule {
    fn from(schedule: &FaultSchedule) -> Self {
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

impl From<&SerializableSchedule> for FaultSchedule {
    fn from(sched: &SerializableSchedule) -> Self {
        let mut schedule = FaultSchedule::new();
        for sf in &sched.faults {
            let mut fault = ScheduledFault::new(sf.time_ns, (&sf.fault).into());
            if let Some(ref label) = sf.label {
                fault = fault.with_label(label.clone());
            }
            schedule.add(fault);
        }
        schedule
    }
}

/// Serializable bug report.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Runtime owner scope, separate from the stable assertion descriptor.
    #[serde(
        default,
        deserialize_with = "chaoscontrol_replay_evidence_core::validate::deserialize_process_instances",
        skip_serializing_if = "std::collections::BTreeSet::is_empty"
    )]
    pub process_instances: std::collections::BTreeSet<String>,
    /// Process-local fallback binding. Its presence is mandatory for fallback descriptors.
    #[serde(default = "no_fallback_scope", skip_serializing_if = "Option::is_none")]
    pub fallback_scope: Option<chaoscontrol_protocol::fallback::FallbackAssertionScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker_context: Option<crate::marker_branching::MarkerFrontierMetadata>,
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
    pub replay_parent_snapshot_ref: Option<ReplayParentSnapshotRef>,
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
    /// Whether the run that found this bug injected seed-derived random
    /// faults. Always written; reproduction and minimization use this value.
    #[serde(default = "legacy_random_faults")]
    pub random_faults: bool,
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
        chaoscontrol_replay_evidence_core::validate::validate_process_instances(
            &self.process_instances,
        )
        .map_err(|_| crate::bug::identity::BugIdentityError::MalformedCarrier)?;
        if let Some(marker) = self.marker_context.as_ref() {
            marker
                .validate()
                .map_err(|_| crate::bug::identity::BugIdentityError::MalformedCarrier)?;
            if self.replay_parent_depth == 0
                || self.replay_parent_snapshot_ref.is_none()
                || marker.observed_tick > self.tick
            {
                return Err(crate::bug::identity::BugIdentityError::MalformedCarrier);
            }
        }
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
    let facts = chaoscontrol_fault::oracle_validation::validate_oracle_report_claim(report)
        .map_err(|_| BugSetIdentityError {
            bug_id: first_bug.bug_id,
            source: crate::bug::identity::BugIdentityError::ReportMismatch,
        })?;
    let mut ids = std::collections::BTreeSet::new();
    for bug in bugs {
        if !ids.insert(bug.bug_id) {
            return Err(BugSetIdentityError {
                bug_id: bug.bug_id,
                source: crate::bug::identity::BugIdentityError::Duplicate,
            });
        }
        BugReport::try_from(bug).map_err(|source| BugSetIdentityError {
            bug_id: bug.bug_id,
            source,
        })?;
        let identity = crate::bug::identity::validate_carrier(
            bug.assertion_id,
            bug.assertion_identity.as_ref(),
        )
        .map_err(|source| BugSetIdentityError {
            bug_id: bug.bug_id,
            source,
        })?;
        let record =
            chaoscontrol_fault::oracle_validation::resolve_assertion_evidence_prevalidated(
                report, &facts, identity,
            )
            .map_err(|_| BugSetIdentityError {
                bug_id: bug.bug_id,
                source: crate::bug::identity::BugIdentityError::ReportMismatch,
            })?;
        if record.process_instances != bug.process_instances
            || record.fallback_scope != bug.fallback_scope
        {
            return Err(BugSetIdentityError {
                bug_id: bug.bug_id,
                source: crate::bug::identity::BugIdentityError::ReportMismatch,
            });
        }
    }
    Ok(())
}

pub fn replay_bug_set(
    bugs: &[SerializableBug],
    assertion_report: Option<&chaoscontrol_fault::oracle::OracleReport>,
) -> Result<Vec<BugReport>, BugSetIdentityError> {
    validate_bug_set(bugs, assertion_report)?;
    bugs.iter()
        .map(|bug| {
            BugReport::try_from(bug).map_err(|source| BugSetIdentityError {
                bug_id: bug.bug_id,
                source,
            })
        })
        .collect()
}

impl From<&BugReport> for SerializableBug {
    fn from(bug: &BugReport) -> Self {
        SerializableBug {
            bug_id: bug.bug_id,
            assertion_id: bug.assertion_id,
            assertion_identity: Some(bug.assertion_identity.clone()),
            fallback_scope: bug.fallback_scope.clone(),
            process_instances: bug.process_instances.clone(),
            marker_context: bug.marker_context.clone(),
            assertion_location: bug.assertion_location.clone(),
            schedule: (&bug.schedule).into(),
            tick: bug.tick,
            replay_parent_depth: bug.replay_parent_depth,
            replay_parent_snapshot_ref: bug.replay_parent_snapshot_ref.clone(),
            dedup_key: Some(bug.dedup_key),
            schedule_variant: bug.schedule_variant.clone(),
            scenario_config: bug.scenario_config.clone(),
            scenario_summary: bug.scenario_summary.clone(),
            random_faults: bug.random_faults,
        }
    }
}

impl TryFrom<&SerializableBug> for BugReport {
    type Error = crate::bug::identity::BugIdentityError;

    fn try_from(bug: &SerializableBug) -> Result<Self, Self::Error> {
        let assertion_identity = bug.require_replay_identity()?.clone();
        if bug.assertion_location.is_empty()
            || bug.schedule.faults.len() > MAX_SERIALIZABLE_FAULTS
            || (bug.replay_parent_depth > 0) != bug.replay_parent_snapshot_ref.is_some()
        {
            return Err(crate::bug::identity::BugIdentityError::MalformedCarrier);
        }
        if let Some(reference) = &bug.replay_parent_snapshot_ref {
            crate::snapshot_store::validate_ref_shape(reference)
                .map_err(|_| crate::bug::identity::BugIdentityError::MalformedCarrier)?;
        }
        Ok(Self {
            bug_id: bug.bug_id,
            assertion_id: bug.assertion_id,
            assertion_identity,
            fallback_scope: bug.fallback_scope.clone(),
            process_instances: bug.process_instances.clone(),
            marker_context: bug.marker_context.clone(),
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
            random_faults: bug.random_faults,
        })
    }
}

/// Complete checkpoint — everything needed to resume exploration.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Campaign journal segment this checkpoint was committed against.
    /// Omitted when absent so legacy checkpoints stay byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub campaign_journal: Option<CampaignJournalBinding>,
    /// False once any round ran under the legacy-diagnostic frontier policy,
    /// including Campaign rounds resumed from such a checkpoint. Serialized
    /// only when false, so eligible checkpoints keep their shape.
    #[serde(
        default = "release_eligible_by_default",
        skip_serializing_if = "is_release_eligible"
    )]
    pub release_eligible: bool,
    /// Campaign-persistent assertion-hit selector state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion_checkpoint_selector: Option<AssertionCheckpointSelectorState>,
}

const fn release_eligible_by_default() -> bool {
    true
}

/// `skip_serializing_if` predicate (serde passes a reference).
const fn is_release_eligible(release_eligible: &bool) -> bool {
    *release_eligible
}

/// Save a checkpoint to a JSON file.
pub fn save_checkpoint<P: AsRef<Path>>(
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
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn write_bug_artifact(
    path: &Path,
    bytes: &[u8],
    overwrite: bool,
) -> Result<(), CheckpointBugExportError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let directory = fs::File::open(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if overwrite {
        temporary.persist(path).map_err(|error| error.error)?;
    } else {
        temporary
            .persist_noclobber(path)
            .map_err(|error| error.error)?;
    }
    directory.sync_all()?;
    Ok(())
}

/// Load a checkpoint from a JSON file.
pub fn load_checkpoint<P: AsRef<Path>>(path: P) -> Result<ExplorationCheckpoint, CheckpointError> {
    let json = crate::bounded_json::read_checkpoint(path.as_ref())?;
    let checkpoint = serde_json::from_str(&json)?;
    validate_checkpoint(&checkpoint)?;
    Ok(checkpoint)
}

pub fn load_serializable_bug<P: AsRef<Path>>(path: P) -> Result<SerializableBug, CheckpointError> {
    load_serializable_bug_artifact(path).map(|(bug, _bytes)| bug)
}

pub fn load_serializable_bug_artifact<P: AsRef<Path>>(
    path: P,
) -> Result<(SerializableBug, Vec<u8>), CheckpointError> {
    let json = crate::bounded_json::read_checkpoint(path.as_ref())?;
    let bug: SerializableBug = serde_json::from_str(&json)?;
    BugReport::try_from(&bug).map_err(|source| CheckpointError::InvalidBugIdentity {
        source: BugSetIdentityError {
            bug_id: bug.bug_id,
            source,
        },
    })?;
    Ok((bug, json.into_bytes()))
}

fn validate_checkpoint(checkpoint: &ExplorationCheckpoint) -> Result<(), CheckpointError> {
    if checkpoint.global_coverage.len() > crate::coverage::MAP_SIZE {
        return Err(CheckpointError::InvalidBounds {
            reason: "global coverage exceeds the bitmap size".to_string(),
        });
    }
    // r[impl chaoscontrol.agent_investigation.checkpoint_smp_topology_bounds]
    if checkpoint.config.num_vcpus == 0
        || checkpoint.config.num_vcpus > chaoscontrol_vmm::scheduler::core::MAX_SCHEDULE_VCPUS
    {
        return Err(CheckpointError::InvalidBounds {
            reason: "vCPU count is outside the supported SMP range".to_string(),
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
    if let Some(state) = &checkpoint.assertion_checkpoint_selector {
        state
            .validate()
            .map_err(|error| CheckpointError::InvalidBounds {
                reason: format!("invalid assertion checkpoint selector state: {error}"),
            })?;
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
    if let Some(binding) = checkpoint.campaign_journal.as_ref() {
        binding.digest_bytes()?;
    }
    // r[impl chaoscontrol.campaign_policy.rollback]
    if checkpoint.config.frontier_policy.is_legacy_diagnostic() && checkpoint.release_eligible {
        return Err(CheckpointError::DiagnosticClaimsReleaseEligibility);
    }
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
    AlreadyExists { path: PathBuf },

    #[snafu(display("invalid tick range: min {min_tick} exceeds max {max_tick}"))]
    InvalidTickRange { min_tick: u64, max_tick: u64 },

    #[snafu(display("invalid assertion fingerprint: {value}"))]
    InvalidAssertionFingerprint { value: String },

    #[snafu(display("no checkpoint bug matches {selector}"))]
    BugNotFound { selector: String },

    #[snafu(display("{matches} checkpoint bugs match {selector}"))]
    BugAmbiguous { selector: String, matches: usize },

    #[snafu(display(
        "exactly one of index, dedup key, or assertion fingerprint must be provided"
    ))]
    BugSelector,

    #[snafu(display("checkpoint digest mismatch: expected {expected}, got {actual}"))]
    CheckpointDigestMismatch { expected: String, actual: String },

    #[snafu(display("requested tick {requested} differs from the failure tick {actual}"))]
    MomentTickMismatch { requested: u64, actual: u64 },

    #[snafu(display("snapshot reference conflict for path {path}"))]
    SnapshotReferenceConflict { path: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckpointBugExportFilter {
    pub assertion_id: Option<u64>,
    pub min_replay_parent_depth: Option<u32>,
    /// Exact failure identity; legacy bugs without a stored key never match.
    pub dedup_key: Option<u64>,
    pub max_bugs: Option<usize>,
}

impl CheckpointBugExportFilter {
    // r[impl chaoscontrol.agent_investigation.export_by_dedup_key]
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
        if let Some(dedup_key) = self.dedup_key {
            if bug.dedup_key != Some(dedup_key) {
                return false;
            }
        }
        true
    }
}

/// One checkpoint-held failure surfaced for agent inspection. Legacy bugs keep
/// their identity fields optional; nothing here is replay evidence.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BugListEntry {
    /// Position of the bug in the checkpoint, matching `bug_N.json` numbering.
    pub checkpoint_index: usize,
    pub artifact: String,
    pub bug_id: u64,
    pub assertion_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedup_key: Option<u64>,
    pub tick: u64,
    pub replay_parent_depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_parent_snapshot_digest: Option<String>,
    pub has_assertion_identity: bool,
    pub assertion_location: String,
    #[serde(skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub process_instances: std::collections::BTreeSet<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BugListFilter {
    pub assertion_id: Option<u64>,
    pub dedup_key: Option<u64>,
    pub min_tick: Option<u64>,
    pub max_tick: Option<u64>,
    /// Only bugs whose runtime owner scope includes this process instance.
    pub process_instance: Option<String>,
    /// Only bugs carrying an admitted assertion identity.
    pub require_assertion_identity: bool,
    /// Only bugs whose admitted assertion fingerprint matches this lower-hex value.
    pub assertion_fingerprint: Option<String>,
}

impl BugListFilter {
    // r[impl chaoscontrol.agent_investigation.list_bugs_owner_identity_filter]
    fn matches(&self, bug: &SerializableBug) -> bool {
        if let Some(assertion_id) = self.assertion_id {
            if bug.assertion_id != assertion_id {
                return false;
            }
        }
        if let Some(dedup_key) = self.dedup_key {
            if bug.dedup_key != Some(dedup_key) {
                return false;
            }
        }
        if let Some(min_tick) = self.min_tick {
            if bug.tick < min_tick {
                return false;
            }
        }
        if let Some(max_tick) = self.max_tick {
            if bug.tick > max_tick {
                return false;
            }
        }
        if let Some(process_instance) = self.process_instance.as_deref() {
            if !bug
                .process_instances
                .iter()
                .any(|owner| owner == process_instance)
            {
                return false;
            }
        }
        if self.require_assertion_identity && bug.assertion_identity.is_none() {
            return false;
        }
        // r[impl chaoscontrol.agent_investigation.list_bugs_assertion_fingerprint_filter]
        if let Some(expected) = self.assertion_fingerprint.as_deref() {
            let actual = bug
                .assertion_identity
                .as_ref()
                .map(|identity| identity.fingerprint.to_hex());
            if actual.as_deref() != Some(expected) {
                return false;
            }
        }
        true
    }
}

/// List admitted checkpoint-held failures without writing artifacts or loading
/// snapshot bytes. Order is the checkpoint's own bug order.
// r[impl chaoscontrol.agent_investigation.locate_bugs]
pub fn list_bugs_with_filter<P: AsRef<Path>>(
    checkpoint_path: P,
    filter: BugListFilter,
) -> Result<Vec<BugListEntry>, CheckpointBugExportError> {
    if let (Some(min_tick), Some(max_tick)) = (filter.min_tick, filter.max_tick) {
        if min_tick > max_tick {
            return Err(CheckpointBugExportError::InvalidTickRange { min_tick, max_tick });
        }
    }
    if let Some(value) = filter.assertion_fingerprint.as_deref() {
        chaoscontrol_protocol::identity::AssertionFingerprint::from_hex(value).map_err(|_| {
            CheckpointBugExportError::InvalidAssertionFingerprint {
                value: value.to_string(),
            }
        })?;
    }
    let checkpoint = load_checkpoint(checkpoint_path)?;
    validate_bug_set(&checkpoint.bugs, checkpoint.assertion_report.as_ref()).map_err(|error| {
        CheckpointBugExportError::InvalidAssertionIdentity {
            bug_id: error.bug_id,
            reason: error.source.to_string(),
        }
    })?;
    Ok(checkpoint
        .bugs
        .iter()
        .enumerate()
        .filter(|(_, bug)| filter.matches(bug))
        .map(|(index, bug)| BugListEntry {
            checkpoint_index: index,
            artifact: format!("bug_{index}.json"),
            bug_id: bug.bug_id,
            assertion_id: bug.assertion_id,
            dedup_key: bug.dedup_key,
            tick: bug.tick,
            replay_parent_depth: bug.replay_parent_depth,
            replay_parent_snapshot_digest: bug
                .replay_parent_snapshot_ref
                .as_ref()
                .map(|reference| reference.digest.clone()),
            has_assertion_identity: bug.assertion_identity.is_some(),
            assertion_location: bug.assertion_location.clone(),
            process_instances: bug.process_instances.clone(),
        })
        .collect())
}

/// Select exactly one checkpoint bug for read-only inspection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BugSelector {
    pub index: Option<usize>,
    pub dedup_key: Option<u64>,
}

// r[impl chaoscontrol.agent_investigation.selector_assertion_fingerprint]
/// Resolve exactly one failure from an index, dedup key, or exact assertion
/// fingerprint. The resolver is read-only and does not write artifacts.
pub fn resolve_bug_selector<P: AsRef<Path>>(
    checkpoint_path: P,
    index: Option<usize>,
    dedup_key: Option<u64>,
    assertion_fingerprint: Option<&str>,
) -> Result<BugSelector, CheckpointBugExportError> {
    let provided = usize::from(index.is_some())
        + usize::from(dedup_key.is_some())
        + usize::from(assertion_fingerprint.is_some());
    if provided != 1 {
        return Err(CheckpointBugExportError::BugSelector);
    }
    let checkpoint_path = checkpoint_path.as_ref();
    if let Some(index) = index {
        return Ok(BugSelector {
            index: Some(index),
            dedup_key: None,
        });
    }
    if let Some(dedup_key) = dedup_key {
        return Ok(BugSelector {
            index: None,
            dedup_key: Some(dedup_key),
        });
    }
    let fingerprint = assertion_fingerprint.expect("exactly one selector is present");
    let entries = list_bugs_with_filter(
        checkpoint_path,
        BugListFilter {
            assertion_fingerprint: Some(fingerprint.to_string()),
            require_assertion_identity: true,
            ..BugListFilter::default()
        },
    )?;
    match entries.as_slice() {
        [entry] => Ok(BugSelector {
            index: Some(entry.checkpoint_index),
            dedup_key: None,
        }),
        [] => Err(CheckpointBugExportError::BugNotFound {
            selector: format!("assertion fingerprint {fingerprint}"),
        }),
        _ => Err(CheckpointBugExportError::BugAmbiguous {
            selector: format!("assertion fingerprint {fingerprint}"),
            matches: entries.len(),
        }),
    }
}

/// Versioned read-only inspection record. It carries the validated bug report as
/// stored; it is not replay evidence and does not imply the snapshot is replayable.
#[derive(Debug, Clone, Serialize)]
pub struct BugInspection {
    pub schema: &'static str,
    pub checkpoint_index: usize,
    pub artifact: String,
    pub bug: SerializableBug,
    pub non_claims: Vec<&'static str>,
}

// r[impl chaoscontrol.agent_investigation.inspect_bug]
pub fn inspect_bug<P: AsRef<Path>>(
    checkpoint_path: P,
    selector: BugSelector,
) -> Result<BugInspection, CheckpointBugExportError> {
    if selector.index.is_some() == selector.dedup_key.is_some() {
        return Err(CheckpointBugExportError::BugSelector);
    }
    let checkpoint = load_checkpoint(checkpoint_path)?;
    validate_bug_set(&checkpoint.bugs, checkpoint.assertion_report.as_ref()).map_err(|error| {
        CheckpointBugExportError::InvalidAssertionIdentity {
            bug_id: error.bug_id,
            reason: error.source.to_string(),
        }
    })?;
    let describe = match (selector.index, selector.dedup_key) {
        (Some(index), _) => format!("index {index}"),
        (None, Some(key)) => format!("dedup key {key}"),
        (None, None) => "no selector".to_string(),
    };
    let matches: Vec<(usize, &SerializableBug)> = checkpoint
        .bugs
        .iter()
        .enumerate()
        .filter(|(index, bug)| match (selector.index, selector.dedup_key) {
            (Some(wanted), _) => *index == wanted,
            (None, Some(key)) => bug.dedup_key == Some(key),
            (None, None) => false,
        })
        .collect();
    match matches.as_slice() {
        [] => Err(CheckpointBugExportError::BugNotFound { selector: describe }),
        [(index, bug)] => Ok(BugInspection {
            schema: "chaoscontrol.bug_inspection.v1",
            checkpoint_index: *index,
            artifact: format!("bug_{index}.json"),
            bug: (*bug).clone(),
            non_claims: vec![
                "A stored bug report is not replay evidence or an accepted verdict.",
                "Inspection does not prove the referenced snapshot is admissible or reproducible.",
                "Schedule bytes are the recorded plan, not evidence that a run followed them.",
            ],
        }),
        many => Err(CheckpointBugExportError::BugAmbiguous {
            selector: describe,
            matches: many.len(),
        }),
    }
}

/// One integrity problem found while admitting a retained checkpoint.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CheckpointVerificationIssue {
    pub checkpoint_index: usize,
    pub bug_id: u64,
    pub reason: String,
}

/// Admission depth. Structure-only admits the checkpoint record without reading
/// snapshot artifacts, which is useful when only the failure index is local.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointAdmissionMode {
    Full,
    StructureOnly,
}

/// Versioned integrity record for a retained checkpoint. It proves structural
/// admission and, in full mode, referenced snapshot digests; it is not a replay
/// verdict.
#[derive(Debug, Clone, Serialize)]
pub struct CheckpointVerification {
    pub schema: &'static str,
    pub mode: CheckpointAdmissionMode,
    pub checkpoint_digest: String,
    pub bugs_total: usize,
    pub bugs_with_snapshot_ref: usize,
    pub snapshots_validated: usize,
    pub snapshots_checked: bool,
    pub issues: Vec<CheckpointVerificationIssue>,
    pub valid: bool,
    pub non_claims: Vec<&'static str>,
}

// r[impl chaoscontrol.agent_investigation.verify_checkpoint]
pub fn verify_checkpoint<P: AsRef<Path>>(
    checkpoint_path: P,
) -> Result<CheckpointVerification, CheckpointBugExportError> {
    verify_checkpoint_with_options(checkpoint_path, None, CheckpointAdmissionMode::Full)
}

// r[impl chaoscontrol.agent_investigation.session_digest_binding]
pub fn verify_checkpoint_with_digest<P: AsRef<Path>>(
    checkpoint_path: P,
    expected_digest: Option<&str>,
) -> Result<CheckpointVerification, CheckpointBugExportError> {
    verify_checkpoint_with_options(
        checkpoint_path,
        expected_digest,
        CheckpointAdmissionMode::Full,
    )
}

// r[impl chaoscontrol.agent_investigation.admission_modes]
pub fn verify_checkpoint_with_options<P: AsRef<Path>>(
    checkpoint_path: P,
    expected_digest: Option<&str>,
    mode: CheckpointAdmissionMode,
) -> Result<CheckpointVerification, CheckpointBugExportError> {
    let checkpoint_path = checkpoint_path.as_ref();
    let bytes = crate::bounded_json::read_checkpoint(checkpoint_path)?;
    let checkpoint_digest = format!("b3:{}", blake3::hash(bytes.as_bytes()).to_hex());
    if let Some(expected) = expected_digest {
        if expected != checkpoint_digest {
            return Err(CheckpointBugExportError::CheckpointDigestMismatch {
                expected: expected.to_string(),
                actual: checkpoint_digest,
            });
        }
    }
    let checkpoint = load_checkpoint(checkpoint_path)?;
    let source_dir = checkpoint_path.parent().unwrap_or_else(|| Path::new("."));
    let store = FileSnapshotStore::new(source_dir);

    let mut issues = Vec::new();
    let mut bugs_with_snapshot_ref = 0;
    let mut snapshots_validated = 0;
    // Bugs from the same branch share one snapshot artifact; validate each
    // unique reference once instead of re-reading the bytes per bug.
    let mut artifacts: std::collections::HashMap<&str, Result<u32, String>> =
        std::collections::HashMap::new();
    for (index, bug) in checkpoint.bugs.iter().enumerate() {
        if mode == CheckpointAdmissionMode::StructureOnly {
            if bug.replay_parent_snapshot_ref.is_some() {
                bugs_with_snapshot_ref += 1;
            }
            continue;
        }
        let Some(reference) = bug.replay_parent_snapshot_ref.as_ref() else {
            continue;
        };
        bugs_with_snapshot_ref += 1;
        let stored = artifacts.entry(reference.path.as_str()).or_insert_with(|| {
            store
                .get_snapshot_artifact(reference)
                .map(|artifact| artifact.replay_parent_depth)
                .map_err(|error| error.to_string())
        });
        match stored {
            Ok(depth) if *depth == bug.replay_parent_depth => {
                snapshots_validated += 1;
            }
            Ok(depth) => issues.push(CheckpointVerificationIssue {
                checkpoint_index: index,
                bug_id: bug.bug_id,
                reason: format!(
                    "snapshot depth mismatch: artifact {depth} vs bug {}",
                    bug.replay_parent_depth
                ),
            }),
            Err(error) => issues.push(CheckpointVerificationIssue {
                checkpoint_index: index,
                bug_id: bug.bug_id,
                reason: error.clone(),
            }),
        }
    }

    let non_claims = if mode == CheckpointAdmissionMode::Full {
        vec![
            "Admission proves structural validity and referenced snapshot digests only.",
            "A valid checkpoint is not replay evidence, an accepted verdict, or workload proof.",
            "Missing or invalid snapshot artifacts fail admission rather than being repaired.",
        ]
    } else {
        vec![
            "Structure-only admission does not read or validate snapshot artifacts.",
            "A structurally valid checkpoint is not replay evidence, an accepted verdict, or workload proof.",
        ]
    };
    Ok(CheckpointVerification {
        schema: "chaoscontrol.checkpoint_verification.v1",
        mode,
        checkpoint_digest,
        bugs_total: checkpoint.bugs.len(),
        bugs_with_snapshot_ref,
        snapshots_validated,
        snapshots_checked: mode == CheckpointAdmissionMode::Full,
        valid: issues.is_empty(),
        issues,
        non_claims,
    })
}

/// Stable read-only reference to one failure moment within one retained session.
/// It binds the checkpoint digest and bug identity to the recorded tick and
/// snapshot reference; it is not replay evidence.
#[derive(Debug, Clone, Serialize)]
pub struct MomentReference {
    pub schema: &'static str,
    pub checkpoint_digest: String,
    pub checkpoint_index: usize,
    pub artifact: String,
    pub bug_id: u64,
    pub assertion_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedup_key: Option<u64>,
    pub tick: u64,
    pub replay_parent_depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_digest: Option<String>,
    pub branch_id: String,
    pub seek_granularity: &'static str,
    pub has_assertion_identity: bool,
    pub non_claims: Vec<&'static str>,
}

// r[impl chaoscontrol.agent_investigation.moment_reference]
pub fn bind_moment<P: AsRef<Path>>(
    checkpoint_path: P,
    selector: BugSelector,
    requested_tick: Option<u64>,
) -> Result<MomentReference, CheckpointBugExportError> {
    let checkpoint_path = checkpoint_path.as_ref();
    let bytes = crate::bounded_json::read_checkpoint(checkpoint_path)?;
    let checkpoint_digest = format!("b3:{}", blake3::hash(bytes.as_bytes()).to_hex());
    let inspection = inspect_bug(checkpoint_path, selector)?;
    if let Some(requested) = requested_tick {
        if requested != inspection.bug.tick {
            return Err(CheckpointBugExportError::MomentTickMismatch {
                requested,
                actual: inspection.bug.tick,
            });
        }
    }
    let snapshot_digest = match inspection.bug.replay_parent_snapshot_ref.as_ref() {
        Some(reference) => {
            let source_dir = checkpoint_path.parent().unwrap_or_else(|| Path::new("."));
            FileSnapshotStore::new(source_dir)
                .get_snapshot_artifact(reference)
                .map_err(|source| CheckpointBugExportError::SnapshotStore { source })?;
            Some(reference.digest.clone())
        }
        None => None,
    };
    // r[impl chaoscontrol.agent_investigation.moment_reference_branch_granularity]
    let branch_id = branch_identity(
        &checkpoint_digest,
        snapshot_digest.as_deref(),
        inspection.checkpoint_index,
        inspection.bug.tick,
        inspection.bug.dedup_key,
        inspection.bug.replay_parent_depth,
    );
    Ok(MomentReference {
        schema: "chaoscontrol.moment_reference.v2",
        checkpoint_digest,
        checkpoint_index: inspection.checkpoint_index,
        artifact: inspection.artifact,
        bug_id: inspection.bug.bug_id,
        assertion_id: inspection.bug.assertion_id,
        dedup_key: inspection.bug.dedup_key,
        tick: inspection.bug.tick,
        replay_parent_depth: inspection.bug.replay_parent_depth,
        snapshot_digest,
        branch_id,
        seek_granularity: "tick",
        has_assertion_identity: inspection.bug.assertion_identity.is_some(),
        non_claims: vec![
            "A moment reference identifies recorded evidence; it is not replay evidence.",
            "A verified snapshot digest does not prove admissibility or reproducibility.",
            "The tick is the recorded failure tick, not an arbitrary seek position.",
        ],
    })
}

/// Read-only summary of the replay parent snapshot referenced by one failure.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotInspection {
    pub schema: &'static str,
    pub checkpoint_digest: String,
    pub checkpoint_index: usize,
    pub artifact: String,
    pub snapshot_digest: String,
    pub tick: u64,
    pub vm_count: usize,
    pub vcpu_stall_entries: usize,
    pub cpu_throttle_entries: usize,
    pub clock_freeze_entries: usize,
    pub memory_pressure_entries: usize,
    pub pending_process_observations: usize,
    pub fault_operation_sequence: u64,
    pub non_claims: Vec<&'static str>,
}

// r[impl chaoscontrol.agent_investigation.inspect_snapshot]
pub fn inspect_snapshot<P: AsRef<Path>>(
    checkpoint_path: P,
    selector: BugSelector,
) -> Result<SnapshotInspection, CheckpointBugExportError> {
    let checkpoint_path = checkpoint_path.as_ref();
    let checkpoint_digest = format!(
        "b3:{}",
        blake3::hash(crate::bounded_json::read_checkpoint(checkpoint_path)?.as_bytes()).to_hex()
    );
    let inspection = inspect_bug(checkpoint_path, selector)?;
    let reference = inspection
        .bug
        .replay_parent_snapshot_ref
        .as_ref()
        .ok_or_else(|| CheckpointBugExportError::BugNotFound {
            selector: "a bug with a replay parent snapshot".to_string(),
        })?;
    let source_dir = checkpoint_path.parent().unwrap_or_else(|| Path::new("."));
    let artifact = FileSnapshotStore::new(source_dir)
        .get_snapshot_artifact(reference)
        .map_err(|source| CheckpointBugExportError::SnapshotStore { source })?;
    if artifact.replay_parent_depth != inspection.bug.replay_parent_depth {
        return Err(CheckpointBugExportError::SnapshotStore {
            source: crate::snapshot_store::SnapshotStoreError::MetadataMismatch {
                field: "replay_parent_depth",
            },
        });
    }
    let snapshot = artifact.snapshot;
    Ok(SnapshotInspection {
        schema: "chaoscontrol.snapshot_inspection.v1",
        checkpoint_digest,
        checkpoint_index: inspection.checkpoint_index,
        artifact: inspection.artifact,
        snapshot_digest: reference.digest.clone(),
        tick: snapshot.tick,
        vm_count: snapshot.vm_snapshots.len(),
        vcpu_stall_entries: snapshot.vcpu_stall_until.len(),
        cpu_throttle_entries: snapshot.cpu_throttle.len(),
        clock_freeze_entries: snapshot.clock_freeze.len(),
        memory_pressure_entries: snapshot.memory_pressure.len(),
        pending_process_observations: snapshot.pending_process_observations.len(),
        fault_operation_sequence: snapshot.fault_operation_sequence,
        non_claims: vec![
            "Decoding a snapshot artifact is not VM execution, replay, or an accepted verdict.",
            "Snapshot summaries prove content identity and shape only.",
            "A stored parent may still be inadmissible for a changed configuration or artifact set.",
        ],
    })
}

/// One unique replay-parent snapshot referenced by a checkpoint.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SnapshotReferenceSummary {
    pub digest: String,
    pub path: String,
    pub store: String,
    pub codec: String,
    pub replay_parent_depth: u32,
    pub bug_indices: Vec<usize>,
    pub artifacts: Vec<String>,
}

/// Versioned listing of unique snapshot references without reading artifacts.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotReferenceList {
    pub schema: &'static str,
    pub checkpoint_digest: String,
    pub snapshots: Vec<SnapshotReferenceSummary>,
    pub non_claims: Vec<&'static str>,
}

// r[impl chaoscontrol.agent_investigation.list_snapshots]
pub fn list_snapshot_references<P: AsRef<Path>>(
    checkpoint_path: P,
) -> Result<SnapshotReferenceList, CheckpointBugExportError> {
    let checkpoint_path = checkpoint_path.as_ref();
    let checkpoint_digest = format!(
        "b3:{}",
        blake3::hash(crate::bounded_json::read_checkpoint(checkpoint_path)?.as_bytes()).to_hex()
    );
    let checkpoint = load_checkpoint(checkpoint_path)?;
    validate_bug_set(&checkpoint.bugs, checkpoint.assertion_report.as_ref()).map_err(|error| {
        CheckpointBugExportError::InvalidAssertionIdentity {
            bug_id: error.bug_id,
            reason: error.source.to_string(),
        }
    })?;
    let mut groups: Vec<SnapshotReferenceSummary> = Vec::new();
    for (index, bug) in checkpoint.bugs.iter().enumerate() {
        let Some(reference) = bug.replay_parent_snapshot_ref.as_ref() else {
            continue;
        };
        match groups.iter_mut().find(|group| group.path == reference.path) {
            Some(group) => {
                if group.digest != reference.digest
                    || group.replay_parent_depth != bug.replay_parent_depth
                {
                    return Err(CheckpointBugExportError::SnapshotReferenceConflict {
                        path: reference.path.clone(),
                    });
                }
                group.bug_indices.push(index);
                group.artifacts.push(format!("bug_{index}.json"));
            }
            None => groups.push(SnapshotReferenceSummary {
                digest: reference.digest.clone(),
                path: reference.path.clone(),
                store: reference.store.clone(),
                codec: reference.codec.clone(),
                replay_parent_depth: bug.replay_parent_depth,
                bug_indices: vec![index],
                artifacts: vec![format!("bug_{index}.json")],
            }),
        }
    }
    Ok(SnapshotReferenceList {
        schema: "chaoscontrol.snapshot_references.v1",
        checkpoint_digest,
        snapshots: groups,
        non_claims: vec![
            "References are listed from checkpoint metadata; artifacts are not read.",
            "Presence here is not snapshot admission or replay evidence.",
        ],
    })
}

/// Read-only plan for restoring one failure's parent. It checks declared inputs and
/// the snapshot artifact path; it does not execute a VM or prove replay.
#[derive(Debug, Clone, Serialize)]
pub struct RestorePlan {
    pub schema: &'static str,
    pub checkpoint_digest: String,
    pub checkpoint_index: usize,
    pub artifact: String,
    pub bug_id: u64,
    pub assertion_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedup_key: Option<u64>,
    pub tick: u64,
    pub replay_parent_depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_digest: Option<String>,
    pub snapshot_present: bool,
    pub snapshot_verified: bool,
    /// Deterministic disposable-branch identity binding the session digest and
    /// selected parent. Identity only; it does not create or isolate a branch.
    pub branch_id: String,
    pub kernel_path: String,
    pub kernel_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initrd_path: Option<String>,
    pub initrd_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_image_path: Option<String>,
    pub disk_image_present: Option<bool>,
    pub ready: bool,
    pub missing: Vec<String>,
    pub non_claims: Vec<&'static str>,
}

/// Deterministic identity for a disposable branch rooted at one failure moment.
/// The encoding is length-delimited so distinct field tuples cannot collide.
// r[impl chaoscontrol.agent_investigation.branch_identity]
pub fn branch_identity(
    checkpoint_digest: &str,
    snapshot_digest: Option<&str>,
    checkpoint_index: usize,
    tick: u64,
    dedup_key: Option<u64>,
    replay_parent_depth: u32,
) -> String {
    let mut hasher = blake3::Hasher::new();
    let fields = [
        "chaoscontrol.branch_identity.v1".to_string(),
        checkpoint_digest.to_string(),
        snapshot_digest.unwrap_or("none").to_string(),
        checkpoint_index.to_string(),
        tick.to_string(),
        dedup_key.map_or_else(|| "none".to_string(), |key| key.to_string()),
        replay_parent_depth.to_string(),
    ];
    for field in fields {
        hasher.update(&(field.len() as u64).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    format!("b3:{}", hasher.finalize().to_hex())
}

// r[impl chaoscontrol.agent_investigation.restore_plan]
pub fn plan_restore<P: AsRef<Path>>(
    checkpoint_path: P,
    selector: BugSelector,
) -> Result<RestorePlan, CheckpointBugExportError> {
    plan_restore_with_snapshot_check(checkpoint_path, selector, false)
}

// r[impl chaoscontrol.agent_investigation.restore_plan_verification]
pub fn plan_restore_with_snapshot_check<P: AsRef<Path>>(
    checkpoint_path: P,
    selector: BugSelector,
    verify_snapshot: bool,
) -> Result<RestorePlan, CheckpointBugExportError> {
    let checkpoint_path = checkpoint_path.as_ref();
    let bytes = crate::bounded_json::read_checkpoint(checkpoint_path)?;
    let checkpoint_digest = format!("b3:{}", blake3::hash(bytes.as_bytes()).to_hex());
    let checkpoint = load_checkpoint(checkpoint_path)?;
    let inspection = inspect_bug(checkpoint_path, selector)?;
    let reference = inspection.bug.replay_parent_snapshot_ref.as_ref();
    let source_dir = checkpoint_path.parent().unwrap_or_else(|| Path::new("."));
    let snapshot_present = reference
        .map(|reference| source_dir.join(&reference.path).exists())
        .unwrap_or(false);
    let mut snapshot_verified = false;
    if verify_snapshot {
        let reference = reference.ok_or_else(|| CheckpointBugExportError::BugNotFound {
            selector: "a bug with a replay parent snapshot".to_string(),
        })?;
        let artifact = FileSnapshotStore::new(source_dir)
            .get_snapshot_artifact(reference)
            .map_err(|source| CheckpointBugExportError::SnapshotStore { source })?;
        if artifact.replay_parent_depth != inspection.bug.replay_parent_depth {
            return Err(CheckpointBugExportError::SnapshotStore {
                source: crate::snapshot_store::SnapshotStoreError::MetadataMismatch {
                    field: "replay_parent_depth",
                },
            });
        }
        snapshot_verified = true;
    }
    let kernel_present = Path::new(&checkpoint.config.kernel_path).exists();
    let initrd_present = checkpoint
        .config
        .initrd_path
        .as_deref()
        .map(|path| Path::new(path).exists());
    let disk_image_present = checkpoint
        .config
        .disk_image_path
        .as_deref()
        .map(|path| Path::new(path).exists());

    let mut missing = Vec::new();
    match reference {
        None => missing.push("snapshot reference".to_string()),
        Some(reference) if !snapshot_present => {
            missing.push(format!("snapshot {}", reference.path));
        }
        Some(_) => {}
    }
    if !kernel_present {
        missing.push(format!("kernel {}", checkpoint.config.kernel_path));
    }
    if let (Some(path), Some(false)) = (&checkpoint.config.initrd_path, initrd_present) {
        missing.push(format!("initrd {path}"));
    }
    if let (Some(path), Some(false)) = (&checkpoint.config.disk_image_path, disk_image_present) {
        missing.push(format!("disk image {path}"));
    }

    let branch_id = branch_identity(
        &checkpoint_digest,
        reference.map(|reference| reference.digest.as_str()),
        inspection.checkpoint_index,
        inspection.bug.tick,
        inspection.bug.dedup_key,
        inspection.bug.replay_parent_depth,
    );
    Ok(RestorePlan {
        schema: "chaoscontrol.restore_plan.v1",
        checkpoint_digest,
        checkpoint_index: inspection.checkpoint_index,
        artifact: inspection.artifact,
        bug_id: inspection.bug.bug_id,
        assertion_id: inspection.bug.assertion_id,
        dedup_key: inspection.bug.dedup_key,
        tick: inspection.bug.tick,
        replay_parent_depth: inspection.bug.replay_parent_depth,
        snapshot_digest: reference.map(|reference| reference.digest.clone()),
        snapshot_present,
        snapshot_verified,
        branch_id,
        kernel_path: checkpoint.config.kernel_path.clone(),
        kernel_present,
        initrd_path: checkpoint.config.initrd_path.clone(),
        initrd_present,
        disk_image_path: checkpoint.config.disk_image_path.clone(),
        disk_image_present,
        ready: missing.is_empty(),
        missing,
        non_claims: vec![
            "A ready plan means the declared input paths exist; it does not execute a VM.",
            "Path existence is not admission, replay evidence, or workload correctness.",
            "Snapshot digest verification runs only when explicitly requested.",
        ],
    })
}

/// Decoded parent snapshot bound to one checkpoint failure, for a fresh-process
/// restore probe. Loading is not execution and does not prove replay.
#[derive(Debug, Clone)]
pub struct LoadedParentSnapshot {
    pub checkpoint_digest: String,
    pub checkpoint_index: usize,
    pub artifact: String,
    pub bug_id: u64,
    pub tick: u64,
    pub snapshot_digest: String,
    pub replay_parent_depth: u32,
    /// Deterministic identity of the disposable branch rooted at this failure.
    pub branch_id: String,
    pub snapshot: chaoscontrol_vmm::controller::SimulationSnapshot,
}

// r[impl chaoscontrol.agent_investigation.restore_probe]
pub fn load_parent_snapshot<P: AsRef<Path>>(
    checkpoint_path: P,
    selector: BugSelector,
) -> Result<LoadedParentSnapshot, CheckpointBugExportError> {
    let checkpoint_path = checkpoint_path.as_ref();
    let checkpoint_digest = format!(
        "b3:{}",
        blake3::hash(crate::bounded_json::read_checkpoint(checkpoint_path)?.as_bytes()).to_hex()
    );
    let inspection = inspect_bug(checkpoint_path, selector)?;
    let reference = inspection
        .bug
        .replay_parent_snapshot_ref
        .as_ref()
        .ok_or_else(|| CheckpointBugExportError::BugNotFound {
            selector: "a bug with a replay parent snapshot".to_string(),
        })?;
    let source_dir = checkpoint_path.parent().unwrap_or_else(|| Path::new("."));
    let artifact = FileSnapshotStore::new(source_dir)
        .get_snapshot_artifact(reference)
        .map_err(|source| CheckpointBugExportError::SnapshotStore { source })?;
    if artifact.replay_parent_depth != inspection.bug.replay_parent_depth {
        return Err(CheckpointBugExportError::SnapshotStore {
            source: crate::snapshot_store::SnapshotStoreError::MetadataMismatch {
                field: "replay_parent_depth",
            },
        });
    }
    // r[impl chaoscontrol.agent_investigation.restore_probe_branch_binding]
    let branch_id = branch_identity(
        &checkpoint_digest,
        Some(&reference.digest),
        inspection.checkpoint_index,
        inspection.bug.tick,
        inspection.bug.dedup_key,
        inspection.bug.replay_parent_depth,
    );
    Ok(LoadedParentSnapshot {
        checkpoint_digest,
        checkpoint_index: inspection.checkpoint_index,
        artifact: inspection.artifact,
        bug_id: inspection.bug.bug_id,
        tick: inspection.bug.tick,
        snapshot_digest: reference.digest.clone(),
        replay_parent_depth: inspection.bug.replay_parent_depth,
        branch_id,
        snapshot: artifact.snapshot,
    })
}

/// Register targets accepted by the bounded diagnostic intervention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeRegister {
    Rip,
    Rsp,
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    Rbp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
    Rflags,
}

/// Parse `name=0xHEX` or `name=DEC` for one allowed general-purpose register.
// r[impl chaoscontrol.agent_investigation.restore_intervention]
pub fn parse_register_assignment(input: &str) -> Result<(ProbeRegister, u64), String> {
    let (name, value) = input
        .split_once('=')
        .ok_or_else(|| "expected NAME=VALUE".to_string())?;
    let register = match name.trim() {
        "rip" => ProbeRegister::Rip,
        "rsp" => ProbeRegister::Rsp,
        "rax" => ProbeRegister::Rax,
        "rbx" => ProbeRegister::Rbx,
        "rcx" => ProbeRegister::Rcx,
        "rdx" => ProbeRegister::Rdx,
        "rsi" => ProbeRegister::Rsi,
        "rdi" => ProbeRegister::Rdi,
        "rbp" => ProbeRegister::Rbp,
        "r8" => ProbeRegister::R8,
        "r9" => ProbeRegister::R9,
        "r10" => ProbeRegister::R10,
        "r11" => ProbeRegister::R11,
        "r12" => ProbeRegister::R12,
        "r13" => ProbeRegister::R13,
        "r14" => ProbeRegister::R14,
        "r15" => ProbeRegister::R15,
        "rflags" => ProbeRegister::Rflags,
        other => return Err(format!("unsupported register {other}")),
    };
    let value = value.trim();
    let parsed = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        value.parse::<u64>()
    }
    .map_err(|_| format!("invalid value for {name}"))?;
    Ok((register, parsed))
}

/// Validate a read-only memory-probe window: non-empty, at most 4096 bytes, and no
/// address-plus-length overflow.
// r[impl chaoscontrol.agent_investigation.restore_probe_memory_window]
pub fn validate_memory_probe(address: u64, length: usize) -> Result<(), String> {
    if length == 0 || length > 4096 {
        return Err("memory probe length must be in 1..=4096".to_string());
    }
    if address.checked_add(length as u64).is_none() {
        return Err("memory probe range overflows".to_string());
    }
    Ok(())
}

/// Validate that a decoded parent snapshot matches the declared topology before a
/// restore attempt. Returns the actual VM count on mismatch.
// r[impl chaoscontrol.agent_investigation.restore_shape_preflight]
pub fn validate_snapshot_vm_count(
    snapshot: &chaoscontrol_vmm::controller::SimulationSnapshot,
    expected_vms: usize,
) -> Result<(), (usize, usize)> {
    let actual = snapshot.vm_snapshots.len();
    if actual == expected_vms {
        Ok(())
    } else {
        Err((actual, expected_vms))
    }
}

// r[impl chaoscontrol.agent_investigation.restore_smp_topology_admission]
/// Validate that every VM in the snapshot carries the declared vCPU count.
pub fn validate_snapshot_vcpu_count(
    snapshot: &chaoscontrol_vmm::controller::SimulationSnapshot,
    expected: usize,
) -> Result<(), (usize, usize)> {
    for vm in &snapshot.vm_snapshots {
        let actual = vm.0.vcpu_snapshots.len();
        if actual != expected {
            return Err((actual, expected));
        }
    }
    Ok(())
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
pub fn export_checkpoint_bugs<P: AsRef<Path>, Q: AsRef<Path>>(
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
pub fn export_checkpoint_bugs_with_filter<P: AsRef<Path>, Q: AsRef<Path>>(
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
    let source_dir = checkpoint_path.parent().unwrap_or_else(|| Path::new("."));
    let output_dir = output_dir.as_ref();
    fs::create_dir_all(output_dir)?;
    let source_snapshot_store = FileSnapshotStore::new(source_dir);
    let output_snapshot_store = FileSnapshotStore::new(output_dir);

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
        let json = serde_json::to_string_pretty(bug)?;
        if let Err(error) = write_bug_artifact(&path, json.as_bytes(), overwrite) {
            if !overwrite {
                if let CheckpointBugExportError::Io { source } = &error {
                    if source.kind() == std::io::ErrorKind::AlreadyExists {
                        return Err(CheckpointBugExportError::AlreadyExists { path });
                    }
                }
            }
            return Err(error);
        }
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
        fs::write(&path, serde_json::to_vec(&bug).expect("serialize bug"))
            .expect("write bug fixture");

        let loaded = load_serializable_bug(&path).expect("complete bug loads");
        assert_eq!(loaded.bug_id, bug.bug_id);
        assert_eq!(loaded.assertion_identity, bug.assertion_identity);
    }

    #[test]
    fn standalone_bug_loader_rejects_malformed_snapshot_refs() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("bug.json");
        let mut bug = minimal_bug(1);
        bug.replay_parent_depth = 1;
        bug.replay_parent_snapshot_ref = Some(crate::snapshot_store::ReplayParentSnapshotRef {
            store: crate::snapshot_store::FILE_STORE_KIND.to_string(),
            digest: "sha256:not-hex".to_string(),
            codec: crate::snapshot_store::SNAPSHOT_CODEC.to_string(),
            schema_version: crate::snapshot_store::SNAPSHOT_SCHEMA_VERSION,
            path: "snapshots/x.snapshot.bin".to_string(),
        });
        fs::write(&path, serde_json::to_vec(&bug).expect("serialize bug"))
            .expect("write bug fixture");

        assert!(matches!(
            load_serializable_bug(&path),
            Err(CheckpointError::InvalidBugIdentity { .. })
        ));
    }

    #[test]
    fn standalone_bug_loader_accepts_the_exact_schedule_bound() {
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
        bug.schedule.faults = vec![fault; MAX_SERIALIZABLE_FAULTS];
        fs::write(&path, serde_json::to_vec(&bug).expect("serialize bug"))
            .expect("write bug fixture");

        let loaded = load_serializable_bug(&path).expect("boundary schedule loads");
        assert_eq!(loaded.schedule.faults.len(), MAX_SERIALIZABLE_FAULTS);
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
        fs::write(&path, serde_json::to_vec(&bug).expect("serialize bug"))
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
        let fault = Fault::NetworkPartition {
            side_a: vec![0, 1],
            side_b: vec![2, 3],
        };
        let serializable: SerializableFault = (&fault).into();
        let roundtrip: Fault = (&serializable).into();
        assert_eq!(fault, roundtrip);
    }

    #[test]
    fn new_mechanism_faults_round_trip_through_checkpoint_schema() {
        let faults = [
            Fault::CpuThrottle {
                target: 0,
                budget_units: 4,
                duration_ticks: 8,
            },
            Fault::ThreadPause {
                target: 0,
                thread: chaoscontrol_fault::thread_pause::LogicalThreadId {
                    process: String::from("process:v1:writer"),
                    generation: 1,
                    ordinal: 1,
                },
                duration_ticks: 5,
            },
            Fault::CustomEffect {
                effect_id: String::from("test-effect"),
                version: 1,
                parameters: std::collections::BTreeMap::from([(
                    String::from("key"),
                    String::from("value"),
                )]),
            },
            Fault::DiskFlushError {
                target: 1,
                delay_ns: 5,
            },
        ];
        for fault in faults {
            let serializable: SerializableFault = (&fault).into();
            let json = serde_json::to_string(&serializable).unwrap();
            let decoded: SerializableFault = serde_json::from_str(&json).unwrap();
            let roundtrip: Fault = (&decoded).into();
            assert_eq!(fault, roundtrip);
        }
    }

    #[test]
    fn test_serialize_schedule() {
        let mut schedule = FaultSchedule::new();
        schedule.add(ScheduledFault::new(1000, Fault::NetworkHeal));
        schedule.add(ScheduledFault::new(2000, Fault::ProcessKill { target: 0 }));

        let serializable: SerializableSchedule = (&schedule).into();
        assert_eq!(serializable.faults.len(), 2);

        let roundtrip: FaultSchedule = (&serializable).into();
        assert_eq!(roundtrip.total(), 2);
    }

    #[test]
    fn test_checkpoint_roundtrip() {
        let checkpoint = ExplorationCheckpoint {
            config: CheckpointConfig {
                num_vms: 2,
                num_vcpus: 1,
                extra_cmdline: None,
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
                frontier_policy: crate::explorer::FrontierPolicy::default(),
                random_faults: false,
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
            campaign_journal: None,
            release_eligible: true,
            assertion_checkpoint_selector: None,
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
        use std::fs;

        let tempdir = std::env::temp_dir();
        let path = tempdir.join("test_checkpoint.json");

        let checkpoint = ExplorationCheckpoint {
            config: CheckpointConfig {
                num_vms: 3,
                num_vcpus: 1,
                kernel_path: "kernel".to_string(),
                initrd_path: None,
                extra_cmdline: None,
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
                frontier_policy: crate::explorer::FrontierPolicy::default(),
                random_faults: false,
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
            campaign_journal: None,
            release_eligible: true,
            assertion_checkpoint_selector: None,
        };

        save_checkpoint(&path, &checkpoint).unwrap();
        let loaded = load_checkpoint(&path).unwrap();

        assert_eq!(checkpoint.config.num_vms, loaded.config.num_vms);
        assert_eq!(checkpoint.rounds_completed, loaded.rounds_completed);
        assert_eq!(checkpoint.global_coverage, loaded.global_coverage);

        // Cleanup
        let _ = fs::remove_file(&path);
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
                host_watchdog_timeouts: 0,
                host_watchdog_discarded_branches: 0,
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
                host_watchdog_timeouts: 0,
                host_watchdog_discarded_branches: 0,
            },
        ];

        let checkpoint = ExplorationCheckpoint {
            config: CheckpointConfig {
                num_vms: 2,
                num_vcpus: 1,
                kernel_path: "k".to_string(),
                initrd_path: None,
                extra_cmdline: None,
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
                frontier_policy: crate::explorer::FrontierPolicy::default(),
                random_faults: false,
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
            campaign_journal: None,
            release_eligible: true,
            assertion_checkpoint_selector: None,
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
    fn test_round_history_without_watchdog_fields_defaults_to_zero() {
        use crate::explorer::RoundHistory;

        let json = r#"{
            "round": 3,
            "branches_run": 8,
            "new_edges": 1,
            "cumulative_edges": 10,
            "bugs_found": 0,
            "cumulative_bugs": 0,
            "frontier_size": 2,
            "corpus_size": 2
        }"#;
        let history: RoundHistory = serde_json::from_str(json).expect("old round history");
        assert_eq!(history.host_watchdog_timeouts, 0);
        assert_eq!(history.host_watchdog_discarded_branches, 0);

        let recorded = RoundHistory {
            host_watchdog_timeouts: 3,
            host_watchdog_discarded_branches: 1,
            ..history
        };
        let reparsed: RoundHistory =
            serde_json::from_str(&serde_json::to_string(&recorded).expect("serialize"))
                .expect("reparse");
        assert_eq!(reparsed.host_watchdog_timeouts, 3);
        assert_eq!(reparsed.host_watchdog_discarded_branches, 1);
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
                num_vcpus: 1,
                kernel_path: "k".to_string(),
                initrd_path: None,
                extra_cmdline: None,
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
                frontier_policy: crate::explorer::FrontierPolicy::default(),
                random_faults: false,
            },
            global_coverage: vec![],
            bugs: vec![SerializableBug {
                bug_id: 1,
                assertion_id: 50,
                assertion_identity: Some(crate::test_support::assertion_identity(50)),
                fallback_scope: None,
                process_instances: Default::default(),
                marker_context: None,
                assertion_location: "test.rs:1".into(),
                schedule: SerializableSchedule { faults: Vec::new() },
                tick: 1000,
                replay_parent_depth: 0,
                replay_parent_snapshot_ref: None,
                dedup_key: Some(0xBB),
                schedule_variant: None,
                scenario_config: Some(sc_config.clone()),
                scenario_summary: Some(materialized.summary.clone()),
                random_faults: false,
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
            campaign_journal: None,
            release_eligible: true,
            assertion_checkpoint_selector: None,
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

    const RANDOM_FAULTS_KEY: &str = "random_faults";

    // r[verify chaoscontrol.system_runs.fault_injection.scenario.recorded]
    #[test]
    fn records_write_random_faults_explicitly_and_legacy_records_load_as_off() {
        for random_faults in [true, false] {
            let mut bug = minimal_bug(1);
            bug.random_faults = random_faults;
            let mut checkpoint = minimal_checkpoint_with_bugs(vec![bug]);
            checkpoint.config.random_faults = random_faults;

            let value = serde_json::to_value(&checkpoint).expect("checkpoint JSON");
            let recorded = serde_json::json!(random_faults);
            assert_eq!(value["config"][RANDOM_FAULTS_KEY], recorded);
            assert_eq!(value["bugs"][0][RANDOM_FAULTS_KEY], recorded);

            let loaded: ExplorationCheckpoint =
                serde_json::from_value(value).expect("recorded checkpoint");
            assert_eq!(loaded.config.random_faults, random_faults);
            assert_eq!(loaded.bugs[0].random_faults, random_faults);
            let replayed = BugReport::try_from(&loaded.bugs[0]).expect("replay bug");
            assert_eq!(replayed.random_faults, random_faults);
        }

        let mut checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(1)]);
        checkpoint.config.random_faults = true;
        checkpoint.bugs[0].random_faults = true;
        let mut legacy = serde_json::to_value(&checkpoint).expect("checkpoint JSON");
        for pointer in ["/config", "/bugs/0"] {
            legacy
                .pointer_mut(pointer)
                .and_then(serde_json::Value::as_object_mut)
                .expect("record object")
                .remove(RANDOM_FAULTS_KEY);
        }
        let loaded: ExplorationCheckpoint =
            serde_json::from_value(legacy).expect("legacy checkpoint");
        assert!(!loaded.config.random_faults);
        assert!(!loaded.bugs[0].random_faults);
    }

    const FRONTIER_POLICY_KEY: &str = "frontier_policy";

    const RELEASE_ELIGIBLE_KEY: &str = "release_eligible";
    const RETIRED_POLICY_NAMES: [&str; 2] = ["legacy", "campaign-candidate"];
    const PRE_POLICY_CHECKPOINT: &str =
        include_str!("../../../dogfood-results/raft-20260506-095025/checkpoint.json");

    // r[verify chaoscontrol.campaign_policy.progress.valid]
    #[test]
    fn campaign_checkpoint_omits_the_policy_and_eligibility_keys() {
        let checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        assert_eq!(checkpoint.config.frontier_policy, FrontierPolicy::Campaign);

        let value = serde_json::to_value(&checkpoint).expect("checkpoint JSON");
        assert!(value["config"].get(FRONTIER_POLICY_KEY).is_none());
        assert!(value.get(RELEASE_ELIGIBLE_KEY).is_none());
    }

    // r[verify chaoscontrol.campaign_policy.cutover.valid]
    #[test]
    fn old_checkpoint_without_the_policy_field_loads_as_campaign() {
        assert!(!PRE_POLICY_CHECKPOINT.contains(FRONTIER_POLICY_KEY));
        assert!(!PRE_POLICY_CHECKPOINT.contains(RELEASE_ELIGIBLE_KEY));
        let loaded: ExplorationCheckpoint =
            serde_json::from_str(PRE_POLICY_CHECKPOINT).expect("pre-policy checkpoint load");
        assert_eq!(loaded.config.frontier_policy, FrontierPolicy::Campaign);
        assert!(loaded.release_eligible);
    }

    // r[verify chaoscontrol.campaign_policy.rollback.valid]
    #[test]
    fn legacy_diagnostic_checkpoint_round_trips_release_ineligible() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let mut checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        checkpoint.config.frontier_policy = FrontierPolicy::LegacyDiagnostic;
        checkpoint.release_eligible = false;

        let value = serde_json::to_value(&checkpoint).expect("checkpoint JSON");
        assert_eq!(
            value["config"][FRONTIER_POLICY_KEY],
            serde_json::json!(crate::explorer::LEGACY_DIAGNOSTIC_POLICY_NAME)
        );
        assert_eq!(value[RELEASE_ELIGIBLE_KEY], serde_json::json!(false));

        save_checkpoint(&path, &checkpoint).expect("save");
        let loaded = load_checkpoint(&path).expect("load");
        assert_eq!(
            loaded.config.frontier_policy,
            FrontierPolicy::LegacyDiagnostic
        );
        assert!(!loaded.release_eligible);
    }

    // r[verify chaoscontrol.campaign_policy.rollback.valid]
    #[test]
    fn legacy_diagnostic_checkpoint_claiming_release_eligibility_is_rejected() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let mut checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        checkpoint.config.frontier_policy = FrontierPolicy::LegacyDiagnostic;
        assert!(matches!(
            save_checkpoint(&path, &checkpoint),
            Err(CheckpointError::DiagnosticClaimsReleaseEligibility)
        ));

        // A hand-edited file that drops the mark is refused on load.
        checkpoint.release_eligible = false;
        save_checkpoint(&path, &checkpoint).expect("save");
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("JSON");
        value
            .as_object_mut()
            .expect("checkpoint object")
            .remove(RELEASE_ELIGIBLE_KEY);
        std::fs::write(&path, serde_json::to_vec(&value).expect("JSON")).expect("write");
        assert!(matches!(
            load_checkpoint(&path),
            Err(CheckpointError::DiagnosticClaimsReleaseEligibility)
        ));
    }

    // r[verify chaoscontrol.campaign_policy.rollback.invalid]
    #[test]
    fn checkpoint_with_unknown_or_retired_frontier_policy_is_rejected() {
        let checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        let mut value = serde_json::to_value(&checkpoint).expect("checkpoint JSON");
        for unknown in ["bogus", "Legacy", "campaign_candidate", ""]
            .into_iter()
            .chain(RETIRED_POLICY_NAMES)
        {
            value["config"][FRONTIER_POLICY_KEY] = serde_json::json!(unknown);
            assert!(
                serde_json::from_value::<ExplorationCheckpoint>(value.clone()).is_err(),
                "policy {unknown:?} must be rejected"
            );
        }
    }

    const CAMPAIGN_JOURNAL_KEY: &str = "campaign_journal";
    const TEST_JOURNAL_EPOCH: u32 = 3;
    const TEST_JOURNAL_SEGMENT: u64 = 17;

    fn test_journal_digest() -> [u8; CAMPAIGN_JOURNAL_DIGEST_BYTES] {
        *blake3::hash(b"settlement segment").as_bytes()
    }

    #[test]
    fn journal_binding_encodes_lowercase_hex_and_decodes_back() {
        let digest = test_journal_digest();
        let binding = CampaignJournalBinding::new(TEST_JOURNAL_EPOCH, TEST_JOURNAL_SEGMENT, digest);
        assert_eq!(
            binding.segment_digest,
            blake3::Hash::from(digest).to_hex().as_str()
        );
        assert_eq!(binding.digest_bytes().expect("digest decodes"), digest);

        let boundary = CampaignJournalBinding::new(0, 0, [0xff; CAMPAIGN_JOURNAL_DIGEST_BYTES]);
        assert_eq!(
            boundary.segment_digest,
            "f".repeat(CAMPAIGN_JOURNAL_DIGEST_HEX_LEN)
        );
        assert_eq!(
            boundary.digest_bytes().expect("boundary decodes"),
            [0xff; CAMPAIGN_JOURNAL_DIGEST_BYTES]
        );
    }

    #[test]
    fn checkpoint_without_journal_binding_keeps_its_legacy_shape() {
        let checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        let value = serde_json::to_value(&checkpoint).expect("checkpoint JSON");
        assert!(value.get(CAMPAIGN_JOURNAL_KEY).is_none());
        let loaded: ExplorationCheckpoint = serde_json::from_value(value).expect("legacy load");
        assert!(loaded.campaign_journal.is_none());
    }

    #[test]
    fn checkpoint_journal_binding_round_trips_through_save_and_load() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let mut checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        let binding = CampaignJournalBinding::new(
            TEST_JOURNAL_EPOCH,
            TEST_JOURNAL_SEGMENT,
            test_journal_digest(),
        );
        checkpoint.campaign_journal = Some(binding.clone());

        save_checkpoint(&path, &checkpoint).expect("save");
        let loaded = load_checkpoint(&path).expect("load");
        assert_eq!(loaded.campaign_journal, Some(binding));
    }

    #[test]
    fn checkpoint_with_malformed_journal_digest_is_rejected() {
        let valid = CampaignJournalBinding::new(
            TEST_JOURNAL_EPOCH,
            TEST_JOURNAL_SEGMENT,
            test_journal_digest(),
        )
        .segment_digest;
        let malformed = [
            valid.to_ascii_uppercase(),
            valid[..CAMPAIGN_JOURNAL_DIGEST_HEX_LEN - 1].to_string(),
            format!("{valid}0"),
            format!("{}g", &valid[..CAMPAIGN_JOURNAL_DIGEST_HEX_LEN - 1]),
            String::new(),
        ];
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        for digest in malformed {
            let binding = CampaignJournalBinding {
                epoch: TEST_JOURNAL_EPOCH,
                segment_seq: TEST_JOURNAL_SEGMENT,
                segment_digest: digest.clone(),
            };
            assert!(
                matches!(
                    binding.digest_bytes(),
                    Err(CheckpointError::InvalidCampaignJournalDigest { .. })
                ),
                "digest {digest:?} must be rejected"
            );
            let mut checkpoint = minimal_checkpoint_with_bugs(Vec::new());
            checkpoint.campaign_journal = Some(binding);
            assert!(save_checkpoint(&path, &checkpoint).is_err());
            write_untrusted_checkpoint(&path, &checkpoint);
            assert!(matches!(
                load_checkpoint(&path),
                Err(CheckpointError::InvalidCampaignJournalDigest { .. })
            ));
        }
    }

    #[test]
    fn checkpoint_journal_binding_rejects_unknown_fields() {
        let mut checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        checkpoint.campaign_journal = Some(CampaignJournalBinding::new(
            TEST_JOURNAL_EPOCH,
            TEST_JOURNAL_SEGMENT,
            test_journal_digest(),
        ));
        let mut value = serde_json::to_value(&checkpoint).expect("checkpoint JSON");
        value[CAMPAIGN_JOURNAL_KEY]["unreviewed"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ExplorationCheckpoint>(value).is_err());
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
                num_vcpus: 1,
                kernel_path: "k".to_string(),
                initrd_path: None,
                extra_cmdline: None,
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
                frontier_policy: crate::explorer::FrontierPolicy::default(),
                random_faults: false,
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
            campaign_journal: None,
            release_eligible: true,
            assertion_checkpoint_selector: None,
        }
    }

    const TEST_EXPORT_ALIAS: u64 = 1_806_003_755;

    fn write_untrusted_checkpoint(path: &Path, checkpoint: &ExplorationCheckpoint) {
        let bytes = serde_json::to_vec_pretty(checkpoint).expect("checkpoint fixture JSON");
        fs::write(path, bytes).expect("write untrusted checkpoint fixture");
    }

    fn minimal_bug(bug_id: u64) -> SerializableBug {
        let assertion_id = bug_id + 100;
        SerializableBug {
            bug_id,
            assertion_id,
            assertion_identity: Some(crate::test_support::assertion_identity(assertion_id)),
            fallback_scope: None,
            process_instances: Default::default(),
            marker_context: None,
            assertion_location: "test.rs:1".into(),
            schedule: SerializableSchedule { faults: Vec::new() },
            tick: 1000,
            replay_parent_depth: 0,
            replay_parent_snapshot_ref: None,
            dedup_key: Some(bug_id),
            schedule_variant: None,
            scenario_config: None,
            scenario_summary: None,
            random_faults: false,
        }
    }

    fn set_assertion_alias(bug: &mut SerializableBug, assertion_id: u64) {
        bug.assertion_id = assertion_id;
        bug.assertion_identity = Some(crate::test_support::assertion_identity(assertion_id));
    }

    #[test]
    fn checkpoint_vcpu_count_is_bounded() {
        let mut checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        checkpoint.config.num_vcpus = 0;
        assert!(matches!(
            validate_checkpoint(&checkpoint),
            Err(CheckpointError::InvalidBounds { .. })
        ));
        checkpoint.config.num_vcpus = chaoscontrol_vmm::scheduler::core::MAX_SCHEDULE_VCPUS + 1;
        assert!(matches!(
            validate_checkpoint(&checkpoint),
            Err(CheckpointError::InvalidBounds { .. })
        ));
        checkpoint.config.num_vcpus = 2;
        validate_checkpoint(&checkpoint).expect("two vCPUs are admitted");
    }

    #[test]
    fn checkpoint_dedup_keys_enforce_the_exact_boundary() {
        let mut checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        checkpoint.seen_dedup_keys = Some((0..MAX_CHECKPOINT_DEDUP_KEYS as u64).collect());
        validate_checkpoint(&checkpoint).expect("exact dedup bound");
        checkpoint
            .seen_dedup_keys
            .as_mut()
            .expect("dedup keys")
            .push(MAX_CHECKPOINT_DEDUP_KEYS as u64);
        assert!(matches!(
            validate_checkpoint(&checkpoint),
            Err(CheckpointError::InvalidBounds { .. })
        ));
    }

    #[test]
    fn prevalidated_assertion_resolution_matches_full_validation() {
        let checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(1), minimal_bug(2)]);
        let report = checkpoint.assertion_report.as_ref().expect("report");
        let facts = chaoscontrol_fault::oracle_validation::validate_oracle_report_claim(report)
            .expect("report facts");
        for bug in &checkpoint.bugs {
            let identity = crate::bug::identity::validate_carrier(
                bug.assertion_id,
                bug.assertion_identity.as_ref(),
            )
            .expect("identity");
            let full =
                chaoscontrol_fault::oracle_validation::resolve_assertion_evidence(report, identity)
                    .expect("full resolution");
            let prevalidated =
                chaoscontrol_fault::oracle_validation::resolve_assertion_evidence_prevalidated(
                    report, &facts, identity,
                )
                .expect("prevalidated resolution");
            assert_eq!(full, prevalidated);
        }
    }

    #[test]
    fn duplicate_bug_ids_are_rejected() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(1), minimal_bug(1)]);
        assert!(matches!(
            save_checkpoint(&path, &checkpoint),
            Err(CheckpointError::InvalidBugIdentity { .. })
        ));
    }

    #[test]
    fn invalid_checkpoint_does_not_replace_a_valid_checkpoint() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let valid = minimal_checkpoint_with_bugs(Vec::new());
        save_checkpoint(&path, &valid).expect("valid checkpoint writes");
        let before = fs::read(&path).expect("valid checkpoint bytes");
        let mut invalid = minimal_checkpoint_with_bugs(vec![minimal_bug(1)]);
        invalid.assertion_report = None;

        assert!(save_checkpoint(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).expect("retained checkpoint bytes"), before);
    }

    #[cfg(unix)]
    #[test]
    fn checkpoint_save_replaces_a_symlink_without_following_it() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("checkpoint.json");
        let target = directory.path().join("target.json");
        fs::write(&target, b"retain target").expect("target fixture");
        symlink(&target, &path).expect("checkpoint symlink");

        save_checkpoint(&path, &minimal_checkpoint_with_bugs(Vec::new()))
            .expect("valid checkpoint replaces symlink");
        assert_eq!(fs::read(&target).expect("target bytes"), b"retain target");
        assert!(!fs::symlink_metadata(&path)
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
            serde_json::from_str(&fs::read_to_string(out.join("bug_0.json")).unwrap()).unwrap();
        let bug1: SerializableBug =
            serde_json::from_str(&fs::read_to_string(out.join("bug_1.json")).unwrap()).unwrap();
        assert_eq!(bug0.bug_id, 41);
        assert_eq!(bug1.bug_id, 99);
    }

    #[cfg(unix)]
    #[test]
    fn export_checkpoint_bugs_replaces_existing_with_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("bug_0.json"), b"original").unwrap();
        let checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(41)]);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        export_checkpoint_bugs(&checkpoint_path, &out, true).unwrap();
        let exported = fs::read(out.join("bug_0.json")).unwrap();
        assert_ne!(exported, b"original");
        assert!(exported.starts_with(b"{"));
    }

    #[test]
    fn export_checkpoint_bugs_preserves_existing_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("bug_0.json"), b"original").unwrap();
        let checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(41)]);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        assert!(matches!(
            export_checkpoint_bugs(&checkpoint_path, &out, false),
            Err(CheckpointBugExportError::AlreadyExists { .. })
        ));
        assert_eq!(fs::read(out.join("bug_0.json")).unwrap(), b"original");
    }

    #[test]
    fn export_checkpoint_bugs_replaces_symlink_without_following() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        fs::create_dir_all(&out).unwrap();
        let target = dir.path().join("target.json");
        fs::write(&target, b"original").unwrap();
        std::os::unix::fs::symlink(&target, out.join("bug_0.json")).unwrap();
        let checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(41)]);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        export_checkpoint_bugs(&checkpoint_path, &out, true).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert!(fs::read(out.join("bug_0.json")).unwrap().starts_with(b"{"));
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
                dedup_key: None,
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
                dedup_key: None,
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
                dedup_key: None,
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
    fn export_checkpoint_bugs_filter_selects_exact_dedup_key() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        let checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(1), minimal_bug(2)]);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        let summary = export_checkpoint_bugs_with_filter(
            &checkpoint_path,
            &out,
            true,
            CheckpointBugExportFilter {
                assertion_id: None,
                min_replay_parent_depth: None,
                dedup_key: Some(2),
                max_bugs: None,
            },
        )
        .unwrap();

        assert_eq!(summary.bugs_scanned, 2);
        assert_eq!(summary.bugs_matched, 1);
        assert_eq!(summary.bugs_written, 1);
        assert!(!out.join("bug_0.json").exists());
        assert!(out.join("bug_1.json").exists());
    }

    #[test]
    fn export_checkpoint_bugs_filter_ignores_legacy_missing_dedup_key() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let out = dir.path().join("exported");
        let mut legacy = minimal_bug(1);
        legacy.dedup_key = None;
        let kept = minimal_bug(2);
        let checkpoint = minimal_checkpoint_with_bugs(vec![legacy, kept]);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        let summary = export_checkpoint_bugs_with_filter(
            &checkpoint_path,
            &out,
            true,
            CheckpointBugExportFilter {
                assertion_id: None,
                min_replay_parent_depth: None,
                dedup_key: Some(2),
                max_bugs: None,
            },
        )
        .unwrap();
        assert_eq!(summary.bugs_scanned, 2);
        assert_eq!(summary.bugs_matched, 1);
        assert_eq!(summary.bugs_written, 1);
        assert!(!out.join("bug_0.json").exists());
        assert!(out.join("bug_1.json").exists());

        let empty_out = dir.path().join("empty");
        let empty = export_checkpoint_bugs_with_filter(
            &checkpoint_path,
            &empty_out,
            true,
            CheckpointBugExportFilter {
                assertion_id: None,
                min_replay_parent_depth: None,
                dedup_key: Some(9),
                max_bugs: None,
            },
        )
        .unwrap();
        assert_eq!(empty.bugs_matched, 0);
        assert_eq!(empty.bugs_written, 0);
        assert!(!empty_out.join("bug_0.json").exists());
        assert!(!empty_out.join("bug_1.json").exists());
    }

    #[test]
    fn checkpoint_records_smp_topology_and_cmdline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkpoint.json");
        let mut checkpoint = minimal_checkpoint_with_bugs(Vec::new());
        checkpoint.config.num_vcpus = 2;
        checkpoint.config.extra_cmdline = Some("clocksource=tsc tsc=reliable".to_string());
        save_checkpoint(&path, &checkpoint).unwrap();
        let loaded = load_checkpoint(&path).unwrap();
        assert_eq!(loaded.config.num_vcpus, 2);
        assert_eq!(
            loaded.config.extra_cmdline.as_deref(),
            Some("clocksource=tsc tsc=reliable")
        );

        let mut legacy = serde_json::to_value(&checkpoint).unwrap();
        legacy["config"]
            .as_object_mut()
            .unwrap()
            .remove("num_vcpus");
        legacy["config"]
            .as_object_mut()
            .unwrap()
            .remove("extra_cmdline");
        let legacy: ExplorationCheckpoint = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.config.num_vcpus, 1);
        assert!(legacy.config.extra_cmdline.is_none());
    }

    #[test]
    fn resolve_bug_selector_accepts_one_identity() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let first = minimal_bug(1);
        let mut second = minimal_bug(2);
        second.assertion_identity = first.assertion_identity.clone();
        second.assertion_id = first.assertion_id;
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![first.clone(), second]),
        )
        .unwrap();
        let fingerprint = first
            .assertion_identity
            .as_ref()
            .unwrap()
            .fingerprint
            .to_hex();
        assert_eq!(
            resolve_bug_selector(&checkpoint_path, None, None, Some(&fingerprint))
                .unwrap_err()
                .to_string(),
            "2 checkpoint bugs match assertion fingerprint ".to_string() + &fingerprint
        );
        let singleton_path = dir.path().join("singleton.json");
        save_checkpoint(
            &singleton_path,
            &minimal_checkpoint_with_bugs(vec![first.clone()]),
        )
        .unwrap();
        let by_fingerprint =
            resolve_bug_selector(&singleton_path, None, None, Some(&fingerprint)).unwrap();
        assert_eq!(by_fingerprint.index, Some(0));
        let unique = resolve_bug_selector(&checkpoint_path, Some(0), None, None).unwrap();
        assert_eq!(unique.index, Some(0));
        assert!(matches!(
            resolve_bug_selector(&checkpoint_path, None, None, None),
            Err(CheckpointBugExportError::BugSelector)
        ));
        assert!(matches!(
            resolve_bug_selector(&checkpoint_path, Some(0), Some(1), None),
            Err(CheckpointBugExportError::BugSelector)
        ));
    }

    #[test]
    fn bug_list_filter_matches_process_scope_and_identity() {
        let mut bug = minimal_bug(7);
        bug.process_instances = ["worker-2".to_string()].into_iter().collect();
        let matches = BugListFilter {
            process_instance: Some("worker-2".to_string()),
            ..BugListFilter::default()
        };
        assert!(matches.matches(&bug));
        let other = BugListFilter {
            process_instance: Some("worker-1".to_string()),
            ..BugListFilter::default()
        };
        assert!(!other.matches(&bug));
        let identity = BugListFilter {
            require_assertion_identity: true,
            ..BugListFilter::default()
        };
        assert_eq!(identity.matches(&bug), bug.assertion_identity.is_some());
        let fingerprint = bug
            .assertion_identity
            .as_ref()
            .expect("minimal bug carries an identity")
            .fingerprint
            .to_hex();
        let matching = BugListFilter {
            assertion_fingerprint: Some(fingerprint),
            ..BugListFilter::default()
        };
        assert!(matching.matches(&bug));
        let mismatched = BugListFilter {
            assertion_fingerprint: Some("00".repeat(32)),
            ..BugListFilter::default()
        };
        assert!(!mismatched.matches(&bug));
    }

    #[test]
    fn list_bugs_filters_without_writing_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let mut first = minimal_bug(1);
        first.tick = 100;
        let mut second = minimal_bug(2);
        second.tick = 900;
        let checkpoint = minimal_checkpoint_with_bugs(vec![first, second]);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        let all = list_bugs_with_filter(&checkpoint_path, BugListFilter::default()).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].checkpoint_index, 0);
        assert_eq!(all[0].artifact, "bug_0.json");
        assert_eq!(all[0].dedup_key, Some(1));
        assert_eq!(all[1].checkpoint_index, 1);

        let by_key = list_bugs_with_filter(
            &checkpoint_path,
            BugListFilter {
                dedup_key: Some(2),
                ..BugListFilter::default()
            },
        )
        .unwrap();
        assert_eq!(by_key.len(), 1);
        assert_eq!(by_key[0].bug_id, 2);

        let by_range = list_bugs_with_filter(
            &checkpoint_path,
            BugListFilter {
                min_tick: Some(500),
                max_tick: Some(1000),
                ..BugListFilter::default()
            },
        )
        .unwrap();
        assert_eq!(by_range.len(), 1);
        assert_eq!(by_range[0].bug_id, 2);

        let by_assertion = list_bugs_with_filter(
            &checkpoint_path,
            BugListFilter {
                assertion_id: Some(101),
                ..BugListFilter::default()
            },
        )
        .unwrap();
        assert_eq!(by_assertion.len(), 1);
        assert_eq!(by_assertion[0].bug_id, 1);

        let by_process = list_bugs_with_filter(
            &checkpoint_path,
            BugListFilter {
                process_instance: Some("worker-1".to_string()),
                ..BugListFilter::default()
            },
        )
        .unwrap();
        assert!(by_process.is_empty());
        let by_identity = list_bugs_with_filter(
            &checkpoint_path,
            BugListFilter {
                require_assertion_identity: true,
                ..BugListFilter::default()
            },
        )
        .unwrap();
        assert_eq!(by_identity.len(), 2);

        assert!(!dir.path().join("bug_0.json").exists());
    }

    #[test]
    fn list_bugs_rejects_invalid_assertion_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![minimal_bug(1)]),
        )
        .unwrap();
        assert!(matches!(
            list_bugs_with_filter(
                &checkpoint_path,
                BugListFilter {
                    assertion_fingerprint: Some("not-hex".to_string()),
                    ..BugListFilter::default()
                },
            ),
            Err(CheckpointBugExportError::InvalidAssertionFingerprint { .. })
        ));
    }

    #[test]
    fn list_bugs_rejects_inverted_tick_range() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![minimal_bug(1)]),
        )
        .unwrap();
        let error = list_bugs_with_filter(
            &checkpoint_path,
            BugListFilter {
                min_tick: Some(10),
                max_tick: Some(5),
                ..BugListFilter::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            CheckpointBugExportError::InvalidTickRange {
                min_tick: 10,
                max_tick: 5
            }
        ));
    }

    #[test]
    fn list_bugs_never_matches_legacy_missing_dedup_key() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let mut legacy = minimal_bug(1);
        legacy.dedup_key = None;
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![legacy]),
        )
        .unwrap();
        let entries = list_bugs_with_filter(
            &checkpoint_path,
            BugListFilter {
                dedup_key: Some(1),
                ..BugListFilter::default()
            },
        )
        .unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn inspect_bug_selects_by_index_or_key() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![minimal_bug(1), minimal_bug(2)]),
        )
        .unwrap();

        let by_index = inspect_bug(
            &checkpoint_path,
            BugSelector {
                index: Some(1),
                dedup_key: None,
            },
        )
        .unwrap();
        assert_eq!(by_index.schema, "chaoscontrol.bug_inspection.v1");
        assert_eq!(by_index.checkpoint_index, 1);
        assert_eq!(by_index.artifact, "bug_1.json");
        assert_eq!(by_index.bug.bug_id, 2);

        let by_key = inspect_bug(
            &checkpoint_path,
            BugSelector {
                index: None,
                dedup_key: Some(1),
            },
        )
        .unwrap();
        assert_eq!(by_key.checkpoint_index, 0);
        assert_eq!(by_key.bug.bug_id, 1);
        assert!(!by_key.non_claims.is_empty());
    }

    #[test]
    fn inspect_bug_rejects_missing_ambiguous_and_empty_selectors() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![minimal_bug(1), minimal_bug(2)]),
        )
        .unwrap();

        let missing = inspect_bug(
            &checkpoint_path,
            BugSelector {
                index: None,
                dedup_key: Some(99),
            },
        )
        .unwrap_err();
        assert!(matches!(
            missing,
            CheckpointBugExportError::BugNotFound { .. }
        ));

        let empty = inspect_bug(&checkpoint_path, BugSelector::default()).unwrap_err();
        assert!(matches!(empty, CheckpointBugExportError::BugSelector));

        let both = inspect_bug(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: Some(1),
            },
        )
        .unwrap_err();
        assert!(matches!(both, CheckpointBugExportError::BugSelector));

        let mut duplicate = minimal_bug(2);
        duplicate.dedup_key = Some(1);
        let ambiguous_path = dir.path().join("ambiguous.json");
        save_checkpoint(
            &ambiguous_path,
            &minimal_checkpoint_with_bugs(vec![minimal_bug(1), duplicate]),
        )
        .unwrap();
        let ambiguous = inspect_bug(
            &ambiguous_path,
            BugSelector {
                index: None,
                dedup_key: Some(1),
            },
        )
        .unwrap_err();
        assert!(matches!(
            ambiguous,
            CheckpointBugExportError::BugAmbiguous { matches: 2, .. }
        ));
    }

    #[test]
    fn verify_checkpoint_admits_structurally_valid_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![minimal_bug(1), minimal_bug(2)]),
        )
        .unwrap();
        let verification = verify_checkpoint(&checkpoint_path).unwrap();
        assert_eq!(
            verification.schema,
            "chaoscontrol.checkpoint_verification.v1"
        );
        assert!(verification.valid);
        assert_eq!(verification.bugs_total, 2);
        assert_eq!(verification.bugs_with_snapshot_ref, 0);
        assert_eq!(verification.snapshots_validated, 0);
        assert!(verification.checkpoint_digest.starts_with("b3:"));
        assert!(!verification.non_claims.is_empty());
    }

    #[test]
    fn structure_only_admission_skips_snapshot_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let mut bug = minimal_bug(1);
        bug.replay_parent_depth = 1;
        bug.replay_parent_snapshot_ref = Some(ReplayParentSnapshotRef {
            store: crate::snapshot_store::FILE_STORE_KIND.to_string(),
            digest: format!("sha256:{}", "0".repeat(64)),
            codec: crate::snapshot_store::SNAPSHOT_CODEC.to_string(),
            schema_version: 2,
            path: format!("snapshots/{}.snapshot.bin", "0".repeat(64)),
        });
        save_checkpoint(&checkpoint_path, &minimal_checkpoint_with_bugs(vec![bug])).unwrap();

        let structure = verify_checkpoint_with_options(
            &checkpoint_path,
            None,
            CheckpointAdmissionMode::StructureOnly,
        )
        .unwrap();
        assert!(structure.valid);
        assert_eq!(structure.mode, CheckpointAdmissionMode::StructureOnly);
        assert!(!structure.snapshots_checked);
        assert_eq!(structure.bugs_with_snapshot_ref, 1);
        assert_eq!(structure.snapshots_validated, 0);

        let full = verify_checkpoint(&checkpoint_path).unwrap();
        assert!(!full.valid);
        assert!(full.snapshots_checked);
        assert_eq!(full.issues.len(), 1);
    }

    fn snapshot_ref(path_suffix: &str, digest_hex: &str) -> ReplayParentSnapshotRef {
        ReplayParentSnapshotRef {
            store: crate::snapshot_store::FILE_STORE_KIND.to_string(),
            digest: format!("sha256:{digest_hex}"),
            codec: crate::snapshot_store::SNAPSHOT_CODEC.to_string(),
            schema_version: 2,
            path: format!("snapshots/{path_suffix}.snapshot.bin"),
        }
    }

    #[test]
    fn restore_plan_checks_declared_inputs_without_executing() {
        let dir = tempfile::tempdir().unwrap();
        let kernel = dir.path().join("vmlinux");
        std::fs::write(&kernel, b"kernel").unwrap();
        let snapshot_dir = dir.path().join("snapshots");
        std::fs::create_dir_all(&snapshot_dir).unwrap();
        let reference = snapshot_ref(&"4".repeat(64), &"4".repeat(64));
        std::fs::write(dir.path().join(&reference.path), b"snapshot").unwrap();

        let checkpoint_path = dir.path().join("checkpoint.json");
        let mut checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(1)]);
        checkpoint.config.kernel_path = kernel.display().to_string();
        checkpoint.bugs[0].replay_parent_depth = 1;
        checkpoint.bugs[0].replay_parent_snapshot_ref = Some(reference);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        let plan = plan_restore(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
        )
        .unwrap();
        assert_eq!(plan.schema, "chaoscontrol.restore_plan.v1");
        assert!(plan.ready);
        assert!(plan.snapshot_present);
        assert!(plan.kernel_present);
        assert!(plan.missing.is_empty());
        assert!(!plan.snapshot_verified);
        let repeat = plan_restore(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
        )
        .unwrap();
        assert_eq!(plan.branch_id, repeat.branch_id);
        assert!(plan.branch_id.starts_with("b3:"));
        assert!(plan_restore_with_snapshot_check(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
            true,
        )
        .is_err());

        let missing_path = dir.path().join("missing.json");
        let mut missing = checkpoint.clone();
        missing.config.kernel_path = dir.path().join("nope").display().to_string();
        save_checkpoint(&missing_path, &missing).unwrap();
        let plan = plan_restore(
            &missing_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
        )
        .unwrap();
        assert!(!plan.ready);
        assert!(plan.missing.iter().any(|item| item.starts_with("kernel ")));
    }

    #[test]
    fn branch_identity_is_length_delimited_and_input_bound() {
        let first = branch_identity("b3:aa", Some("sha256:bb"), 0, 10, Some(1), 1);
        let repeat = branch_identity("b3:aa", Some("sha256:bb"), 0, 10, Some(1), 1);
        assert_eq!(first, repeat);
        assert_ne!(
            first,
            branch_identity("b3:aa", Some("sha256:bb"), 0, 11, Some(1), 1)
        );
        assert_ne!(
            first,
            branch_identity("b3:aa", Some("sha256:bb"), 1, 10, Some(1), 1)
        );
        assert_ne!(
            branch_identity("ab", Some("c"), 0, 0, None, 0),
            branch_identity("a", Some("bc"), 0, 0, None, 0)
        );
    }

    #[test]
    fn restore_plan_can_verify_a_stored_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let kernel = dir.path().join("vmlinux");
        std::fs::write(&kernel, b"kernel").unwrap();
        let network_state = chaoscontrol_vmm::controller::NetworkFabric::new(2, 42);
        let engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());
        let snapshot = chaoscontrol_vmm::controller::SimulationSnapshot {
            tick: 321,
            vm_snapshots: Vec::new(),
            network_state,
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: vec![],
            cpu_throttle: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
            memory_pressure: vec![],
            process_fault_attempt: vec![],
            pending_process_observations: Default::default(),
            fault_operation_sequence: 0,
        };
        let reference = FileSnapshotStore::new(dir.path())
            .put_snapshot(&snapshot, 1)
            .unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let mut checkpoint = minimal_checkpoint_with_bugs(vec![minimal_bug(1)]);
        checkpoint.config.kernel_path = kernel.display().to_string();
        checkpoint.bugs[0].replay_parent_depth = 1;
        checkpoint.bugs[0].replay_parent_snapshot_ref = Some(reference);
        save_checkpoint(&checkpoint_path, &checkpoint).unwrap();

        let plan = plan_restore_with_snapshot_check(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
            true,
        )
        .unwrap();
        assert!(plan.ready);
        assert!(plan.snapshot_verified);
        assert_eq!(
            plan.snapshot_digest,
            checkpoint.bugs[0]
                .replay_parent_snapshot_ref
                .as_ref()
                .map(|r| r.digest.clone())
        );
    }

    #[test]
    fn list_snapshot_references_groups_shared_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let shared = snapshot_ref(&"1".repeat(64), &"1".repeat(64));
        let mut first = minimal_bug(1);
        first.replay_parent_depth = 1;
        first.replay_parent_snapshot_ref = Some(shared.clone());
        let mut second = minimal_bug(2);
        second.replay_parent_depth = 1;
        second.replay_parent_snapshot_ref = Some(shared);
        let plain = minimal_bug(3);
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![first, second, plain]),
        )
        .unwrap();

        let listing = list_snapshot_references(&checkpoint_path).unwrap();
        assert_eq!(listing.schema, "chaoscontrol.snapshot_references.v1");
        assert_eq!(listing.snapshots.len(), 1);
        assert_eq!(listing.snapshots[0].bug_indices, vec![0, 1]);
        assert_eq!(
            listing.snapshots[0].artifacts,
            vec!["bug_0.json".to_string(), "bug_1.json".to_string()]
        );
        assert!(listing.checkpoint_digest.starts_with("b3:"));
    }

    #[test]
    fn list_snapshot_references_rejects_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let first = snapshot_ref(&"2".repeat(64), &"2".repeat(64));
        let conflicting = snapshot_ref(&"2".repeat(64), &"3".repeat(64));
        let mut one = minimal_bug(1);
        one.replay_parent_depth = 1;
        one.replay_parent_snapshot_ref = Some(first);
        let mut two = minimal_bug(2);
        two.replay_parent_depth = 1;
        two.replay_parent_snapshot_ref = Some(conflicting);
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![one, two]),
        )
        .unwrap();

        let error = list_snapshot_references(&checkpoint_path).unwrap_err();
        assert!(matches!(
            error,
            CheckpointBugExportError::SnapshotReferenceConflict { .. }
        ));
    }

    #[test]
    fn register_assignment_parses_allowlisted_names_and_values() {
        assert_eq!(
            parse_register_assignment("rax=0x2a"),
            Ok((ProbeRegister::Rax, 42))
        );
        assert_eq!(
            parse_register_assignment("rip=4096"),
            Ok((ProbeRegister::Rip, 4096))
        );
        assert_eq!(
            parse_register_assignment("r15 = 0XFF"),
            Ok((ProbeRegister::R15, 255))
        );
        assert!(parse_register_assignment("cr3=0x1").is_err());
        assert!(parse_register_assignment("rax").is_err());
        assert!(parse_register_assignment("rax=nope").is_err());
    }

    #[test]
    fn memory_probe_window_is_bounded_and_overflow_checked() {
        assert!(validate_memory_probe(0x1000, 16).is_ok());
        assert!(validate_memory_probe(0x1000, 4096).is_ok());
        assert!(validate_memory_probe(0x1000, 0).is_err());
        assert!(validate_memory_probe(0x1000, 4097).is_err());
        assert!(validate_memory_probe(u64::MAX - 1, 16).is_err());
    }

    #[test]
    fn snapshot_vm_count_preflight_matches_declared_topology() {
        let network_state = chaoscontrol_vmm::controller::NetworkFabric::new(2, 42);
        let engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());
        let snapshot = chaoscontrol_vmm::controller::SimulationSnapshot {
            tick: 10,
            vm_snapshots: Vec::new(),
            network_state,
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: vec![],
            cpu_throttle: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
            memory_pressure: vec![],
            process_fault_attempt: vec![],
            pending_process_observations: Default::default(),
            fault_operation_sequence: 0,
        };
        assert_eq!(validate_snapshot_vm_count(&snapshot, 0), Ok(()));
        assert_eq!(validate_snapshot_vm_count(&snapshot, 3), Err((0, 3)));
    }

    #[test]
    fn load_parent_snapshot_returns_the_decoded_parent() {
        let dir = tempfile::tempdir().unwrap();
        let network_state = chaoscontrol_vmm::controller::NetworkFabric::new(2, 42);
        let engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());
        let snapshot = chaoscontrol_vmm::controller::SimulationSnapshot {
            tick: 750,
            vm_snapshots: Vec::new(),
            network_state,
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: vec![],
            cpu_throttle: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
            memory_pressure: vec![],
            process_fault_attempt: vec![],
            pending_process_observations: Default::default(),
            fault_operation_sequence: 0,
        };
        let reference = FileSnapshotStore::new(dir.path())
            .put_snapshot(&snapshot, 1)
            .unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let mut bug = minimal_bug(1);
        bug.replay_parent_depth = 1;
        bug.replay_parent_snapshot_ref = Some(reference.clone());
        save_checkpoint(&checkpoint_path, &minimal_checkpoint_with_bugs(vec![bug])).unwrap();

        let loaded = load_parent_snapshot(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
        )
        .unwrap();
        assert_eq!(loaded.snapshot.tick, 750);
        assert_eq!(loaded.snapshot_digest, reference.digest);
        assert_eq!(loaded.replay_parent_depth, 1);
        assert_eq!(loaded.bug_id, 1);
        assert!(loaded.checkpoint_digest.starts_with("b3:"));
    }

    #[test]
    fn inspect_snapshot_summarizes_the_referenced_parent() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let network_state = chaoscontrol_vmm::controller::NetworkFabric::new(2, 42);
        let engine = chaoscontrol_fault::engine::FaultEngine::new(Default::default());
        let snapshot = chaoscontrol_vmm::controller::SimulationSnapshot {
            tick: 750,
            vm_snapshots: Vec::new(),
            network_state,
            fault_engine_snapshot: engine.snapshot(),
            vcpu_stall_until: vec![],
            cpu_throttle: vec![],
            clock_freeze: vec![],
            clock_jitter_bound: vec![],
            memory_pressure: vec![],
            process_fault_attempt: vec![],
            pending_process_observations: Default::default(),
            fault_operation_sequence: 3,
        };
        let reference = FileSnapshotStore::new(dir.path())
            .put_snapshot(&snapshot, 1)
            .unwrap();
        let mut bug = minimal_bug(1);
        bug.replay_parent_depth = 1;
        bug.replay_parent_snapshot_ref = Some(reference.clone());
        save_checkpoint(&checkpoint_path, &minimal_checkpoint_with_bugs(vec![bug])).unwrap();

        let inspection = inspect_snapshot(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
        )
        .unwrap();
        assert_eq!(inspection.schema, "chaoscontrol.snapshot_inspection.v1");
        assert_eq!(inspection.tick, 750);
        assert_eq!(inspection.snapshot_digest, reference.digest);
        assert_eq!(inspection.vm_count, 0);
        assert_eq!(inspection.fault_operation_sequence, 3);
        assert!(inspection.checkpoint_digest.starts_with("b3:"));
    }

    #[test]
    fn verify_checkpoint_binds_an_expected_session_digest() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        save_checkpoint(
            &checkpoint_path,
            &minimal_checkpoint_with_bugs(vec![minimal_bug(1)]),
        )
        .unwrap();
        let admitted = verify_checkpoint(&checkpoint_path).unwrap();
        let bound = verify_checkpoint_with_digest(
            &checkpoint_path,
            Some(admitted.checkpoint_digest.as_str()),
        )
        .unwrap();
        assert_eq!(bound.checkpoint_digest, admitted.checkpoint_digest);

        let wrong = verify_checkpoint_with_digest(&checkpoint_path, Some("b3:00")).unwrap_err();
        assert!(matches!(
            wrong,
            CheckpointBugExportError::CheckpointDigestMismatch { .. }
        ));
    }

    #[test]
    fn verify_checkpoint_reports_missing_snapshot_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let mut bug = minimal_bug(1);
        bug.replay_parent_depth = 1;
        bug.replay_parent_snapshot_ref = Some(ReplayParentSnapshotRef {
            store: crate::snapshot_store::FILE_STORE_KIND.to_string(),
            digest: format!("sha256:{}", "0".repeat(64)),
            codec: crate::snapshot_store::SNAPSHOT_CODEC.to_string(),
            schema_version: 2,
            path: format!("snapshots/{}.snapshot.bin", "0".repeat(64)),
        });
        save_checkpoint(&checkpoint_path, &minimal_checkpoint_with_bugs(vec![bug])).unwrap();
        let verification = verify_checkpoint(&checkpoint_path).unwrap();
        assert!(!verification.valid);
        assert_eq!(verification.bugs_with_snapshot_ref, 1);
        assert_eq!(verification.snapshots_validated, 0);
        assert_eq!(verification.issues.len(), 1);
        assert_eq!(verification.issues[0].bug_id, 1);
    }

    #[test]
    fn bind_moment_refs_a_stable_tick_and_identity() {
        let dir = tempfile::tempdir().unwrap();
        let checkpoint_path = dir.path().join("checkpoint.json");
        let mut bug = minimal_bug(1);
        bug.tick = 4242;
        save_checkpoint(&checkpoint_path, &minimal_checkpoint_with_bugs(vec![bug])).unwrap();
        let moment = bind_moment(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
            Some(4242),
        )
        .unwrap();
        assert_eq!(moment.schema, "chaoscontrol.moment_reference.v2");
        assert_eq!(moment.tick, 4242);
        assert_eq!(moment.seek_granularity, "tick");
        assert!(moment.branch_id.starts_with("b3:"));
        let repeat = bind_moment(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
            Some(4242),
        )
        .unwrap();
        assert_eq!(repeat.branch_id, moment.branch_id);
        assert_eq!(moment.checkpoint_index, 0);
        assert_eq!(moment.artifact, "bug_0.json");
        assert!(moment.checkpoint_digest.starts_with("b3:"));
        assert_eq!(moment.snapshot_digest, None);

        let mismatch = bind_moment(
            &checkpoint_path,
            BugSelector {
                index: Some(0),
                dedup_key: None,
            },
            Some(1),
        )
        .unwrap_err();
        assert!(matches!(
            mismatch,
            CheckpointBugExportError::MomentTickMismatch {
                requested: 1,
                actual: 4242
            }
        ));
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
                dedup_key: None,
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
