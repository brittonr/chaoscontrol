//! Fault type definitions for chaos engineering.
//!
//! Each fault variant represents a specific failure mode that can be
//! injected into a running VM.  Faults are deterministic: given the
//! same seed and schedule, the same faults fire at the same points.

use serde::{Deserialize, Serialize};
use std::fmt;

/// General-purpose register identifier for CPU fault injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GpRegister {
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    Rbp,
    Rsp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

impl fmt::Display for GpRegister {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            GpRegister::Rax => "rax",
            GpRegister::Rbx => "rbx",
            GpRegister::Rcx => "rcx",
            GpRegister::Rdx => "rdx",
            GpRegister::Rsi => "rsi",
            GpRegister::Rdi => "rdi",
            GpRegister::Rbp => "rbp",
            GpRegister::Rsp => "rsp",
            GpRegister::R8 => "r8",
            GpRegister::R9 => "r9",
            GpRegister::R10 => "r10",
            GpRegister::R11 => "r11",
            GpRegister::R12 => "r12",
            GpRegister::R13 => "r13",
            GpRegister::R14 => "r14",
            GpRegister::R15 => "r15",
        };
        write!(f, "{name}")
    }
}

impl GpRegister {
    /// All register variants for iteration.
    pub const ALL: [GpRegister; 16] = [
        GpRegister::Rax,
        GpRegister::Rbx,
        GpRegister::Rcx,
        GpRegister::Rdx,
        GpRegister::Rsi,
        GpRegister::Rdi,
        GpRegister::Rbp,
        GpRegister::Rsp,
        GpRegister::R8,
        GpRegister::R9,
        GpRegister::R10,
        GpRegister::R11,
        GpRegister::R12,
        GpRegister::R13,
        GpRegister::R14,
        GpRegister::R15,
    ];
}

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

    /// Add jitter (random latency variation) to a VM's network.
    ///
    /// Each packet receives up to `jitter_ns` extra random delay on
    /// top of the base latency.
    NetworkJitter {
        /// Target VM index.
        target: usize,
        /// Maximum additional random delay in nanoseconds.
        jitter_ns: u64,
    },

    /// Limit a VM's outgoing network bandwidth.
    ///
    /// Models serialization delay: large packets take longer on slow
    /// links, and back-to-back packets queue behind each other.
    NetworkBandwidth {
        /// Target VM index.
        target: usize,
        /// Maximum throughput in bytes per second (0 = unlimited).
        bytes_per_sec: u64,
    },

    /// Duplicate packets to/from a VM with a given probability.
    PacketDuplicate {
        /// Target VM index.
        target: usize,
        /// Duplication probability in parts per million.
        rate_ppm: u32,
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

    // ── Interrupt injection ─────────────────────────────────
    /// Inject a hardware interrupt (IRQ) into a VM via set_irq_line.
    ///
    /// Triggers the specified IRQ line in the in-kernel IRQ chip.
    /// Standard x86 IRQs:
    ///   0 = PIT timer, 4 = COM1 serial, 5-7 = virtio MMIO devices.
    InjectInterrupt {
        /// Target VM index.
        target: usize,
        /// IRQ line number (0-23 for standard x86 PIC/IOAPIC).
        irq: u32,
    },

    /// Inject a non-maskable interrupt (NMI) into a VM's vCPU.
    ///
    /// NMIs bypass interrupt masking and are delivered immediately.
    /// Used to test crash handlers, watchdog paths, and profiling code.
    InjectNmi {
        /// Target VM index.
        target: usize,
        /// Target vCPU index within the VM (0 = BSP).
        vcpu: usize,
    },

    // ── Advanced disk faults ────────────────────────────────
    /// Add per-operation latency to a VM's block device.
    ///
    /// Persists until cleared with `delay_ns: 0`.
    DiskSlow {
        /// Target VM index.
        target: usize,
        /// Additional delay per I/O operation in nanoseconds of virtual time.
        delay_ns: u64,
    },

    /// Enable fsync-lie mode: writes go to a volatile buffer that is
    /// discarded on ProcessKill. Models power-loss with writeback caching.
    DiskFsyncLie {
        /// Target VM index.
        target: usize,
    },

    /// Flush (commit) the volatile buffer created by DiskFsyncLie.
    DiskFsyncFlush {
        /// Target VM index.
        target: usize,
    },

    /// Return fewer bytes than requested on the next read at a
    /// specific offset. One-shot fault.
    DiskPartialRead {
        /// Target VM index.
        target: usize,
        /// Block offset that triggers the short read.
        offset: u64,
        /// Maximum bytes returned (rest zeroed).
        max_bytes: usize,
    },

    // ── CPU faults ──────────────────────────────────────────
    /// Flip a single bit in a general-purpose register.
    ///
    /// Models single-event upsets (cosmic ray bitflips, ECC failures).
    /// One-shot: the bit is flipped once at the tick boundary.
    CpuBitflip {
        /// Target VM index.
        target: usize,
        /// Target vCPU index within the VM.
        vcpu: usize,
        /// Which general-purpose register to corrupt.
        register: GpRegister,
        /// Bit position to flip (0–63). Values >= 64 are silently ignored.
        bit: u8,
    },

    /// Stall a single vCPU for a duration while other vCPUs continue.
    ///
    /// Models core C-state, thermal throttling, or microcode assist stall.
    CpuStall {
        /// Target VM index.
        target: usize,
        /// Target vCPU index within the VM.
        vcpu: usize,
        /// Number of ticks to stall.
        duration_ticks: u64,
    },

    // ── Advanced clock faults ───────────────────────────────
    /// Freeze the virtual TSC at its current value for a duration.
    ///
    /// The guest sees zero time progression during the freeze.
    /// After expiry, TSC resumes from the frozen value.
    ClockFreeze {
        /// Target VM index.
        target: usize,
        /// Number of ticks to hold the TSC frozen.
        duration_ticks: u64,
    },

    /// Add random per-exit TSC noise within a bound.
    ///
    /// Models an unstable oscillator. Jitter is cosmetic — the
    /// underlying VirtualTsc advances normally. Persists until
    /// cleared with `bound_tsc: 0`.
    ClockJitter {
        /// Target VM index.
        target: usize,
        /// Maximum jitter in TSC ticks (±bound). 0 = disabled.
        bound_tsc: u64,
    },
}

