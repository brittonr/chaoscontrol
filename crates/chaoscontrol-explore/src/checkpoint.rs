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

use crate::corpus::BugReport;
use chaoscontrol_fault::faults::{Fault, GpRegister};
use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::fs;
use std::path::Path;

/// Errors from checkpoint operations.
#[derive(Debug, Snafu)]
pub enum CheckpointError {
    #[snafu(display("I/O error"), context(false))]
    Io { source: std::io::Error },

    #[snafu(display("JSON error"), context(false))]
    Json { source: serde_json::Error },
}

/// Configuration subset needed to resume exploration.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

fn default_bootstrap_budget() -> u64 {
    10_000
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
        register: GpRegister,
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
            } => SerializableFault::MemoryPressure {
                target: *target,
                limit_bytes: *limit_bytes,
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
            } => Fault::MemoryPressure {
                target: *target,
                limit_bytes: *limit_bytes,
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
pub struct SerializableScheduledFault {
    pub time_ns: u64,
    pub fault: SerializableFault,
    pub label: Option<String>,
}

/// Serializable fault schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct SerializableBug {
    pub bug_id: u64,
    pub assertion_id: u64,
    pub assertion_location: String,
    pub schedule: SerializableSchedule,
    pub tick: u64,
}

impl From<&BugReport> for SerializableBug {
    fn from(bug: &BugReport) -> Self {
        SerializableBug {
            bug_id: bug.bug_id,
            assertion_id: bug.assertion_id,
            assertion_location: bug.assertion_location.clone(),
            schedule: (&bug.schedule).into(),
            tick: bug.tick,
        }
    }
}

/// Complete checkpoint — everything needed to resume exploration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationCheckpoint {
    pub config: CheckpointConfig,
    pub global_coverage: Vec<u8>,
    pub bugs: Vec<SerializableBug>,
    pub rounds_completed: u64,
    pub total_branches_run: u64,
    pub total_edges: usize,
    pub seed: u64,
    /// Per-round exploration history (optional for backward compat).
    #[serde(default)]
    pub round_history: Option<Vec<crate::explorer::RoundHistory>>,
}

/// Save a checkpoint to a JSON file.
pub fn save_checkpoint<P: AsRef<Path>>(
    path: P,
    checkpoint: &ExplorationCheckpoint,
) -> Result<(), CheckpointError> {
    let json = serde_json::to_string_pretty(checkpoint)?;
    fs::write(path, json)?;
    Ok(())
}

/// Load a checkpoint from a JSON file.
pub fn load_checkpoint<P: AsRef<Path>>(path: P) -> Result<ExplorationCheckpoint, CheckpointError> {
    let json = fs::read_to_string(path)?;
    let checkpoint = serde_json::from_str(&json)?;
    Ok(checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            },
            global_coverage: vec![1, 2, 3, 4, 5],
            bugs: vec![],
            rounds_completed: 10,
            total_branches_run: 80,
            total_edges: 1234,
            seed: 42,
            round_history: None,
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
            },
            global_coverage: vec![10, 20, 30],
            bugs: vec![],
            rounds_completed: 5,
            total_branches_run: 20,
            total_edges: 567,
            seed: 123,
            round_history: None,
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
            },
            global_coverage: vec![],
            bugs: vec![],
            rounds_completed: 2,
            total_branches_run: 16,
            total_edges: 52,
            seed: 1,
            round_history: Some(history.clone()),
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
}
