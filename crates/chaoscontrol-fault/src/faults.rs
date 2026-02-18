//! Fault type definitions for chaos engineering.
//!
//! Each fault variant represents a specific failure mode that can be
//! injected into a running VM.  Faults are deterministic: given the
//! same seed and schedule, the same faults fire at the same points.

use std::fmt;

/// A fault that can be injected into a running VM.
#[derive(Debug, Clone, PartialEq)]
pub enum Fault {
    // ── Network faults ──────────────────────────────────────────
    /// Partition: drop all packets between two sets of VMs.
    NetworkPartition {
        /// VMs on one side of the partition (by index).
        side_a: Vec<usize>,
        /// VMs on the other side.
        side_b: Vec<usize>,
    },

    /// Add latency to a VM's network (both send and receive).
    NetworkLatency {
        /// Target VM index.
        target: usize,
        /// Additional latency in nanoseconds.
        latency_ns: u64,
    },

    /// Drop packets to/from a VM with a given probability.
    PacketLoss {
        /// Target VM index.
        target: usize,
        /// Drop probability (0.0 = never, 1.0 = always).
        /// Stored as fixed-point: `rate_ppm` parts per million.
        rate_ppm: u32,
    },

    /// Corrupt packet payloads to/from a VM.
    PacketCorruption {
        /// Target VM index.
        target: usize,
        /// Corruption probability in parts per million.
        rate_ppm: u32,
    },

    /// Reorder packets within a time window.
    PacketReorder {
        /// Target VM index.
        target: usize,
        /// Reorder window in nanoseconds.
        window_ns: u64,
    },

    /// Heal all network partitions and remove network faults.
    NetworkHeal,

    // ── Disk faults ─────────────────────────────────────────────
    /// Inject a read I/O error at a specific block offset.
    DiskReadError {
        /// Target VM index.
        target: usize,
        /// Block offset that will fail.
        offset: u64,
    },

    /// Inject a write I/O error at a specific block offset.
    DiskWriteError {
        /// Target VM index.
        target: usize,
        /// Block offset that will fail.
        offset: u64,
    },

    /// Simulate a torn write (partial write + crash).
    DiskTornWrite {
        /// Target VM index.
        target: usize,
        /// Block offset.
        offset: u64,
        /// How many bytes actually get written before "crash".
        bytes_written: usize,
    },

    /// Corrupt data at a specific disk offset.
    DiskCorruption {
        /// Target VM index.
        target: usize,
        /// Offset to corrupt.
        offset: u64,
        /// Number of bytes to corrupt.
        len: usize,
    },

    /// Simulate disk full (all writes fail).
    DiskFull {
        /// Target VM index.
        target: usize,
    },

    // ── Process faults ──────────────────────────────────────────
    /// Kill (crash) a VM immediately.
    ProcessKill {
        /// VM index to kill.
        target: usize,
    },

    /// Pause a VM for a duration (simulates freeze/hang).
    ProcessPause {
        /// VM index to pause.
        target: usize,
        /// Pause duration in nanoseconds of virtual time.
        duration_ns: u64,
    },

    /// Restart a previously killed VM from its initial state.
    ProcessRestart {
        /// VM index to restart.
        target: usize,
    },

    // ── Clock faults ────────────────────────────────────────────
    /// Skew a VM's clock by a fixed offset (simulates NTP drift).
    ClockSkew {
        /// VM index.
        target: usize,
        /// Offset in nanoseconds (positive = fast, negative = slow).
        offset_ns: i64,
    },

    /// Jump a VM's clock suddenly (simulates NTP correction).
    ClockJump {
        /// VM index.
        target: usize,
        /// Jump amount in nanoseconds.
        delta_ns: i64,
    },

    // ── Resource faults ─────────────────────────────────────────
    /// Limit available memory for a VM.
    MemoryPressure {
        /// VM index.
        target: usize,
        /// Maximum usable memory in bytes.
        limit_bytes: u64,
    },
}

impl Fault {
    /// Get the target VM index, if this fault targets a specific VM.
    pub fn target(&self) -> Option<usize> {
        match self {
            Fault::NetworkPartition { .. } | Fault::NetworkHeal => None,
            Fault::NetworkLatency { target, .. }
            | Fault::PacketLoss { target, .. }
            | Fault::PacketCorruption { target, .. }
            | Fault::PacketReorder { target, .. }
            | Fault::DiskReadError { target, .. }
            | Fault::DiskWriteError { target, .. }
            | Fault::DiskTornWrite { target, .. }
            | Fault::DiskCorruption { target, .. }
            | Fault::DiskFull { target }
            | Fault::ProcessKill { target }
            | Fault::ProcessPause { target, .. }
            | Fault::ProcessRestart { target }
            | Fault::ClockSkew { target, .. }
            | Fault::ClockJump { target, .. }
            | Fault::MemoryPressure { target, .. } => Some(*target),
        }
    }