impl Fault {
    /// Get the target VM index, if this fault targets a specific VM.
    pub fn target(&self) -> Option<usize> {
        match self {
            Fault::NetworkPartition { .. } | Fault::NetworkHeal => None,
            Fault::NetworkLatency { target, .. }
            | Fault::NetworkJitter { target, .. }
            | Fault::NetworkBandwidth { target, .. }
            | Fault::PacketLoss { target, .. }
            | Fault::PacketCorruption { target, .. }
            | Fault::PacketReorder { target, .. }
            | Fault::PacketDuplicate { target, .. }
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
            | Fault::MemoryPressure { target, .. }
            | Fault::InjectInterrupt { target, .. }
            | Fault::InjectNmi { target, .. }
            | Fault::DiskSlow { target, .. }
            | Fault::DiskFsyncLie { target }
            | Fault::DiskFsyncFlush { target }
            | Fault::DiskPartialRead { target, .. }
            | Fault::CpuBitflip { target, .. }
            | Fault::CpuStall { target, .. }
            | Fault::ClockFreeze { target, .. }
            | Fault::ClockJitter { target, .. } => Some(*target),
        }
    }

    /// Classify this fault by category.
    pub fn category(&self) -> FaultCategory {
        match self {
            Fault::NetworkPartition { .. }
            | Fault::NetworkLatency { .. }
            | Fault::NetworkJitter { .. }
            | Fault::NetworkBandwidth { .. }
            | Fault::PacketLoss { .. }
            | Fault::PacketCorruption { .. }
            | Fault::PacketReorder { .. }
            | Fault::PacketDuplicate { .. }
            | Fault::NetworkHeal => FaultCategory::Network,

            Fault::DiskReadError { .. }
            | Fault::DiskWriteError { .. }
            | Fault::DiskTornWrite { .. }
            | Fault::DiskCorruption { .. }
            | Fault::DiskFull { .. }
            | Fault::DiskSlow { .. }
            | Fault::DiskFsyncLie { .. }
            | Fault::DiskFsyncFlush { .. }
            | Fault::DiskPartialRead { .. } => FaultCategory::Disk,

            Fault::ProcessKill { .. }
            | Fault::ProcessPause { .. }
            | Fault::ProcessRestart { .. } => FaultCategory::Process,

            Fault::ClockSkew { .. }
            | Fault::ClockJump { .. }
            | Fault::ClockFreeze { .. }
            | Fault::ClockJitter { .. } => FaultCategory::Clock,

            Fault::MemoryPressure { .. } => FaultCategory::Resource,

            Fault::InjectInterrupt { .. } | Fault::InjectNmi { .. } => FaultCategory::Interrupt,

            Fault::CpuBitflip { .. } | Fault::CpuStall { .. } => FaultCategory::Cpu,
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
            Fault::NetworkJitter { target, jitter_ns } => {
                write!(f, "network-jitter(vm={target}, ±{jitter_ns}ns)")
            }
            Fault::NetworkBandwidth {
                target,
                bytes_per_sec,
            } => {
                write!(f, "network-bandwidth(vm={target}, {bytes_per_sec}B/s)")
            }
            Fault::PacketDuplicate { target, rate_ppm } => {
                write!(f, "packet-duplicate(vm={target}, {rate_ppm}ppm)")
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
            Fault::InjectInterrupt { target, irq } => {
                write!(f, "inject-irq(vm={target}, irq={irq})")
            }
            Fault::InjectNmi { target, vcpu } => {
                write!(f, "inject-nmi(vm={target}, vcpu={vcpu})")
            }
            Fault::DiskSlow { target, delay_ns } => {
                write!(f, "disk-slow(vm={target}, {delay_ns}ns)")
            }
            Fault::DiskFsyncLie { target } => write!(f, "disk-fsync-lie(vm={target})"),
            Fault::DiskFsyncFlush { target } => write!(f, "disk-fsync-flush(vm={target})"),
            Fault::DiskPartialRead {
                target,
                offset,
                max_bytes,
            } => write!(
                f,
                "disk-partial-read(vm={target}, offset={offset:#x}, max={max_bytes})"
            ),
            Fault::CpuBitflip {
                target,
                vcpu,
                register,
                bit,
            } => write!(
                f,
                "cpu-bitflip(vm={target}, vcpu={vcpu}, {register}[{bit}])"
            ),
            Fault::CpuStall {
                target,
                vcpu,
                duration_ticks,
            } => write!(
                f,
                "cpu-stall(vm={target}, vcpu={vcpu}, {duration_ticks} ticks)"
            ),
            Fault::ClockFreeze {
                target,
                duration_ticks,
            } => write!(f, "clock-freeze(vm={target}, {duration_ticks} ticks)"),
            Fault::ClockJitter { target, bound_tsc } => {
                write!(f, "clock-jitter(vm={target}, ±{bound_tsc} tsc)")
            }
        }
    }
}

impl Fault {
    /// Short discriminant name (no parameters) for dedup hashing.
    pub fn type_name(&self) -> &'static str {
        match self {
            Fault::NetworkPartition { .. } => "network-partition",
            Fault::NetworkLatency { .. } => "network-latency",
            Fault::PacketLoss { .. } => "packet-loss",
            Fault::PacketCorruption { .. } => "packet-corrupt",
            Fault::PacketReorder { .. } => "packet-reorder",
            Fault::NetworkJitter { .. } => "network-jitter",
            Fault::NetworkBandwidth { .. } => "network-bandwidth",
            Fault::PacketDuplicate { .. } => "packet-duplicate",
            Fault::NetworkHeal => "network-heal",
            Fault::DiskReadError { .. } => "disk-read-error",
            Fault::DiskWriteError { .. } => "disk-write-error",
            Fault::DiskTornWrite { .. } => "disk-torn-write",
            Fault::DiskCorruption { .. } => "disk-corrupt",
            Fault::DiskFull { .. } => "disk-full",
            Fault::ProcessKill { .. } => "process-kill",
            Fault::ProcessPause { .. } => "process-pause",
            Fault::ProcessRestart { .. } => "process-restart",
            Fault::ClockSkew { .. } => "clock-skew",
            Fault::ClockJump { .. } => "clock-jump",
            Fault::MemoryPressure { .. } => "memory-pressure",
            Fault::InjectInterrupt { .. } => "inject-irq",
            Fault::InjectNmi { .. } => "inject-nmi",
            Fault::DiskSlow { .. } => "disk-slow",
            Fault::DiskFsyncLie { .. } => "disk-fsync-lie",
            Fault::DiskFsyncFlush { .. } => "disk-fsync-flush",
            Fault::DiskPartialRead { .. } => "disk-partial-read",
            Fault::CpuBitflip { .. } => "cpu-bitflip",
            Fault::CpuStall { .. } => "cpu-stall",
            Fault::ClockFreeze { .. } => "clock-freeze",
            Fault::ClockJitter { .. } => "clock-jitter",
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
    Interrupt,
    Cpu,
}

impl fmt::Display for FaultCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FaultCategory::Network => write!(f, "network"),
            FaultCategory::Disk => write!(f, "disk"),
            FaultCategory::Process => write!(f, "process"),
            FaultCategory::Clock => write!(f, "clock"),
            FaultCategory::Resource => write!(f, "resource"),
            FaultCategory::Interrupt => write!(f, "interrupt"),
            FaultCategory::Cpu => write!(f, "cpu"),
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