    /// Classify this fault by category.
    pub fn category(&self) -> FaultCategory {
        match self {
            Fault::NetworkPartition { .. }
            | Fault::NetworkLatency { .. }
            | Fault::PacketLoss { .. }
            | Fault::PacketCorruption { .. }
            | Fault::PacketReorder { .. }
            | Fault::NetworkHeal => FaultCategory::Network,

            Fault::DiskReadError { .. }
            | Fault::DiskWriteError { .. }
            | Fault::DiskTornWrite { .. }
            | Fault::DiskCorruption { .. }
            | Fault::DiskFull { .. } => FaultCategory::Disk,

            Fault::ProcessKill { .. }
            | Fault::ProcessPause { .. }
            | Fault::ProcessRestart { .. } => FaultCategory::Process,

            Fault::ClockSkew { .. } | Fault::ClockJump { .. } => FaultCategory::Clock,

            Fault::MemoryPressure { .. } => FaultCategory::Resource,
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fault::NetworkPartition { side_a, side_b } => {
                write!(f, "network-partition({side_a:?} | {side_b:?})")
            }
            Fault::NetworkLatency { target, latency_ns } => {
                write!(f, "network-latency(vm={target}, +{latency_ns}ns)")
            }
            Fault::PacketLoss { target, rate_ppm } => {
                write!(f, "packet-loss(vm={target}, {rate_ppm}ppm)")
            }
            Fault::PacketCorruption { target, rate_ppm } => {
                write!(f, "packet-corrupt(vm={target}, {rate_ppm}ppm)")
            }
            Fault::PacketReorder { target, window_ns } => {
                write!(f, "packet-reorder(vm={target}, {window_ns}ns)")
            }
            Fault::NetworkHeal => write!(f, "network-heal"),
            Fault::DiskReadError { target, offset } => {
                write!(f, "disk-read-error(vm={target}, offset={offset:#x})")
            }
            Fault::DiskWriteError { target, offset } => {
                write!(f, "disk-write-error(vm={target}, offset={offset:#x})")
            }
            Fault::DiskTornWrite {
                target,
                offset,
                bytes_written,
            } => write!(
                f,
                "disk-torn-write(vm={target}, offset={offset:#x}, partial={bytes_written})"
            ),
            Fault::DiskCorruption {
                target,
                offset,
                len,
            } => {
                write!(
                    f,
                    "disk-corrupt(vm={target}, offset={offset:#x}, len={len})"
                )
            }
            Fault::DiskFull { target } => write!(f, "disk-full(vm={target})"),
            Fault::ProcessKill { target } => write!(f, "process-kill(vm={target})"),
            Fault::ProcessPause {
                target,
                duration_ns,
            } => write!(f, "process-pause(vm={target}, {duration_ns}ns)"),
            Fault::ProcessRestart { target } => write!(f, "process-restart(vm={target})"),
            Fault::ClockSkew { target, offset_ns } => {
                write!(f, "clock-skew(vm={target}, {offset_ns}ns)")
            }
            Fault::ClockJump { target, delta_ns } => {
                write!(f, "clock-jump(vm={target}, {delta_ns}ns)")
            }
            Fault::MemoryPressure {
                target,
                limit_bytes,
            } => write!(f, "memory-pressure(vm={target}, limit={limit_bytes})"),
        }
    }
}

/// Broad category for a fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultCategory {
    Network,
    Disk,
    Process,
    Clock,
    Resource,
}

impl fmt::Display for FaultCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FaultCategory::Network => write!(f, "network"),
            FaultCategory::Disk => write!(f, "disk"),
            FaultCategory::Process => write!(f, "process"),
            FaultCategory::Clock => write!(f, "clock"),
            FaultCategory::Resource => write!(f, "resource"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fault_target_partition_is_none() {
        let f = Fault::NetworkPartition {
            side_a: vec![0],
            side_b: vec![1, 2],
        };
        assert_eq!(f.target(), None);
    }

    #[test]
    fn fault_target_latency_is_some() {
        let f = Fault::NetworkLatency {
            target: 2,
            latency_ns: 1000,
        };
        assert_eq!(f.target(), Some(2));
    }

    #[test]
    fn fault_category_classification() {
        assert_eq!(Fault::NetworkHeal.category(), FaultCategory::Network);
        assert_eq!(
            Fault::DiskFull { target: 0 }.category(),
            FaultCategory::Disk
        );
        assert_eq!(
            Fault::ProcessKill { target: 0 }.category(),
            FaultCategory::Process
        );
        assert_eq!(
            Fault::ClockSkew {
                target: 0,
                offset_ns: 0
            }
            .category(),
            FaultCategory::Clock
        );
        assert_eq!(
            Fault::MemoryPressure {
                target: 0,
                limit_bytes: 0
            }
            .category(),
            FaultCategory::Resource
        );
    }

    #[test]
    fn fault_display() {
        let f = Fault::ProcessKill { target: 1 };
        assert_eq!(f.to_string(), "process-kill(vm=1)");

        let f = Fault::NetworkPartition {
            side_a: vec![0],
            side_b: vec![1, 2],
        };
        assert_eq!(f.to_string(), "network-partition([0] | [1, 2])");
    }
}