    #[test]
    fn fault_target_inject_interrupt() {
        let f = Fault::InjectInterrupt { target: 1, irq: 4 };
        assert_eq!(f.target(), Some(1));
    }

    #[test]
    fn fault_target_inject_nmi() {
        let f = Fault::InjectNmi { target: 2, vcpu: 0 };
        assert_eq!(f.target(), Some(2));
    }

    #[test]
    fn fault_category_interrupt() {
        assert_eq!(
            Fault::InjectInterrupt { target: 0, irq: 0 }.category(),
            FaultCategory::Interrupt
        );
        assert_eq!(
            Fault::InjectNmi { target: 0, vcpu: 0 }.category(),
            FaultCategory::Interrupt
        );
    }

    #[test]
    fn fault_display_interrupt_variants() {
        let f = Fault::InjectInterrupt { target: 1, irq: 5 };
        assert_eq!(f.to_string(), "inject-irq(vm=1, irq=5)");

        let f = Fault::InjectNmi { target: 0, vcpu: 0 };
        assert_eq!(f.to_string(), "inject-nmi(vm=0, vcpu=0)");
    }

    #[test]
    fn fault_category_cpu() {
        assert_eq!(
            Fault::CpuBitflip {
                target: 0,
                vcpu: 0,
                register: GpRegister::Rax,
                bit: 12
            }
            .category(),
            FaultCategory::Cpu
        );
        assert_eq!(
            Fault::CpuStall {
                target: 0,
                vcpu: 0,
                duration_ticks: 50
            }
            .category(),
            FaultCategory::Cpu
        );
    }

    #[test]
    fn fault_category_new_disk_variants() {
        assert_eq!(
            Fault::DiskSlow {
                target: 0,
                delay_ns: 1000
            }
            .category(),
            FaultCategory::Disk
        );
        assert_eq!(
            Fault::DiskFsyncLie { target: 0 }.category(),
            FaultCategory::Disk
        );
        assert_eq!(
            Fault::DiskFsyncFlush { target: 0 }.category(),
            FaultCategory::Disk
        );
        assert_eq!(
            Fault::DiskPartialRead {
                target: 0,
                offset: 0,
                max_bytes: 256
            }
            .category(),
            FaultCategory::Disk
        );
    }

    #[test]
    fn fault_category_new_clock_variants() {
        assert_eq!(
            Fault::ClockFreeze {
                target: 0,
                duration_ticks: 100
            }
            .category(),
            FaultCategory::Clock
        );
        assert_eq!(
            Fault::ClockJitter {
                target: 0,
                bound_tsc: 500
            }
            .category(),
            FaultCategory::Clock
        );
    }

    #[test]
    fn fault_target_new_variants() {
        assert_eq!(
            Fault::DiskSlow {
                target: 3,
                delay_ns: 0
            }
            .target(),
            Some(3)
        );
        assert_eq!(Fault::DiskFsyncLie { target: 1 }.target(), Some(1));
        assert_eq!(Fault::DiskFsyncFlush { target: 2 }.target(), Some(2));
        assert_eq!(
            Fault::DiskPartialRead {
                target: 0,
                offset: 0,
                max_bytes: 128
            }
            .target(),
            Some(0)
        );
        assert_eq!(
            Fault::CpuBitflip {
                target: 1,
                vcpu: 0,
                register: GpRegister::Rcx,
                bit: 7
            }
            .target(),
            Some(1)
        );
        assert_eq!(
            Fault::CpuStall {
                target: 2,
                vcpu: 1,
                duration_ticks: 10
            }
            .target(),
            Some(2)
        );
        assert_eq!(
            Fault::ClockFreeze {
                target: 0,
                duration_ticks: 50
            }
            .target(),
            Some(0)
        );
        assert_eq!(
            Fault::ClockJitter {
                target: 1,
                bound_tsc: 100
            }
            .target(),
            Some(1)
        );
    }

    #[test]
    fn fault_display_new_variants() {
        assert_eq!(
            Fault::DiskSlow {
                target: 0,
                delay_ns: 5_000_000
            }
            .to_string(),
            "disk-slow(vm=0, 5000000ns)"
        );
        assert_eq!(
            Fault::DiskFsyncLie { target: 1 }.to_string(),
            "disk-fsync-lie(vm=1)"
        );
        assert_eq!(
            Fault::CpuBitflip {
                target: 0,
                vcpu: 0,
                register: GpRegister::R15,
                bit: 63
            }
            .to_string(),
            "cpu-bitflip(vm=0, vcpu=0, r15[63])"
        );
        assert_eq!(
            Fault::ClockFreeze {
                target: 0,
                duration_ticks: 100
            }
            .to_string(),
            "clock-freeze(vm=0, 100 ticks)"
        );
    }

    #[test]
    fn gp_register_all_count() {
        assert_eq!(GpRegister::ALL.len(), 16);
    }

    #[test]
    fn gp_register_serde_roundtrip() {
        for reg in &GpRegister::ALL {
            let json = serde_json::to_string(reg).unwrap();
            let restored: GpRegister = serde_json::from_str(&json).unwrap();
            assert_eq!(*reg, restored);
        }
    }
}
