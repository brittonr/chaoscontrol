//! Deterministic fault planning and outcome accounting.
//!
//! This module is the functional core for fault application evidence. It does
//! not access devices, KVM, files, clocks, processes, logs, or ambient state.

use crate::faults::{Fault, FaultCategory, FaultVariant, GpRegister};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const PARTS_PER_MILLION_MAX: u32 = 1_000_000;
pub const NANOSECONDS_PER_SIMULATION_TICK: u64 = 1_000_000;
pub const MAX_FAULT_OUTCOME_EVENTS: usize = 65_536;
pub const MAX_FAULT_ATTEMPTS: usize = 16_384;
pub const MAX_OBSERVATIONS_PER_ATTEMPT: usize = MAX_FAULT_OUTCOME_EVENTS;
pub const MAX_FAULT_VMS: usize = 256;
pub const MAX_PARTITION_MEMBERS: usize = MAX_FAULT_VMS;
pub const GENERAL_REGISTER_BIT_COUNT: u8 = 64;
pub const MAX_STANDARD_IRQ_LINE: u32 = 23;
pub const PROCESS_RESTART_DELAY_TICKS: u64 = 10;
const SECONDS_PER_DAY: u64 = 86_400;
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;
pub const DEFAULT_MAX_FAULT_DURATION_NS: u64 = SECONDS_PER_DAY * NANOSECONDS_PER_SECOND;
pub const DEFAULT_MAX_FAULT_DURATION_TICKS: u64 =
    DEFAULT_MAX_FAULT_DURATION_NS / NANOSECONDS_PER_SIMULATION_TICK;
const FAULT_ATTEMPT_DOMAIN: &[u8] = b"chaoscontrol.fault-attempt.v1";
const FAULT_RUN_DOMAIN: &[u8] = b"chaoscontrol.fault-run.v1";
const FAULT_SCHEDULE_DOMAIN: &[u8] = b"chaoscontrol.fault-schedule.v1";
const FAULT_OPERATION_DOMAIN: &[u8] = b"chaoscontrol.fault-operation.v1";
const RANDOM_SCHEDULE_ENTRY_DOMAIN: &[u8] = b"random";

/// Stable BLAKE3 identity for one schedule.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct FaultScheduleId(pub [u8; 32]);

/// Stable BLAKE3 identity for one engine run.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct FaultRunId(pub [u8; 32]);

/// Stable BLAKE3 identity for one selected fault.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct FaultAttemptId(pub [u8; 32]);

/// Stable BLAKE3 identity for one operation affected by a fault.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct FaultOperationId(pub [u8; 32]);

macro_rules! impl_hex_display {
    ($type_name:ty) => {
        impl fmt::Display for $type_name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in self.0 {
                    write!(formatter, "{byte:02x}")?;
                }
                Ok(())
            }
        }
    };
}

impl_hex_display!(FaultScheduleId);
impl_hex_display!(FaultRunId);
impl_hex_display!(FaultAttemptId);
impl_hex_display!(FaultOperationId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultAttemptSource {
    Direct,
    Scheduled {
        entry_index: u64,
        scheduled_at_ns: u64,
    },
    Random,
}

/// One deterministic selection from a schedule or the seeded random source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultAttempt {
    pub id: FaultAttemptId,
    pub run_id: FaultRunId,
    pub run_sequence: u64,
    pub schedule_id: FaultScheduleId,
    pub selection_index: u64,
    pub selected_at_ns: u64,
    pub source: FaultAttemptSource,
    pub fault: Fault,
}

impl FaultAttempt {
    pub fn new(
        run_id: FaultRunId,
        run_sequence: u64,
        schedule_id: FaultScheduleId,
        selection_index: u64,
        selected_at_ns: u64,
        fault: Fault,
    ) -> Self {
        Self::new_with_source(
            run_id,
            run_sequence,
            schedule_id,
            selection_index,
            selected_at_ns,
            FaultAttemptSource::Direct,
            fault,
        )
    }

    pub fn new_with_source(
        run_id: FaultRunId,
        run_sequence: u64,
        schedule_id: FaultScheduleId,
        selection_index: u64,
        selected_at_ns: u64,
        source: FaultAttemptSource,
        fault: Fault,
    ) -> Self {
        let id = fault_attempt_id(
            run_id,
            schedule_id,
            selection_index,
            selected_at_ns,
            source,
            &fault,
        );
        Self {
            id,
            run_id,
            run_sequence,
            schedule_id,
            selection_index,
            selected_at_ns,
            source,
            fault,
        }
    }

    pub fn has_valid_identity(&self) -> bool {
        self.id
            == fault_attempt_id(
                self.run_id,
                self.schedule_id,
                self.selection_index,
                self.selected_at_ns,
                self.source,
                &self.fault,
            )
    }
}

pub fn fault_schedule_id<'a>(
    entries: impl IntoIterator<Item = (u64, Option<&'a str>, &'a Fault)>,
) -> FaultScheduleId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(FAULT_SCHEDULE_DOMAIN);
    for (time_ns, label, fault) in entries {
        hasher.update(&time_ns.to_le_bytes());
        hash_optional_text(&mut hasher, label);
        hash_fault(&mut hasher, fault);
    }
    FaultScheduleId(*hasher.finalize().as_bytes())
}

pub fn random_fault_schedule_id(seed: u64) -> FaultScheduleId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(FAULT_SCHEDULE_DOMAIN);
    hasher.update(RANDOM_SCHEDULE_ENTRY_DOMAIN);
    hasher.update(&seed.to_le_bytes());
    FaultScheduleId(*hasher.finalize().as_bytes())
}

pub fn fault_run_id(seed: u64, run_sequence: u64, schedule_id: FaultScheduleId) -> FaultRunId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(FAULT_RUN_DOMAIN);
    hasher.update(&seed.to_le_bytes());
    hasher.update(&run_sequence.to_le_bytes());
    hasher.update(&schedule_id.0);
    FaultRunId(*hasher.finalize().as_bytes())
}

pub fn fault_operation_id(
    attempt_id: FaultAttemptId,
    subsystem: FaultObservationSubsystem,
    operation_sequence: u64,
) -> FaultOperationId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(FAULT_OPERATION_DOMAIN);
    hasher.update(&attempt_id.0);
    hasher.update(&[subsystem as u8]);
    hasher.update(&operation_sequence.to_le_bytes());
    FaultOperationId(*hasher.finalize().as_bytes())
}

fn fault_attempt_id(
    run_id: FaultRunId,
    schedule_id: FaultScheduleId,
    selection_index: u64,
    selected_at_ns: u64,
    source: FaultAttemptSource,
    fault: &Fault,
) -> FaultAttemptId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(FAULT_ATTEMPT_DOMAIN);
    hasher.update(&run_id.0);
    hasher.update(&schedule_id.0);
    hasher.update(&selection_index.to_le_bytes());
    hasher.update(&selected_at_ns.to_le_bytes());
    match source {
        FaultAttemptSource::Direct => {
            hasher.update(&[0]);
        }
        FaultAttemptSource::Scheduled {
            entry_index,
            scheduled_at_ns,
        } => {
            hasher.update(&[1]);
            hasher.update(&entry_index.to_le_bytes());
            hasher.update(&scheduled_at_ns.to_le_bytes());
        }
        FaultAttemptSource::Random => {
            hasher.update(&[2]);
        }
    }
    hash_fault(&mut hasher, fault);
    FaultAttemptId(*hasher.finalize().as_bytes())
}

fn hash_optional_text(hasher: &mut blake3::Hasher, text: Option<&str>) {
    match text {
        Some(value) => {
            hasher.update(&[1]);
            hash_bytes(hasher, value.as_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn hash_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
}

fn hash_usize(hasher: &mut blake3::Hasher, value: usize) {
    let value_u64 = u64::try_from(value).unwrap_or(u64::MAX);
    hasher.update(&value_u64.to_le_bytes());
}

fn hash_usize_slice(hasher: &mut blake3::Hasher, values: &[usize]) {
    hash_usize(hasher, values.len());
    for value in values {
        hash_usize(hasher, *value);
    }
}

fn hash_fault(hasher: &mut blake3::Hasher, fault: &Fault) {
    hash_bytes(hasher, fault.type_name().as_bytes());
    match fault.category() {
        FaultCategory::Network => hash_network_fault(hasher, fault),
        FaultCategory::Disk => hash_disk_fault(hasher, fault),
        FaultCategory::Process => hash_process_fault(hasher, fault),
        FaultCategory::Clock => hash_clock_fault(hasher, fault),
        FaultCategory::Resource => hash_resource_fault(hasher, fault),
        FaultCategory::Interrupt => hash_interrupt_fault(hasher, fault),
        FaultCategory::Cpu => hash_cpu_fault(hasher, fault),
    }
}

fn hash_network_fault(hasher: &mut blake3::Hasher, fault: &Fault) {
    match fault {
        Fault::NetworkPartition { side_a, side_b } => {
            hash_usize_slice(hasher, side_a);
            hash_usize_slice(hasher, side_b);
        }
        Fault::NetworkLatency { target, latency_ns } => {
            hash_usize(hasher, *target);
            hasher.update(&latency_ns.to_le_bytes());
        }
        Fault::PacketLoss { target, rate_ppm }
        | Fault::PacketCorruption { target, rate_ppm }
        | Fault::PacketDuplicate { target, rate_ppm } => {
            hash_usize(hasher, *target);
            hasher.update(&rate_ppm.to_le_bytes());
        }
        Fault::PacketReorder { target, window_ns } => {
            hash_usize(hasher, *target);
            hasher.update(&window_ns.to_le_bytes());
        }
        Fault::NetworkJitter { target, jitter_ns } => {
            hash_usize(hasher, *target);
            hasher.update(&jitter_ns.to_le_bytes());
        }
        Fault::NetworkBandwidth {
            target,
            bytes_per_sec,
        } => {
            hash_usize(hasher, *target);
            hasher.update(&bytes_per_sec.to_le_bytes());
        }
        Fault::NetworkHeal => {}
        _ => unreachable!("network category must contain a network fault"),
    }
}

fn hash_disk_fault(hasher: &mut blake3::Hasher, fault: &Fault) {
    match fault {
        Fault::DiskReadError { target, offset } | Fault::DiskWriteError { target, offset } => {
            hash_usize(hasher, *target);
            hasher.update(&offset.to_le_bytes());
        }
        Fault::DiskTornWrite {
            target,
            offset,
            bytes_written,
        } => {
            hash_usize(hasher, *target);
            hasher.update(&offset.to_le_bytes());
            hash_usize(hasher, *bytes_written);
        }
        Fault::DiskCorruption {
            target,
            offset,
            len,
        } => {
            hash_usize(hasher, *target);
            hasher.update(&offset.to_le_bytes());
            hash_usize(hasher, *len);
        }
        Fault::DiskFull { target }
        | Fault::DiskFsyncLie { target }
        | Fault::DiskFsyncFlush { target } => hash_usize(hasher, *target),
        Fault::DiskSlow { target, delay_ns } => {
            hash_usize(hasher, *target);
            hasher.update(&delay_ns.to_le_bytes());
        }
        Fault::DiskPartialRead {
            target,
            offset,
            max_bytes,
        } => {
            hash_usize(hasher, *target);
            hasher.update(&offset.to_le_bytes());
            hash_usize(hasher, *max_bytes);
        }
        _ => unreachable!("disk category must contain a disk fault"),
    }
}

fn hash_process_fault(hasher: &mut blake3::Hasher, fault: &Fault) {
    match fault {
        Fault::ProcessKill { target } | Fault::ProcessRestart { target } => {
            hash_usize(hasher, *target);
        }
        Fault::ProcessPause {
            target,
            duration_ns,
        } => {
            hash_usize(hasher, *target);
            hasher.update(&duration_ns.to_le_bytes());
        }
        _ => unreachable!("process category must contain a process fault"),
    }
}

fn hash_clock_fault(hasher: &mut blake3::Hasher, fault: &Fault) {
    match fault {
        Fault::ClockSkew { target, offset_ns } => {
            hash_usize(hasher, *target);
            hasher.update(&offset_ns.to_le_bytes());
        }
        Fault::ClockJump { target, delta_ns } => {
            hash_usize(hasher, *target);
            hasher.update(&delta_ns.to_le_bytes());
        }
        Fault::ClockFreeze {
            target,
            duration_ticks,
        } => {
            hash_usize(hasher, *target);
            hasher.update(&duration_ticks.to_le_bytes());
        }
        Fault::ClockJitter { target, bound_tsc } => {
            hash_usize(hasher, *target);
            hasher.update(&bound_tsc.to_le_bytes());
        }
        _ => unreachable!("clock category must contain a clock fault"),
    }
}

fn hash_resource_fault(hasher: &mut blake3::Hasher, fault: &Fault) {
    match fault {
        Fault::MemoryPressure {
            target,
            limit_bytes,
            duration_ticks,
        } => {
            hash_usize(hasher, *target);
            hasher.update(&limit_bytes.to_le_bytes());
            hasher.update(&duration_ticks.to_le_bytes());
        }
        _ => unreachable!("resource category must contain a resource fault"),
    }
}

fn hash_interrupt_fault(hasher: &mut blake3::Hasher, fault: &Fault) {
    match fault {
        Fault::InjectInterrupt { target, irq } => {
            hash_usize(hasher, *target);
            hasher.update(&irq.to_le_bytes());
        }
        Fault::InjectNmi { target, vcpu } => {
            hash_usize(hasher, *target);
            hash_usize(hasher, *vcpu);
        }
        _ => unreachable!("interrupt category must contain an interrupt fault"),
    }
}

fn hash_cpu_fault(hasher: &mut blake3::Hasher, fault: &Fault) {
    match fault {
        Fault::CpuBitflip {
            target,
            vcpu,
            register,
            bit,
        } => {
            hash_usize(hasher, *target);
            hash_usize(hasher, *vcpu);
            hasher.update(&[register_index(*register)]);
            hasher.update(&[*bit]);
        }
        Fault::CpuStall {
            target,
            vcpu,
            duration_ticks,
        } => {
            hash_usize(hasher, *target);
            hash_usize(hasher, *vcpu);
            hasher.update(&duration_ticks.to_le_bytes());
        }
        _ => unreachable!("CPU category must contain a CPU fault"),
    }
}

fn register_index(register: GpRegister) -> u8 {
    match register {
        GpRegister::Rax => 0,
        GpRegister::Rbx => 1,
        GpRegister::Rcx => 2,
        GpRegister::Rdx => 3,
        GpRegister::Rsi => 4,
        GpRegister::Rdi => 5,
        GpRegister::Rbp => 6,
        GpRegister::Rsp => 7,
        GpRegister::R8 => 8,
        GpRegister::R9 => 9,
        GpRegister::R10 => 10,
        GpRegister::R11 => 11,
        GpRegister::R12 => 12,
        GpRegister::R13 => 13,
        GpRegister::R14 => 14,
        GpRegister::R15 => 15,
    }
}

/// Controller policy for applicability decisions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultApplicationPolicy {
    pub rejection_is_fatal: bool,
    pub max_duration_ns: u64,
    pub max_duration_ticks: u64,
    pub max_partition_members: u32,
}

impl Default for FaultApplicationPolicy {
    fn default() -> Self {
        Self {
            rejection_is_fatal: false,
            max_duration_ns: DEFAULT_MAX_FAULT_DURATION_NS,
            max_duration_ticks: DEFAULT_MAX_FAULT_DURATION_TICKS,
            max_partition_members: u32::try_from(MAX_PARTITION_MEMBERS).unwrap_or(u32::MAX),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultVmStatus {
    Running,
    Paused,
    Crashed,
    Restarting,
    Resuming,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VmFaultFacts {
    pub status: FaultVmStatus,
    pub vcpu_count: u32,
    pub memory_size_bytes: u64,
    pub block_device_size_bytes: Option<u64>,
    pub has_initial_snapshot: bool,
    pub supports_irq: bool,
    pub supports_nmi: bool,
    pub supports_clock_freeze: bool,
    pub supports_clock_jitter: bool,
    pub supports_cpu_stall: bool,
    pub supports_memory_pressure: bool,
    pub virtual_tsc: u64,
    pub tsc_khz: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultPlanningFacts {
    pub current_tick: u64,
    pub network_supported: bool,
    pub vms: Vec<VmFaultFacts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultEffectTiming {
    Immediate,
    Armed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultMechanism {
    NetworkPartition,
    NetworkLatency,
    PacketLoss,
    PacketCorruption,
    PacketReorder,
    NetworkJitter,
    NetworkBandwidth,
    PacketDuplicate,
    NetworkHeal,
    BlockReadError,
    BlockWriteError,
    BlockTornWrite,
    BlockCorruption,
    BlockFull,
    ProcessKill,
    ProcessPause,
    ProcessRestart,
    VirtualClockSkew,
    VirtualClockJump,
    IrqInjection,
    NmiInjection,
    BlockSlow,
    BlockFsyncLie,
    BlockFsyncFlush,
    BlockPartialRead,
    CpuRegisterBitflip,
    VirtualClockFreeze,
    VirtualClockJitter,
    CpuStall,
    MemoryPressure,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultPlan {
    pub attempt_id: FaultAttemptId,
    pub effect: FaultPlanEffect,
}

impl FaultPlan {
    pub fn mechanism(&self) -> FaultMechanism {
        self.effect.mechanism()
    }

    pub fn timing(&self) -> FaultEffectTiming {
        self.effect.timing()
    }

    pub fn max_immediate_observations(&self) -> usize {
        const SINGLE_IMMEDIATE_OBSERVATION: usize = 1;
        match self.effect {
            FaultPlanEffect::VirtualClockSkew { .. }
            | FaultPlanEffect::VirtualClockJump { .. }
            | FaultPlanEffect::IrqInjection { .. }
            | FaultPlanEffect::NmiInjection { .. }
            | FaultPlanEffect::CpuRegisterBitflip { .. }
            | FaultPlanEffect::VirtualClockFreeze { .. }
            | FaultPlanEffect::VirtualClockJitter { .. }
            | FaultPlanEffect::CpuStall { .. }
            | FaultPlanEffect::MemoryPressure { .. } => SINGLE_IMMEDIATE_OBSERVATION,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultPlanEffect {
    NetworkPartition {
        side_a: Vec<u32>,
        side_b: Vec<u32>,
    },
    NetworkLatency {
        target: u32,
        latency_ticks: u64,
    },
    PacketLoss {
        target: u32,
        rate_ppm: u32,
    },
    PacketCorruption {
        target: u32,
        rate_ppm: u32,
    },
    PacketReorder {
        target: u32,
        window_ticks: u64,
    },
    NetworkJitter {
        target: u32,
        jitter_ticks: u64,
    },
    NetworkBandwidth {
        target: u32,
        bytes_per_sec: u64,
    },
    PacketDuplicate {
        target: u32,
        rate_ppm: u32,
    },
    NetworkHeal,
    BlockReadError {
        target: u32,
        offset: u64,
    },
    BlockWriteError {
        target: u32,
        offset: u64,
    },
    BlockTornWrite {
        target: u32,
        offset: u64,
        bytes_written: u64,
    },
    BlockCorruption {
        target: u32,
        offset: u64,
        len: u64,
    },
    BlockFull {
        target: u32,
    },
    ProcessKill {
        target: u32,
    },
    ProcessPause {
        target: u32,
        resume_at_tick: u64,
    },
    ProcessRestart {
        target: u32,
        restart_at_tick: u64,
    },
    VirtualClockSkew {
        target: u32,
        basis_tsc: u64,
        tsc_khz: u32,
        offset_ns: i64,
        tsc_delta: i64,
        target_tsc: u64,
    },
    VirtualClockJump {
        target: u32,
        basis_tsc: u64,
        tsc_khz: u32,
        delta_ns: i64,
        tsc_delta: i64,
        target_tsc: u64,
    },
    IrqInjection {
        target: u32,
        irq: u32,
    },
    NmiInjection {
        target: u32,
        vcpu: u32,
    },
    BlockSlow {
        target: u32,
        delay_ns: u64,
    },
    BlockFsyncLie {
        target: u32,
    },
    BlockFsyncFlush {
        target: u32,
    },
    BlockPartialRead {
        target: u32,
        offset: u64,
        max_bytes: u64,
    },
    CpuRegisterBitflip {
        target: u32,
        vcpu: u32,
        register: GpRegister,
        bit: u8,
    },
    VirtualClockFreeze {
        target: u32,
        frozen_tsc: u64,
        release_at_tick: u64,
    },
    VirtualClockJitter {
        target: u32,
        bound_tsc: u64,
    },
    CpuStall {
        target: u32,
        vcpu: u32,
        release_at_tick: u64,
    },
    MemoryPressure {
        target: u32,
        limit_bytes: u64,
        baseline_bytes: u64,
        release_at_tick: u64,
    },
}

impl FaultPlanEffect {
    pub fn mechanism(&self) -> FaultMechanism {
        match self {
            Self::NetworkPartition { .. } => FaultMechanism::NetworkPartition,
            Self::NetworkLatency { .. } => FaultMechanism::NetworkLatency,
            Self::PacketLoss { .. } => FaultMechanism::PacketLoss,
            Self::PacketCorruption { .. } => FaultMechanism::PacketCorruption,
            Self::PacketReorder { .. } => FaultMechanism::PacketReorder,
            Self::NetworkJitter { .. } => FaultMechanism::NetworkJitter,
            Self::NetworkBandwidth { .. } => FaultMechanism::NetworkBandwidth,
            Self::PacketDuplicate { .. } => FaultMechanism::PacketDuplicate,
            Self::NetworkHeal => FaultMechanism::NetworkHeal,
            Self::BlockReadError { .. } => FaultMechanism::BlockReadError,
            Self::BlockWriteError { .. } => FaultMechanism::BlockWriteError,
            Self::BlockTornWrite { .. } => FaultMechanism::BlockTornWrite,
            Self::BlockCorruption { .. } => FaultMechanism::BlockCorruption,
            Self::BlockFull { .. } => FaultMechanism::BlockFull,
            Self::ProcessKill { .. } => FaultMechanism::ProcessKill,
            Self::ProcessPause { .. } => FaultMechanism::ProcessPause,
            Self::ProcessRestart { .. } => FaultMechanism::ProcessRestart,
            Self::VirtualClockSkew { .. } => FaultMechanism::VirtualClockSkew,
            Self::VirtualClockJump { .. } => FaultMechanism::VirtualClockJump,
            Self::IrqInjection { .. } => FaultMechanism::IrqInjection,
            Self::NmiInjection { .. } => FaultMechanism::NmiInjection,
            Self::BlockSlow { .. } => FaultMechanism::BlockSlow,
            Self::BlockFsyncLie { .. } => FaultMechanism::BlockFsyncLie,
            Self::BlockFsyncFlush { .. } => FaultMechanism::BlockFsyncFlush,
            Self::BlockPartialRead { .. } => FaultMechanism::BlockPartialRead,
            Self::CpuRegisterBitflip { .. } => FaultMechanism::CpuRegisterBitflip,
            Self::VirtualClockFreeze { .. } => FaultMechanism::VirtualClockFreeze,
            Self::VirtualClockJitter { .. } => FaultMechanism::VirtualClockJitter,
            Self::CpuStall { .. } => FaultMechanism::CpuStall,
            Self::MemoryPressure { .. } => FaultMechanism::MemoryPressure,
        }
    }

    pub fn timing(&self) -> FaultEffectTiming {
        match self {
            Self::NetworkLatency {
                latency_ticks: 0, ..
            }
            | Self::PacketLoss { rate_ppm: 0, .. }
            | Self::PacketCorruption { rate_ppm: 0, .. }
            | Self::PacketReorder {
                window_ticks: 0, ..
            }
            | Self::NetworkJitter {
                jitter_ticks: 0, ..
            }
            | Self::NetworkBandwidth {
                bytes_per_sec: 0, ..
            }
            | Self::PacketDuplicate { rate_ppm: 0, .. }
            | Self::BlockSlow { delay_ns: 0, .. }
            | Self::NetworkHeal
            | Self::ProcessKill { .. }
            | Self::VirtualClockSkew { .. }
            | Self::VirtualClockJump { .. }
            | Self::IrqInjection { .. }
            | Self::NmiInjection { .. }
            | Self::BlockFsyncFlush { .. }
            | Self::CpuRegisterBitflip { .. }
            | Self::VirtualClockFreeze { .. }
            | Self::VirtualClockJitter { .. }
            | Self::CpuStall { .. }
            | Self::MemoryPressure { .. } => FaultEffectTiming::Immediate,
            _ => FaultEffectTiming::Armed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultRejectionReason {
    TooManyVms {
        count: u64,
        max: u64,
    },
    MissingVm {
        target: u64,
    },
    VmNotRunning {
        target: u32,
        status: FaultVmStatus,
    },
    VmNotCrashed {
        target: u32,
        status: FaultVmStatus,
    },
    MissingInitialSnapshot {
        target: u32,
    },
    MissingBlockDevice {
        target: u32,
    },
    InvalidVcpu {
        target: u32,
        vcpu: u64,
        count: u32,
    },
    InvalidRegisterBit {
        bit: u8,
    },
    InvalidRate {
        rate_ppm: u32,
    },
    ZeroDuration,
    ZeroClockDelta,
    ClockDeltaRoundsToZero {
        value_ns: i64,
        tsc_khz: u32,
    },
    DurationExceedsPolicy {
        value: u64,
        max: u64,
    },
    DurationRoundsToZero {
        value_ns: u64,
    },
    EmptyRange,
    RangeOverflow {
        offset: u64,
        len: u64,
    },
    RangeOutOfBounds {
        offset: u64,
        len: u64,
        size: u64,
    },
    EmptyPartitionSide,
    TooManyPartitionMembers {
        count: u64,
        max: u32,
    },
    DuplicatePartitionMember {
        target: u32,
    },
    OverlappingPartitionMember {
        target: u32,
    },
    InvalidIrq {
        irq: u32,
    },
    InvalidMemoryLimit {
        limit_bytes: u64,
        baseline_bytes: u64,
    },
    ArithmeticOverflow,
    UnsupportedCapability {
        variant: FaultVariant,
    },
}

pub fn plan_fault_application(
    attempt: &FaultAttempt,
    facts: &FaultPlanningFacts,
    policy: &FaultApplicationPolicy,
) -> Result<FaultPlan, FaultRejectionReason> {
    validate_fact_bounds(facts)?;
    let effect = match attempt.fault.category() {
        FaultCategory::Network => plan_network_fault(&attempt.fault, facts, policy)?,
        FaultCategory::Disk => plan_disk_fault(&attempt.fault, facts, policy)?,
        FaultCategory::Process => plan_process_fault(&attempt.fault, facts, policy)?,
        FaultCategory::Clock => plan_clock_fault(&attempt.fault, facts, policy)?,
        FaultCategory::Resource => plan_resource_fault(&attempt.fault, facts, policy)?,
        FaultCategory::Interrupt => plan_interrupt_fault(&attempt.fault, facts)?,
        FaultCategory::Cpu => plan_cpu_fault(&attempt.fault, facts, policy)?,
    };
    Ok(FaultPlan {
        attempt_id: attempt.id,
        effect,
    })
}

fn validate_fact_bounds(facts: &FaultPlanningFacts) -> Result<(), FaultRejectionReason> {
    if facts.vms.len() > MAX_FAULT_VMS {
        return Err(FaultRejectionReason::TooManyVms {
            count: u64::try_from(facts.vms.len()).unwrap_or(u64::MAX),
            max: u64::try_from(MAX_FAULT_VMS).unwrap_or(u64::MAX),
        });
    }
    Ok(())
}

fn plan_network_fault(
    fault: &Fault,
    facts: &FaultPlanningFacts,
    policy: &FaultApplicationPolicy,
) -> Result<FaultPlanEffect, FaultRejectionReason> {
    if !facts.network_supported {
        return Err(FaultRejectionReason::UnsupportedCapability {
            variant: fault.variant(),
        });
    }
    match fault {
        Fault::NetworkPartition { side_a, side_b } => {
            plan_network_partition(side_a, side_b, facts, policy)
        }
        Fault::NetworkLatency { target, latency_ns } => Ok(FaultPlanEffect::NetworkLatency {
            target: checked_target(*target, facts)?,
            latency_ticks: duration_ns_to_ticks(*latency_ns, policy, true)?,
        }),
        Fault::PacketLoss { target, rate_ppm } => Ok(FaultPlanEffect::PacketLoss {
            target: checked_target(*target, facts)?,
            rate_ppm: checked_rate(*rate_ppm)?,
        }),
        Fault::PacketCorruption { target, rate_ppm } => Ok(FaultPlanEffect::PacketCorruption {
            target: checked_target(*target, facts)?,
            rate_ppm: checked_rate(*rate_ppm)?,
        }),
        Fault::PacketReorder { target, window_ns } => Ok(FaultPlanEffect::PacketReorder {
            target: checked_target(*target, facts)?,
            window_ticks: duration_ns_to_ticks(*window_ns, policy, true)?,
        }),
        Fault::NetworkJitter { target, jitter_ns } => Ok(FaultPlanEffect::NetworkJitter {
            target: checked_target(*target, facts)?,
            jitter_ticks: duration_ns_to_ticks(*jitter_ns, policy, true)?,
        }),
        Fault::NetworkBandwidth {
            target,
            bytes_per_sec,
        } => Ok(FaultPlanEffect::NetworkBandwidth {
            target: checked_target(*target, facts)?,
            bytes_per_sec: *bytes_per_sec,
        }),
        Fault::PacketDuplicate { target, rate_ppm } => Ok(FaultPlanEffect::PacketDuplicate {
            target: checked_target(*target, facts)?,
            rate_ppm: checked_rate(*rate_ppm)?,
        }),
        Fault::NetworkHeal => Ok(FaultPlanEffect::NetworkHeal),
        _ => unreachable!("network planner must receive a network fault"),
    }
}

fn plan_network_partition(
    side_a: &[usize],
    side_b: &[usize],
    facts: &FaultPlanningFacts,
    policy: &FaultApplicationPolicy,
) -> Result<FaultPlanEffect, FaultRejectionReason> {
    if side_a.is_empty() || side_b.is_empty() {
        return Err(FaultRejectionReason::EmptyPartitionSide);
    }
    let member_count = side_a
        .len()
        .checked_add(side_b.len())
        .ok_or(FaultRejectionReason::ArithmeticOverflow)?;
    if member_count > policy.max_partition_members as usize {
        return Err(FaultRejectionReason::TooManyPartitionMembers {
            count: u64::try_from(member_count).unwrap_or(u64::MAX),
            max: policy.max_partition_members,
        });
    }
    let normalized_a = normalize_partition_side(side_a, facts)?;
    let normalized_b = normalize_partition_side(side_b, facts)?;
    for target in &normalized_a {
        if normalized_b.binary_search(target).is_ok() {
            return Err(FaultRejectionReason::OverlappingPartitionMember { target: *target });
        }
    }
    Ok(FaultPlanEffect::NetworkPartition {
        side_a: normalized_a,
        side_b: normalized_b,
    })
}

fn normalize_partition_side(
    side: &[usize],
    facts: &FaultPlanningFacts,
) -> Result<Vec<u32>, FaultRejectionReason> {
    let mut normalized = Vec::with_capacity(side.len());
    for target in side {
        normalized.push(checked_target(*target, facts)?);
    }
    normalized.sort_unstable();
    for pair in normalized.windows(2) {
        if pair[0] == pair[1] {
            return Err(FaultRejectionReason::DuplicatePartitionMember { target: pair[0] });
        }
    }
    Ok(normalized)
}

fn plan_disk_fault(
    fault: &Fault,
    facts: &FaultPlanningFacts,
    policy: &FaultApplicationPolicy,
) -> Result<FaultPlanEffect, FaultRejectionReason> {
    match fault {
        Fault::DiskReadError { target, offset } => {
            let target = checked_block_target(*target, facts)?;
            checked_block_range(target, *offset, 1, facts)?;
            Ok(FaultPlanEffect::BlockReadError {
                target,
                offset: *offset,
            })
        }
        Fault::DiskWriteError { target, offset } => {
            let target = checked_block_target(*target, facts)?;
            checked_block_range(target, *offset, 1, facts)?;
            Ok(FaultPlanEffect::BlockWriteError {
                target,
                offset: *offset,
            })
        }
        Fault::DiskTornWrite {
            target,
            offset,
            bytes_written,
        } => {
            let target = checked_block_target(*target, facts)?;
            let bytes_written = checked_nonzero_usize(*bytes_written)?;
            checked_block_range(target, *offset, bytes_written, facts)?;
            Ok(FaultPlanEffect::BlockTornWrite {
                target,
                offset: *offset,
                bytes_written,
            })
        }
        Fault::DiskCorruption {
            target,
            offset,
            len,
        } => {
            let target = checked_block_target(*target, facts)?;
            let len = checked_nonzero_usize(*len)?;
            checked_block_range(target, *offset, len, facts)?;
            Ok(FaultPlanEffect::BlockCorruption {
                target,
                offset: *offset,
                len,
            })
        }
        Fault::DiskFull { target } => Ok(FaultPlanEffect::BlockFull {
            target: checked_block_target(*target, facts)?,
        }),
        Fault::DiskSlow { target, delay_ns } => {
            checked_duration_ns(*delay_ns, policy, true)?;
            Ok(FaultPlanEffect::BlockSlow {
                target: checked_block_target(*target, facts)?,
                delay_ns: *delay_ns,
            })
        }
        Fault::DiskFsyncLie { target } => Ok(FaultPlanEffect::BlockFsyncLie {
            target: checked_block_target(*target, facts)?,
        }),
        Fault::DiskFsyncFlush { target } => Ok(FaultPlanEffect::BlockFsyncFlush {
            target: checked_block_target(*target, facts)?,
        }),
        Fault::DiskPartialRead {
            target,
            offset,
            max_bytes,
        } => {
            let target = checked_block_target(*target, facts)?;
            let max_bytes = checked_nonzero_usize(*max_bytes)?;
            checked_block_range(target, *offset, max_bytes, facts)?;
            Ok(FaultPlanEffect::BlockPartialRead {
                target,
                offset: *offset,
                max_bytes,
            })
        }
        _ => unreachable!("disk planner must receive a disk fault"),
    }
}

fn plan_process_fault(
    fault: &Fault,
    facts: &FaultPlanningFacts,
    policy: &FaultApplicationPolicy,
) -> Result<FaultPlanEffect, FaultRejectionReason> {
    match fault {
        Fault::ProcessKill { target } => {
            let target = checked_running_target(*target, facts)?;
            Ok(FaultPlanEffect::ProcessKill { target })
        }
        Fault::ProcessPause {
            target,
            duration_ns,
        } => {
            let target = checked_running_target(*target, facts)?;
            let duration_ticks = duration_ns_to_ticks(*duration_ns, policy, false)?;
            let resume_at_tick = facts
                .current_tick
                .checked_add(duration_ticks)
                .ok_or(FaultRejectionReason::ArithmeticOverflow)?;
            Ok(FaultPlanEffect::ProcessPause {
                target,
                resume_at_tick,
            })
        }
        Fault::ProcessRestart { target } => {
            let target = checked_target(*target, facts)?;
            let vm = &facts.vms[target as usize];
            if vm.status != FaultVmStatus::Crashed {
                return Err(FaultRejectionReason::VmNotCrashed {
                    target,
                    status: vm.status,
                });
            }
            if !vm.has_initial_snapshot {
                return Err(FaultRejectionReason::MissingInitialSnapshot { target });
            }
            let restart_at_tick = facts
                .current_tick
                .checked_add(PROCESS_RESTART_DELAY_TICKS)
                .ok_or(FaultRejectionReason::ArithmeticOverflow)?;
            Ok(FaultPlanEffect::ProcessRestart {
                target,
                restart_at_tick,
            })
        }
        _ => unreachable!("process planner must receive a process fault"),
    }
}

fn plan_clock_fault(
    fault: &Fault,
    facts: &FaultPlanningFacts,
    policy: &FaultApplicationPolicy,
) -> Result<FaultPlanEffect, FaultRejectionReason> {
    match fault {
        Fault::ClockSkew { target, offset_ns } => {
            let target = checked_running_target(*target, facts)?;
            if *offset_ns == 0 {
                return Err(FaultRejectionReason::ZeroClockDelta);
            }
            let vm = &facts.vms[target as usize];
            let tsc_delta = checked_ns_to_tsc_delta(*offset_ns, vm.tsc_khz)?;
            if tsc_delta == 0 {
                return Err(FaultRejectionReason::ClockDeltaRoundsToZero {
                    value_ns: *offset_ns,
                    tsc_khz: vm.tsc_khz,
                });
            }
            let target_tsc = checked_signed_add(vm.virtual_tsc, tsc_delta)?;
            Ok(FaultPlanEffect::VirtualClockSkew {
                target,
                basis_tsc: vm.virtual_tsc,
                tsc_khz: vm.tsc_khz,
                offset_ns: *offset_ns,
                tsc_delta,
                target_tsc,
            })
        }
        Fault::ClockJump { target, delta_ns } => {
            let target = checked_running_target(*target, facts)?;
            if *delta_ns == 0 {
                return Err(FaultRejectionReason::ZeroClockDelta);
            }
            let vm = &facts.vms[target as usize];
            let tsc_delta = checked_ns_to_tsc_delta(*delta_ns, vm.tsc_khz)?;
            if tsc_delta == 0 {
                return Err(FaultRejectionReason::ClockDeltaRoundsToZero {
                    value_ns: *delta_ns,
                    tsc_khz: vm.tsc_khz,
                });
            }
            let target_tsc = checked_signed_add(vm.virtual_tsc, tsc_delta)?;
            Ok(FaultPlanEffect::VirtualClockJump {
                target,
                basis_tsc: vm.virtual_tsc,
                tsc_khz: vm.tsc_khz,
                delta_ns: *delta_ns,
                tsc_delta,
                target_tsc,
            })
        }
        Fault::ClockFreeze {
            target,
            duration_ticks,
        } => {
            let target = checked_running_target(*target, facts)?;
            let vm = &facts.vms[target as usize];
            if !vm.supports_clock_freeze {
                return Err(FaultRejectionReason::UnsupportedCapability {
                    variant: fault.variant(),
                });
            }
            checked_duration_ticks(*duration_ticks, policy, false)?;
            let release_at_tick = facts
                .current_tick
                .checked_add(*duration_ticks)
                .ok_or(FaultRejectionReason::ArithmeticOverflow)?;
            Ok(FaultPlanEffect::VirtualClockFreeze {
                target,
                frozen_tsc: vm.virtual_tsc,
                release_at_tick,
            })
        }
        Fault::ClockJitter { target, bound_tsc } => {
            let target = checked_running_target(*target, facts)?;
            if !facts.vms[target as usize].supports_clock_jitter {
                return Err(FaultRejectionReason::UnsupportedCapability {
                    variant: fault.variant(),
                });
            }
            Ok(FaultPlanEffect::VirtualClockJitter {
                target,
                bound_tsc: *bound_tsc,
            })
        }
        _ => unreachable!("clock planner must receive a clock fault"),
    }
}

fn plan_resource_fault(
    fault: &Fault,
    facts: &FaultPlanningFacts,
    policy: &FaultApplicationPolicy,
) -> Result<FaultPlanEffect, FaultRejectionReason> {
    match fault {
        Fault::MemoryPressure {
            target,
            limit_bytes,
            duration_ticks,
        } => {
            let target = checked_running_target(*target, facts)?;
            let vm = &facts.vms[target as usize];
            if !vm.supports_memory_pressure {
                return Err(FaultRejectionReason::UnsupportedCapability {
                    variant: fault.variant(),
                });
            }
            if *limit_bytes == 0 || *limit_bytes >= vm.memory_size_bytes {
                return Err(FaultRejectionReason::InvalidMemoryLimit {
                    limit_bytes: *limit_bytes,
                    baseline_bytes: vm.memory_size_bytes,
                });
            }
            checked_duration_ticks(*duration_ticks, policy, false)?;
            let release_at_tick = facts
                .current_tick
                .checked_add(*duration_ticks)
                .ok_or(FaultRejectionReason::ArithmeticOverflow)?;
            Ok(FaultPlanEffect::MemoryPressure {
                target,
                limit_bytes: *limit_bytes,
                baseline_bytes: vm.memory_size_bytes,
                release_at_tick,
            })
        }
        _ => unreachable!("resource planner must receive a resource fault"),
    }
}

fn plan_interrupt_fault(
    fault: &Fault,
    facts: &FaultPlanningFacts,
) -> Result<FaultPlanEffect, FaultRejectionReason> {
    match fault {
        Fault::InjectInterrupt { target, irq } => {
            let target = checked_running_target(*target, facts)?;
            if *irq > MAX_STANDARD_IRQ_LINE {
                return Err(FaultRejectionReason::InvalidIrq { irq: *irq });
            }
            if !facts.vms[target as usize].supports_irq {
                return Err(FaultRejectionReason::UnsupportedCapability {
                    variant: fault.variant(),
                });
            }
            Ok(FaultPlanEffect::IrqInjection { target, irq: *irq })
        }
        Fault::InjectNmi { target, vcpu } => {
            let target = checked_running_target(*target, facts)?;
            let vcpu = checked_vcpu(target, *vcpu, facts)?;
            if !facts.vms[target as usize].supports_nmi {
                return Err(FaultRejectionReason::UnsupportedCapability {
                    variant: fault.variant(),
                });
            }
            Ok(FaultPlanEffect::NmiInjection { target, vcpu })
        }
        _ => unreachable!("interrupt planner must receive an interrupt fault"),
    }
}

fn plan_cpu_fault(
    fault: &Fault,
    facts: &FaultPlanningFacts,
    policy: &FaultApplicationPolicy,
) -> Result<FaultPlanEffect, FaultRejectionReason> {
    match fault {
        Fault::CpuBitflip {
            target,
            vcpu,
            register,
            bit,
        } => {
            let target = checked_running_target(*target, facts)?;
            let vcpu = checked_vcpu(target, *vcpu, facts)?;
            if *bit >= GENERAL_REGISTER_BIT_COUNT {
                return Err(FaultRejectionReason::InvalidRegisterBit { bit: *bit });
            }
            Ok(FaultPlanEffect::CpuRegisterBitflip {
                target,
                vcpu,
                register: *register,
                bit: *bit,
            })
        }
        Fault::CpuStall {
            target,
            vcpu,
            duration_ticks,
        } => {
            let target = checked_running_target(*target, facts)?;
            let vcpu = checked_vcpu(target, *vcpu, facts)?;
            if !facts.vms[target as usize].supports_cpu_stall {
                return Err(FaultRejectionReason::UnsupportedCapability {
                    variant: fault.variant(),
                });
            }
            checked_duration_ticks(*duration_ticks, policy, false)?;
            let release_at_tick = facts
                .current_tick
                .checked_add(*duration_ticks)
                .ok_or(FaultRejectionReason::ArithmeticOverflow)?;
            Ok(FaultPlanEffect::CpuStall {
                target,
                vcpu,
                release_at_tick,
            })
        }
        _ => unreachable!("CPU planner must receive a CPU fault"),
    }
}

fn checked_target(target: usize, facts: &FaultPlanningFacts) -> Result<u32, FaultRejectionReason> {
    if target >= facts.vms.len() {
        return Err(FaultRejectionReason::MissingVm {
            target: u64::try_from(target).unwrap_or(u64::MAX),
        });
    }
    u32::try_from(target).map_err(|_| FaultRejectionReason::MissingVm {
        target: u64::try_from(target).unwrap_or(u64::MAX),
    })
}

fn checked_running_target(
    target: usize,
    facts: &FaultPlanningFacts,
) -> Result<u32, FaultRejectionReason> {
    let target = checked_target(target, facts)?;
    let status = facts.vms[target as usize].status;
    if status != FaultVmStatus::Running {
        return Err(FaultRejectionReason::VmNotRunning { target, status });
    }
    Ok(target)
}

fn checked_block_target(
    target: usize,
    facts: &FaultPlanningFacts,
) -> Result<u32, FaultRejectionReason> {
    let target = checked_running_target(target, facts)?;
    if facts.vms[target as usize].block_device_size_bytes.is_none() {
        return Err(FaultRejectionReason::MissingBlockDevice { target });
    }
    Ok(target)
}

fn checked_vcpu(
    target: u32,
    vcpu: usize,
    facts: &FaultPlanningFacts,
) -> Result<u32, FaultRejectionReason> {
    let count = facts.vms[target as usize].vcpu_count;
    let vcpu_u64 = u64::try_from(vcpu).unwrap_or(u64::MAX);
    if vcpu_u64 >= u64::from(count) {
        return Err(FaultRejectionReason::InvalidVcpu {
            target,
            vcpu: vcpu_u64,
            count,
        });
    }
    u32::try_from(vcpu).map_err(|_| FaultRejectionReason::InvalidVcpu {
        target,
        vcpu: vcpu_u64,
        count,
    })
}

fn checked_rate(rate_ppm: u32) -> Result<u32, FaultRejectionReason> {
    if rate_ppm > PARTS_PER_MILLION_MAX {
        return Err(FaultRejectionReason::InvalidRate { rate_ppm });
    }
    Ok(rate_ppm)
}

fn checked_duration_ticks(
    value: u64,
    policy: &FaultApplicationPolicy,
    zero_is_disable: bool,
) -> Result<(), FaultRejectionReason> {
    if value == 0 {
        if zero_is_disable {
            return Ok(());
        }
        return Err(FaultRejectionReason::ZeroDuration);
    }
    if value > policy.max_duration_ticks {
        return Err(FaultRejectionReason::DurationExceedsPolicy {
            value,
            max: policy.max_duration_ticks,
        });
    }
    Ok(())
}

fn checked_duration_ns(
    value_ns: u64,
    policy: &FaultApplicationPolicy,
    zero_is_disable: bool,
) -> Result<(), FaultRejectionReason> {
    if value_ns == 0 {
        if zero_is_disable {
            return Ok(());
        }
        return Err(FaultRejectionReason::ZeroDuration);
    }
    if value_ns > policy.max_duration_ns {
        return Err(FaultRejectionReason::DurationExceedsPolicy {
            value: value_ns,
            max: policy.max_duration_ns,
        });
    }
    Ok(())
}

fn duration_ns_to_ticks(
    value_ns: u64,
    policy: &FaultApplicationPolicy,
    zero_is_disable: bool,
) -> Result<u64, FaultRejectionReason> {
    checked_duration_ns(value_ns, policy, zero_is_disable)?;
    if value_ns == 0 {
        return Ok(0);
    }
    let adjusted = value_ns
        .checked_add(NANOSECONDS_PER_SIMULATION_TICK - 1)
        .ok_or(FaultRejectionReason::ArithmeticOverflow)?;
    let ticks = adjusted / NANOSECONDS_PER_SIMULATION_TICK;
    if ticks == 0 {
        return Err(FaultRejectionReason::DurationRoundsToZero { value_ns });
    }
    if ticks > policy.max_duration_ticks {
        return Err(FaultRejectionReason::DurationExceedsPolicy {
            value: ticks,
            max: policy.max_duration_ticks,
        });
    }
    Ok(ticks)
}

fn checked_nonzero_usize(value: usize) -> Result<u64, FaultRejectionReason> {
    if value == 0 {
        return Err(FaultRejectionReason::EmptyRange);
    }
    u64::try_from(value).map_err(|_| FaultRejectionReason::ArithmeticOverflow)
}

fn checked_block_range(
    target: u32,
    offset: u64,
    len: u64,
    facts: &FaultPlanningFacts,
) -> Result<(), FaultRejectionReason> {
    if len == 0 {
        return Err(FaultRejectionReason::EmptyRange);
    }
    let size = facts.vms[target as usize]
        .block_device_size_bytes
        .ok_or(FaultRejectionReason::MissingBlockDevice { target })?;
    let end = offset
        .checked_add(len)
        .ok_or(FaultRejectionReason::RangeOverflow { offset, len })?;
    if end > size {
        return Err(FaultRejectionReason::RangeOutOfBounds { offset, len, size });
    }
    Ok(())
}

pub fn checked_ns_to_tsc_delta(delta_ns: i64, tsc_khz: u32) -> Result<i64, FaultRejectionReason> {
    const NANOSECONDS_PER_MILLISECOND: i128 = 1_000_000;
    if tsc_khz == 0 {
        return Err(FaultRejectionReason::ArithmeticOverflow);
    }
    let scaled = i128::from(delta_ns)
        .checked_mul(i128::from(tsc_khz))
        .ok_or(FaultRejectionReason::ArithmeticOverflow)?;
    i64::try_from(scaled / NANOSECONDS_PER_MILLISECOND)
        .map_err(|_| FaultRejectionReason::ArithmeticOverflow)
}

fn checked_signed_add(value: u64, delta: i64) -> Result<u64, FaultRejectionReason> {
    if delta >= 0 {
        return value
            .checked_add(delta.unsigned_abs())
            .ok_or(FaultRejectionReason::ArithmeticOverflow);
    }
    value
        .checked_sub(delta.unsigned_abs())
        .ok_or(FaultRejectionReason::ArithmeticOverflow)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultObservationSubsystem {
    Network = 1,
    Block = 2,
    Scheduler = 3,
    VirtualClock = 4,
    Cpu = 5,
    Process = 6,
    Interrupt = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultObservationEffect {
    PacketDroppedByPartition,
    PacketDroppedByLoss,
    PacketCorrupted,
    PacketDelayedByLatency,
    PacketDelayedByJitter,
    PacketDelayedByBandwidth,
    PacketReordered,
    PacketDuplicated,
    BlockReadFailed,
    BlockWriteFailed,
    BlockWriteTorn,
    BlockBytesCorrupted,
    BlockReadShortened,
    BlockWriteRejectedFull,
    BlockOperationDelayed,
    BlockWriteMadeVolatile,
    ProcessSkipped,
    ProcessRestarted,
    VirtualClockChanged,
    CpuRegisterChanged,
    VirtualClockFrozen,
    VirtualClockJitterConfigured,
    CpuStallActivated,
    MemoryCeilingChanged,
    InterruptInjected,
    NmiInjected,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultObservation {
    pub attempt_id: FaultAttemptId,
    pub operation_id: FaultOperationId,
    pub subsystem: FaultObservationSubsystem,
    pub operation_sequence: u64,
    pub effect: FaultObservationEffect,
}

impl FaultObservation {
    pub fn new(
        attempt_id: FaultAttemptId,
        subsystem: FaultObservationSubsystem,
        operation_sequence: u64,
        effect: FaultObservationEffect,
    ) -> Self {
        Self {
            attempt_id,
            operation_id: fault_operation_id(attempt_id, subsystem, operation_sequence),
            subsystem,
            operation_sequence,
            effect,
        }
    }

    pub fn has_valid_identity(&self) -> bool {
        self.operation_id
            == fault_operation_id(self.attempt_id, self.subsystem, self.operation_sequence)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultApplicationFailureDisposition {
    RolledBack,
    NonRunnable,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultApplicationFailureReason {
    BackendRejected,
    DeviceDisappeared,
    TargetStateChanged,
    InternalInvariant,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultStageKind {
    Selected,
    Applicable {
        effect: FaultPlanEffect,
    },
    Rejected {
        reason: FaultRejectionReason,
    },
    Applied {
        effect: FaultPlanEffect,
    },
    ApplicationFailed {
        reason: FaultApplicationFailureReason,
        disposition: FaultApplicationFailureDisposition,
    },
    Observed {
        observation: FaultObservation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultStageEvent {
    pub sequence: u64,
    pub attempt_id: FaultAttemptId,
    pub kind: FaultStageKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultStageCounters {
    pub selected: u64,
    pub rejected: u64,
    pub applied: u64,
    pub application_failed: u64,
    pub observed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FaultAuthoritativeStage {
    Selected,
    Applicable,
    Rejected,
    Applied,
    ApplicationFailed,
    Observed,
}

fn no_applicable_effect() -> Option<FaultPlanEffect> {
    None
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultAttemptState {
    pub attempt: FaultAttempt,
    pub stage: FaultAuthoritativeStage,
    #[serde(default = "no_applicable_effect")]
    pub applicable_effect: Option<FaultPlanEffect>,
    pub observed_operations: BTreeSet<FaultOperationId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultOutcomeLedger {
    pub attempts: BTreeMap<FaultAttemptId, FaultAttemptState>,
    pub events: Vec<FaultStageEvent>,
    pub counters: FaultStageCounters,
}

impl FaultOutcomeLedger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultTransitionError {
    AttemptIdentityMismatch,
    DuplicateAttempt,
    UnknownAttempt,
    AttemptIdMismatch,
    InvalidTransition {
        from: FaultAuthoritativeStage,
        event: &'static str,
    },
    ApplicablePlanMismatch,
    ApplicableEffectMissing,
    AppliedPlanMismatch,
    ObservationIdentityMismatch,
    ObservationSubsystemMismatch,
    ObservationEffectMismatch,
    EventSequenceMismatch,
    LedgerReplayMismatch,
    MissingSelectedAttempt,
    SnapshotRunIdentityMismatch,
    SnapshotRunStateMismatch,
    SnapshotScheduleIdentityMismatch,
    SnapshotScheduleCursorMismatch,
    SnapshotAttemptSourceMismatch,
    SnapshotRandomStateMismatch,
    SnapshotRngStateMismatch,
    SnapshotSelectionSequenceMismatch,
    SnapshotPendingStateMismatch,
    SnapshotAssertionIdentityMismatch,
    DuplicateObservation,
    AttemptBoundExceeded,
    EventBoundExceeded,
    ObservationBoundExceeded,
    CounterOverflow,
    EventSequenceOverflow,
}

impl fmt::Display for FaultTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for FaultTransitionError {}

/// Apply one stage event to an immutable ledger and return the next ledger.
///
/// r[impl chaoscontrol.fault_outcomes.stage_model]
/// r[impl chaoscontrol.fault_outcomes.boundary]
pub fn transition_fault_outcome(
    ledger: &FaultOutcomeLedger,
    attempt: Option<&FaultAttempt>,
    attempt_id: FaultAttemptId,
    kind: FaultStageKind,
) -> Result<FaultOutcomeLedger, FaultTransitionError> {
    let mut next = ledger.clone();
    apply_fault_outcome_transition(&mut next, attempt, attempt_id, kind)?;
    Ok(next)
}

fn apply_fault_outcome_transition(
    ledger: &mut FaultOutcomeLedger,
    attempt: Option<&FaultAttempt>,
    attempt_id: FaultAttemptId,
    kind: FaultStageKind,
) -> Result<(), FaultTransitionError> {
    validate_ledger_bounds(ledger)?;
    match &kind {
        FaultStageKind::Selected => transition_selected(ledger, attempt, attempt_id)?,
        FaultStageKind::Applicable { effect } => {
            transition_applicable(ledger, attempt_id, effect)?;
        }
        FaultStageKind::Rejected { .. } => {
            transition_terminal(
                ledger,
                attempt_id,
                FaultAuthoritativeStage::Selected,
                FaultAuthoritativeStage::Rejected,
                "rejected",
            )?;
            ledger.counters.rejected = checked_increment(ledger.counters.rejected)?;
        }
        FaultStageKind::Applied { effect } => {
            transition_applied(ledger, attempt_id, effect)?;
            ledger.counters.applied = checked_increment(ledger.counters.applied)?;
        }
        FaultStageKind::ApplicationFailed { .. } => {
            transition_terminal(
                ledger,
                attempt_id,
                FaultAuthoritativeStage::Applicable,
                FaultAuthoritativeStage::ApplicationFailed,
                "application-failed",
            )?;
            ledger.counters.application_failed =
                checked_increment(ledger.counters.application_failed)?;
        }
        FaultStageKind::Observed { observation } => {
            transition_observed(ledger, attempt_id, observation)?;
            ledger.counters.observed = checked_increment(ledger.counters.observed)?;
        }
    }
    push_stage_event(ledger, attempt_id, kind)?;
    validate_counter_invariants(ledger)?;
    Ok(())
}

/// Validate an untrusted ledger by replaying every event from an empty state.
pub fn validate_fault_outcome_ledger(
    ledger: &FaultOutcomeLedger,
) -> Result<(), FaultTransitionError> {
    validate_ledger_bounds(ledger)?;
    let mut replay = FaultOutcomeLedger::default();
    for (index, event) in ledger.events.iter().enumerate() {
        let expected_sequence =
            u64::try_from(index).map_err(|_| FaultTransitionError::EventSequenceOverflow)?;
        if event.sequence != expected_sequence {
            return Err(FaultTransitionError::EventSequenceMismatch);
        }
        let attempt = if event.kind == FaultStageKind::Selected {
            Some(
                &ledger
                    .attempts
                    .get(&event.attempt_id)
                    .ok_or(FaultTransitionError::MissingSelectedAttempt)?
                    .attempt,
            )
        } else {
            None
        };
        apply_fault_outcome_transition(&mut replay, attempt, event.attempt_id, event.kind.clone())?;
    }
    if replay != *ledger {
        return Err(FaultTransitionError::LedgerReplayMismatch);
    }
    Ok(())
}

/// Ensure application can record applicability, one terminal result, and immediate observations.
pub fn preflight_fault_application_events(
    ledger: &FaultOutcomeLedger,
    max_immediate_observations: usize,
) -> Result<(), FaultTransitionError> {
    preflight_fault_application_events_with_limit(
        ledger,
        max_immediate_observations,
        MAX_FAULT_OUTCOME_EVENTS,
    )
}

#[doc(hidden)]
pub fn preflight_fault_application_events_with_limit(
    ledger: &FaultOutcomeLedger,
    max_immediate_observations: usize,
    event_limit: usize,
) -> Result<(), FaultTransitionError> {
    const APPLICATION_STAGE_EVENT_COUNT: usize = 2;
    validate_ledger_bounds(ledger)?;
    if event_limit > MAX_FAULT_OUTCOME_EVENTS {
        return Err(FaultTransitionError::EventBoundExceeded);
    }
    let required = APPLICATION_STAGE_EVENT_COUNT
        .checked_add(max_immediate_observations)
        .ok_or(FaultTransitionError::EventBoundExceeded)?;
    let final_count = ledger
        .events
        .len()
        .checked_add(required)
        .ok_or(FaultTransitionError::EventBoundExceeded)?;
    if final_count > event_limit {
        return Err(FaultTransitionError::EventBoundExceeded);
    }
    Ok(())
}

/// Validate that a pending effect exactly matches an applied attempt.
pub fn validate_pending_fault_effect(
    ledger: &FaultOutcomeLedger,
    attempt_id: FaultAttemptId,
    pending_effect: &FaultPlanEffect,
) -> Result<(), FaultTransitionError> {
    validate_ledger_bounds(ledger)?;
    let state = ledger
        .attempts
        .get(&attempt_id)
        .ok_or(FaultTransitionError::UnknownAttempt)?;
    if state.stage != FaultAuthoritativeStage::Applied
        && state.stage != FaultAuthoritativeStage::Observed
    {
        return Err(FaultTransitionError::InvalidTransition {
            from: state.stage,
            event: "pending effect",
        });
    }
    let effect = state
        .applicable_effect
        .as_ref()
        .ok_or(FaultTransitionError::ApplicableEffectMissing)?;
    if effect != pending_effect {
        return Err(FaultTransitionError::AppliedPlanMismatch);
    }
    Ok(())
}

/// Validate a pending observation batch without changing the supplied ledger.
pub fn validate_pending_fault_observations(
    ledger: &FaultOutcomeLedger,
    observations: &[FaultObservation],
) -> Result<(), FaultTransitionError> {
    preflight_fault_observation_events_with_limit(
        ledger,
        observations.len(),
        MAX_FAULT_OUTCOME_EVENTS,
    )?;
    let mut next = ledger.clone();
    for observation in observations {
        next = transition_fault_outcome(
            &next,
            None,
            observation.attempt_id,
            FaultStageKind::Observed {
                observation: observation.clone(),
            },
        )?;
    }
    Ok(())
}

#[doc(hidden)]
pub fn preflight_fault_observation_events_with_limit(
    ledger: &FaultOutcomeLedger,
    observation_count: usize,
    event_limit: usize,
) -> Result<(), FaultTransitionError> {
    validate_ledger_bounds(ledger)?;
    if event_limit > MAX_FAULT_OUTCOME_EVENTS {
        return Err(FaultTransitionError::EventBoundExceeded);
    }
    let final_count = ledger
        .events
        .len()
        .checked_add(observation_count)
        .ok_or(FaultTransitionError::EventBoundExceeded)?;
    if final_count > event_limit {
        return Err(FaultTransitionError::EventBoundExceeded);
    }
    Ok(())
}

fn transition_selected(
    ledger: &mut FaultOutcomeLedger,
    attempt: Option<&FaultAttempt>,
    attempt_id: FaultAttemptId,
) -> Result<(), FaultTransitionError> {
    let attempt = attempt.ok_or(FaultTransitionError::UnknownAttempt)?;
    if attempt.id != attempt_id {
        return Err(FaultTransitionError::AttemptIdMismatch);
    }
    if !attempt.has_valid_identity() {
        return Err(FaultTransitionError::AttemptIdentityMismatch);
    }
    if ledger.attempts.contains_key(&attempt_id) {
        return Err(FaultTransitionError::DuplicateAttempt);
    }
    if ledger.attempts.len() >= MAX_FAULT_ATTEMPTS {
        return Err(FaultTransitionError::AttemptBoundExceeded);
    }
    ledger.attempts.insert(
        attempt_id,
        FaultAttemptState {
            attempt: attempt.clone(),
            stage: FaultAuthoritativeStage::Selected,
            applicable_effect: None,
            observed_operations: BTreeSet::new(),
        },
    );
    ledger.counters.selected = checked_increment(ledger.counters.selected)?;
    Ok(())
}

fn transition_applicable(
    ledger: &mut FaultOutcomeLedger,
    attempt_id: FaultAttemptId,
    effect: &FaultPlanEffect,
) -> Result<(), FaultTransitionError> {
    let state = ledger
        .attempts
        .get_mut(&attempt_id)
        .ok_or(FaultTransitionError::UnknownAttempt)?;
    if state.stage != FaultAuthoritativeStage::Selected {
        return Err(FaultTransitionError::InvalidTransition {
            from: state.stage,
            event: "applicable",
        });
    }
    if !plan_effect_matches_attempt(&state.attempt, effect) {
        return Err(FaultTransitionError::ApplicablePlanMismatch);
    }
    state.applicable_effect = Some(effect.clone());
    state.stage = FaultAuthoritativeStage::Applicable;
    Ok(())
}

fn plan_effect_matches_attempt(attempt: &FaultAttempt, effect: &FaultPlanEffect) -> bool {
    let target = |value: usize| u32::try_from(value).ok();
    match (&attempt.fault, effect) {
        (
            Fault::NetworkPartition { side_a, side_b },
            FaultPlanEffect::NetworkPartition {
                side_a: planned_a,
                side_b: planned_b,
            },
        ) => {
            partition_side_matches(side_a, planned_a)
                && partition_side_matches(side_b, planned_b)
                && planned_a.iter().all(|member| !planned_b.contains(member))
        }
        (
            Fault::NetworkLatency {
                target: fault_target,
                latency_ns,
            },
            FaultPlanEffect::NetworkLatency {
                target: planned_target,
                latency_ticks,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && duration_ticks(*latency_ns) == Some(*latency_ticks)
        }
        (
            Fault::PacketLoss {
                target: fault_target,
                rate_ppm: fault_rate,
            },
            FaultPlanEffect::PacketLoss {
                target: planned_target,
                rate_ppm: planned_rate,
            },
        )
        | (
            Fault::PacketCorruption {
                target: fault_target,
                rate_ppm: fault_rate,
            },
            FaultPlanEffect::PacketCorruption {
                target: planned_target,
                rate_ppm: planned_rate,
            },
        )
        | (
            Fault::PacketDuplicate {
                target: fault_target,
                rate_ppm: fault_rate,
            },
            FaultPlanEffect::PacketDuplicate {
                target: planned_target,
                rate_ppm: planned_rate,
            },
        ) => target(*fault_target) == Some(*planned_target) && fault_rate == planned_rate,
        (
            Fault::PacketReorder {
                target: fault_target,
                window_ns,
            },
            FaultPlanEffect::PacketReorder {
                target: planned_target,
                window_ticks,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && duration_ticks(*window_ns) == Some(*window_ticks)
        }
        (
            Fault::NetworkJitter {
                target: fault_target,
                jitter_ns,
            },
            FaultPlanEffect::NetworkJitter {
                target: planned_target,
                jitter_ticks,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && duration_ticks(*jitter_ns) == Some(*jitter_ticks)
        }
        (
            Fault::NetworkBandwidth {
                target: fault_target,
                bytes_per_sec: fault_rate,
            },
            FaultPlanEffect::NetworkBandwidth {
                target: planned_target,
                bytes_per_sec: planned_rate,
            },
        ) => target(*fault_target) == Some(*planned_target) && fault_rate == planned_rate,
        (Fault::NetworkHeal, FaultPlanEffect::NetworkHeal) => true,
        (
            Fault::DiskReadError {
                target: fault_target,
                offset: fault_offset,
            },
            FaultPlanEffect::BlockReadError {
                target: planned_target,
                offset: planned_offset,
            },
        )
        | (
            Fault::DiskWriteError {
                target: fault_target,
                offset: fault_offset,
            },
            FaultPlanEffect::BlockWriteError {
                target: planned_target,
                offset: planned_offset,
            },
        ) => target(*fault_target) == Some(*planned_target) && fault_offset == planned_offset,
        (
            Fault::DiskTornWrite {
                target: fault_target,
                offset: fault_offset,
                bytes_written,
            },
            FaultPlanEffect::BlockTornWrite {
                target: planned_target,
                offset: planned_offset,
                bytes_written: planned_bytes,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && fault_offset == planned_offset
                && u64::try_from(*bytes_written).ok() == Some(*planned_bytes)
        }
        (
            Fault::DiskCorruption {
                target: fault_target,
                offset: fault_offset,
                len,
            },
            FaultPlanEffect::BlockCorruption {
                target: planned_target,
                offset: planned_offset,
                len: planned_len,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && fault_offset == planned_offset
                && u64::try_from(*len).ok() == Some(*planned_len)
        }
        (
            Fault::DiskFull {
                target: fault_target,
            },
            FaultPlanEffect::BlockFull {
                target: planned_target,
            },
        )
        | (
            Fault::ProcessKill {
                target: fault_target,
            },
            FaultPlanEffect::ProcessKill {
                target: planned_target,
            },
        )
        | (
            Fault::DiskFsyncLie {
                target: fault_target,
            },
            FaultPlanEffect::BlockFsyncLie {
                target: planned_target,
            },
        )
        | (
            Fault::DiskFsyncFlush {
                target: fault_target,
            },
            FaultPlanEffect::BlockFsyncFlush {
                target: planned_target,
            },
        ) => target(*fault_target) == Some(*planned_target),
        (
            Fault::DiskSlow {
                target: fault_target,
                delay_ns: fault_delay,
            },
            FaultPlanEffect::BlockSlow {
                target: planned_target,
                delay_ns: planned_delay,
            },
        ) => target(*fault_target) == Some(*planned_target) && fault_delay == planned_delay,
        (
            Fault::DiskPartialRead {
                target: fault_target,
                offset: fault_offset,
                max_bytes,
            },
            FaultPlanEffect::BlockPartialRead {
                target: planned_target,
                offset: planned_offset,
                max_bytes: planned_max,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && fault_offset == planned_offset
                && u64::try_from(*max_bytes).ok() == Some(*planned_max)
        }
        (
            Fault::ProcessPause {
                target: fault_target,
                duration_ns,
            },
            FaultPlanEffect::ProcessPause {
                target: planned_target,
                resume_at_tick,
            },
        ) => {
            let selected_tick = attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK;
            target(*fault_target) == Some(*planned_target)
                && duration_ticks(*duration_ns)
                    .and_then(|duration| selected_tick.checked_add(duration))
                    == Some(*resume_at_tick)
        }
        (
            Fault::ProcessRestart {
                target: fault_target,
            },
            FaultPlanEffect::ProcessRestart {
                target: planned_target,
                restart_at_tick,
            },
        ) => {
            let selected_tick = attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK;
            target(*fault_target) == Some(*planned_target)
                && selected_tick.checked_add(PROCESS_RESTART_DELAY_TICKS) == Some(*restart_at_tick)
        }
        (
            Fault::ClockSkew {
                target: fault_target,
                offset_ns: fault_offset,
            },
            FaultPlanEffect::VirtualClockSkew {
                target: planned_target,
                basis_tsc,
                tsc_khz,
                offset_ns,
                tsc_delta,
                target_tsc,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && fault_offset == offset_ns
                && checked_ns_to_tsc_delta(*offset_ns, *tsc_khz).ok() == Some(*tsc_delta)
                && checked_signed_add(*basis_tsc, *tsc_delta).ok() == Some(*target_tsc)
        }
        (
            Fault::ClockJump {
                target: fault_target,
                delta_ns: fault_delta,
            },
            FaultPlanEffect::VirtualClockJump {
                target: planned_target,
                basis_tsc,
                tsc_khz,
                delta_ns,
                tsc_delta,
                target_tsc,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && fault_delta == delta_ns
                && checked_ns_to_tsc_delta(*delta_ns, *tsc_khz).ok() == Some(*tsc_delta)
                && checked_signed_add(*basis_tsc, *tsc_delta).ok() == Some(*target_tsc)
        }
        (
            Fault::ClockFreeze {
                target: fault_target,
                duration_ticks,
            },
            FaultPlanEffect::VirtualClockFreeze {
                target: planned_target,
                release_at_tick,
                ..
            },
        ) => {
            let selected_tick = attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK;
            target(*fault_target) == Some(*planned_target)
                && *duration_ticks > 0
                && selected_tick.checked_add(*duration_ticks) == Some(*release_at_tick)
        }
        (
            Fault::ClockJitter {
                target: fault_target,
                bound_tsc: fault_bound,
            },
            FaultPlanEffect::VirtualClockJitter {
                target: planned_target,
                bound_tsc: planned_bound,
            },
        ) => target(*fault_target) == Some(*planned_target) && fault_bound == planned_bound,
        (
            Fault::MemoryPressure {
                target: fault_target,
                limit_bytes: fault_limit,
                duration_ticks,
            },
            FaultPlanEffect::MemoryPressure {
                target: planned_target,
                limit_bytes: planned_limit,
                baseline_bytes,
                release_at_tick,
            },
        ) => {
            let selected_tick = attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK;
            target(*fault_target) == Some(*planned_target)
                && fault_limit == planned_limit
                && *planned_limit > 0
                && *planned_limit < *baseline_bytes
                && *duration_ticks > 0
                && selected_tick.checked_add(*duration_ticks) == Some(*release_at_tick)
        }
        (
            Fault::InjectInterrupt {
                target: fault_target,
                irq: fault_irq,
            },
            FaultPlanEffect::IrqInjection {
                target: planned_target,
                irq: planned_irq,
            },
        ) => target(*fault_target) == Some(*planned_target) && fault_irq == planned_irq,
        (
            Fault::InjectNmi {
                target: fault_target,
                vcpu: fault_vcpu,
            },
            FaultPlanEffect::NmiInjection {
                target: planned_target,
                vcpu: planned_vcpu,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && u32::try_from(*fault_vcpu).ok() == Some(*planned_vcpu)
        }
        (
            Fault::CpuStall {
                target: fault_target,
                vcpu: fault_vcpu,
                duration_ticks,
            },
            FaultPlanEffect::CpuStall {
                target: planned_target,
                vcpu: planned_vcpu,
                release_at_tick,
            },
        ) => {
            let selected_tick = attempt.selected_at_ns / NANOSECONDS_PER_SIMULATION_TICK;
            target(*fault_target) == Some(*planned_target)
                && u32::try_from(*fault_vcpu).ok() == Some(*planned_vcpu)
                && *duration_ticks > 0
                && selected_tick.checked_add(*duration_ticks) == Some(*release_at_tick)
        }
        (
            Fault::CpuBitflip {
                target: fault_target,
                vcpu: fault_vcpu,
                register: fault_register,
                bit: fault_bit,
            },
            FaultPlanEffect::CpuRegisterBitflip {
                target: planned_target,
                vcpu: planned_vcpu,
                register: planned_register,
                bit: planned_bit,
            },
        ) => {
            target(*fault_target) == Some(*planned_target)
                && u32::try_from(*fault_vcpu).ok() == Some(*planned_vcpu)
                && fault_register == planned_register
                && fault_bit == planned_bit
        }
        _ => false,
    }
}

fn partition_side_matches(original: &[usize], planned: &[u32]) -> bool {
    if original.is_empty() || original.len() != planned.len() {
        return false;
    }
    let mut normalized = Vec::with_capacity(original.len());
    for member in original {
        let Ok(member) = u32::try_from(*member) else {
            return false;
        };
        normalized.push(member);
    }
    normalized.sort_unstable();
    normalized.windows(2).all(|pair| pair[0] != pair[1]) && normalized == planned
}

fn duration_ticks(duration_ns: u64) -> Option<u64> {
    if duration_ns == 0 {
        return Some(0);
    }
    duration_ns
        .checked_add(NANOSECONDS_PER_SIMULATION_TICK - 1)
        .map(|adjusted| adjusted / NANOSECONDS_PER_SIMULATION_TICK)
}

fn transition_applied(
    ledger: &mut FaultOutcomeLedger,
    attempt_id: FaultAttemptId,
    effect: &FaultPlanEffect,
) -> Result<(), FaultTransitionError> {
    let state = ledger
        .attempts
        .get_mut(&attempt_id)
        .ok_or(FaultTransitionError::UnknownAttempt)?;
    if state.stage != FaultAuthoritativeStage::Applicable {
        return Err(FaultTransitionError::InvalidTransition {
            from: state.stage,
            event: "applied",
        });
    }
    let applicable_effect = state
        .applicable_effect
        .as_ref()
        .ok_or(FaultTransitionError::ApplicableEffectMissing)?;
    if applicable_effect != effect {
        return Err(FaultTransitionError::AppliedPlanMismatch);
    }
    state.stage = FaultAuthoritativeStage::Applied;
    Ok(())
}

fn transition_terminal(
    ledger: &mut FaultOutcomeLedger,
    attempt_id: FaultAttemptId,
    expected: FaultAuthoritativeStage,
    next_stage: FaultAuthoritativeStage,
    event: &'static str,
) -> Result<(), FaultTransitionError> {
    let state = ledger
        .attempts
        .get_mut(&attempt_id)
        .ok_or(FaultTransitionError::UnknownAttempt)?;
    if state.stage != expected {
        return Err(FaultTransitionError::InvalidTransition {
            from: state.stage,
            event,
        });
    }
    state.stage = next_stage;
    Ok(())
}

fn transition_observed(
    ledger: &mut FaultOutcomeLedger,
    attempt_id: FaultAttemptId,
    observation: &FaultObservation,
) -> Result<(), FaultTransitionError> {
    if observation.attempt_id != attempt_id {
        return Err(FaultTransitionError::AttemptIdMismatch);
    }
    if !observation.has_valid_identity() {
        return Err(FaultTransitionError::ObservationIdentityMismatch);
    }
    if observation.subsystem != observation_effect_subsystem(observation.effect) {
        return Err(FaultTransitionError::ObservationSubsystemMismatch);
    }
    let state = ledger
        .attempts
        .get_mut(&attempt_id)
        .ok_or(FaultTransitionError::UnknownAttempt)?;
    if state.stage != FaultAuthoritativeStage::Applied
        && state.stage != FaultAuthoritativeStage::Observed
    {
        return Err(FaultTransitionError::InvalidTransition {
            from: state.stage,
            event: "observed",
        });
    }
    let applicable_effect = state
        .applicable_effect
        .as_ref()
        .ok_or(FaultTransitionError::ApplicableEffectMissing)?;
    if !mechanism_accepts_observation(applicable_effect.mechanism(), observation.effect) {
        return Err(FaultTransitionError::ObservationEffectMismatch);
    }
    if state
        .observed_operations
        .contains(&observation.operation_id)
    {
        return Err(FaultTransitionError::DuplicateObservation);
    }
    if state.observed_operations.len() >= MAX_OBSERVATIONS_PER_ATTEMPT {
        return Err(FaultTransitionError::ObservationBoundExceeded);
    }
    state.observed_operations.insert(observation.operation_id);
    state.stage = FaultAuthoritativeStage::Observed;
    Ok(())
}

fn observation_effect_subsystem(effect: FaultObservationEffect) -> FaultObservationSubsystem {
    match effect {
        FaultObservationEffect::PacketDroppedByPartition
        | FaultObservationEffect::PacketDroppedByLoss
        | FaultObservationEffect::PacketCorrupted
        | FaultObservationEffect::PacketDelayedByLatency
        | FaultObservationEffect::PacketDelayedByJitter
        | FaultObservationEffect::PacketDelayedByBandwidth
        | FaultObservationEffect::PacketReordered
        | FaultObservationEffect::PacketDuplicated => FaultObservationSubsystem::Network,
        FaultObservationEffect::BlockReadFailed
        | FaultObservationEffect::BlockWriteFailed
        | FaultObservationEffect::BlockWriteTorn
        | FaultObservationEffect::BlockBytesCorrupted
        | FaultObservationEffect::BlockReadShortened
        | FaultObservationEffect::BlockWriteRejectedFull
        | FaultObservationEffect::BlockOperationDelayed
        | FaultObservationEffect::BlockWriteMadeVolatile => FaultObservationSubsystem::Block,
        FaultObservationEffect::ProcessSkipped | FaultObservationEffect::ProcessRestarted => {
            FaultObservationSubsystem::Process
        }
        FaultObservationEffect::VirtualClockChanged
        | FaultObservationEffect::VirtualClockFrozen
        | FaultObservationEffect::VirtualClockJitterConfigured => {
            FaultObservationSubsystem::VirtualClock
        }
        FaultObservationEffect::CpuRegisterChanged | FaultObservationEffect::CpuStallActivated => {
            FaultObservationSubsystem::Cpu
        }
        FaultObservationEffect::MemoryCeilingChanged => FaultObservationSubsystem::Scheduler,
        FaultObservationEffect::InterruptInjected | FaultObservationEffect::NmiInjected => {
            FaultObservationSubsystem::Interrupt
        }
    }
}

fn mechanism_accepts_observation(
    mechanism: FaultMechanism,
    effect: FaultObservationEffect,
) -> bool {
    matches!(
        (mechanism, effect),
        (
            FaultMechanism::NetworkPartition,
            FaultObservationEffect::PacketDroppedByPartition
        ) | (
            FaultMechanism::NetworkLatency,
            FaultObservationEffect::PacketDelayedByLatency
        ) | (
            FaultMechanism::PacketLoss,
            FaultObservationEffect::PacketDroppedByLoss
        ) | (
            FaultMechanism::PacketCorruption,
            FaultObservationEffect::PacketCorrupted
        ) | (
            FaultMechanism::PacketReorder,
            FaultObservationEffect::PacketReordered
        ) | (
            FaultMechanism::NetworkJitter,
            FaultObservationEffect::PacketDelayedByJitter
        ) | (
            FaultMechanism::NetworkBandwidth,
            FaultObservationEffect::PacketDelayedByBandwidth
        ) | (
            FaultMechanism::PacketDuplicate,
            FaultObservationEffect::PacketDuplicated
        ) | (
            FaultMechanism::BlockReadError,
            FaultObservationEffect::BlockReadFailed
        ) | (
            FaultMechanism::BlockWriteError,
            FaultObservationEffect::BlockWriteFailed
        ) | (
            FaultMechanism::BlockTornWrite,
            FaultObservationEffect::BlockWriteTorn
        ) | (
            FaultMechanism::BlockCorruption,
            FaultObservationEffect::BlockBytesCorrupted
        ) | (
            FaultMechanism::BlockFull,
            FaultObservationEffect::BlockWriteRejectedFull
        ) | (
            FaultMechanism::BlockSlow,
            FaultObservationEffect::BlockOperationDelayed
        ) | (
            FaultMechanism::BlockFsyncLie,
            FaultObservationEffect::BlockWriteMadeVolatile
        ) | (
            FaultMechanism::BlockPartialRead,
            FaultObservationEffect::BlockReadShortened
        ) | (
            FaultMechanism::ProcessKill | FaultMechanism::ProcessPause,
            FaultObservationEffect::ProcessSkipped
        ) | (
            FaultMechanism::ProcessRestart,
            FaultObservationEffect::ProcessRestarted
        ) | (
            FaultMechanism::VirtualClockSkew | FaultMechanism::VirtualClockJump,
            FaultObservationEffect::VirtualClockChanged
        ) | (
            FaultMechanism::VirtualClockFreeze,
            FaultObservationEffect::VirtualClockFrozen
        ) | (
            FaultMechanism::VirtualClockJitter,
            FaultObservationEffect::VirtualClockJitterConfigured
        ) | (
            FaultMechanism::CpuStall,
            FaultObservationEffect::CpuStallActivated
        ) | (
            FaultMechanism::MemoryPressure,
            FaultObservationEffect::MemoryCeilingChanged
        ) | (
            FaultMechanism::IrqInjection,
            FaultObservationEffect::InterruptInjected
        ) | (
            FaultMechanism::NmiInjection,
            FaultObservationEffect::NmiInjected
        ) | (
            FaultMechanism::CpuRegisterBitflip,
            FaultObservationEffect::CpuRegisterChanged
        )
    )
}

fn push_stage_event(
    ledger: &mut FaultOutcomeLedger,
    attempt_id: FaultAttemptId,
    kind: FaultStageKind,
) -> Result<(), FaultTransitionError> {
    if ledger.events.len() >= MAX_FAULT_OUTCOME_EVENTS {
        return Err(FaultTransitionError::EventBoundExceeded);
    }
    let sequence = u64::try_from(ledger.events.len())
        .map_err(|_| FaultTransitionError::EventSequenceOverflow)?;
    ledger.events.push(FaultStageEvent {
        sequence,
        attempt_id,
        kind,
    });
    Ok(())
}

fn validate_ledger_bounds(ledger: &FaultOutcomeLedger) -> Result<(), FaultTransitionError> {
    if ledger.attempts.len() > MAX_FAULT_ATTEMPTS {
        return Err(FaultTransitionError::AttemptBoundExceeded);
    }
    if ledger.events.len() > MAX_FAULT_OUTCOME_EVENTS {
        return Err(FaultTransitionError::EventBoundExceeded);
    }
    Ok(())
}

fn validate_counter_invariants(ledger: &FaultOutcomeLedger) -> Result<(), FaultTransitionError> {
    if ledger.counters.rejected > ledger.counters.selected {
        return Err(FaultTransitionError::CounterOverflow);
    }
    if ledger.counters.applied > ledger.counters.selected {
        return Err(FaultTransitionError::CounterOverflow);
    }
    if ledger.counters.application_failed > ledger.counters.selected {
        return Err(FaultTransitionError::CounterOverflow);
    }
    Ok(())
}

fn checked_increment(value: u64) -> Result<u64, FaultTransitionError> {
    value
        .checked_add(1)
        .ok_or(FaultTransitionError::CounterOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SEED: u64 = 42;
    const TEST_SCHEDULE_SEQUENCE: u64 = 1;
    const TEST_SELECTED_AT_NS: u64 = NANOSECONDS_PER_SIMULATION_TICK;
    const TEST_BLOCK_SIZE_BYTES: u64 = 4_096;
    const TEST_MEMORY_SIZE_BYTES: u64 = 8_192;
    const TEST_TSC_KHZ: u32 = 3_000_000;

    fn running_vm() -> VmFaultFacts {
        VmFaultFacts {
            status: FaultVmStatus::Running,
            vcpu_count: 2,
            memory_size_bytes: TEST_MEMORY_SIZE_BYTES,
            block_device_size_bytes: Some(TEST_BLOCK_SIZE_BYTES),
            has_initial_snapshot: true,
            supports_irq: true,
            supports_nmi: true,
            supports_clock_freeze: true,
            supports_clock_jitter: true,
            supports_cpu_stall: true,
            supports_memory_pressure: true,
            virtual_tsc: TEST_SELECTED_AT_NS,
            tsc_khz: TEST_TSC_KHZ,
        }
    }

    fn facts() -> FaultPlanningFacts {
        FaultPlanningFacts {
            current_tick: 1,
            network_supported: true,
            vms: vec![running_vm(), running_vm(), running_vm()],
        }
    }

    fn attempt(fault: Fault) -> FaultAttempt {
        let schedule_id = fault_schedule_id([(TEST_SELECTED_AT_NS, None, &fault)]);
        let run_id = fault_run_id(TEST_SEED, TEST_SCHEDULE_SEQUENCE, schedule_id);
        FaultAttempt::new(
            run_id,
            TEST_SCHEDULE_SEQUENCE,
            schedule_id,
            0,
            TEST_SELECTED_AT_NS,
            fault,
        )
    }

    fn select(
        ledger: &FaultOutcomeLedger,
        attempt: &FaultAttempt,
    ) -> Result<FaultOutcomeLedger, FaultTransitionError> {
        transition_fault_outcome(ledger, Some(attempt), attempt.id, FaultStageKind::Selected)
    }

    fn process_kill_effect(target: u32) -> FaultPlanEffect {
        FaultPlanEffect::ProcessKill { target }
    }

    #[test]
    fn attempt_identity_is_stable_and_parameter_sensitive() {
        let first = attempt(Fault::ProcessKill { target: 0 });
        let repeated = attempt(Fault::ProcessKill { target: 0 });
        let changed = attempt(Fault::ProcessKill { target: 1 });
        assert_eq!(first.id, repeated.id);
        assert_ne!(first.id, changed.id);
        assert!(first.has_valid_identity());
        assert!(!first.id.to_string().is_empty());
    }

    #[test]
    fn valid_transition_trace_updates_stage_specific_counters() {
        // r[verify chaoscontrol.fault_outcomes.validation.core]
        let attempt = attempt(Fault::ProcessKill { target: 0 });
        let selected = select(&FaultOutcomeLedger::default(), &attempt).unwrap();
        let applicable = transition_fault_outcome(
            &selected,
            None,
            attempt.id,
            FaultStageKind::Applicable {
                effect: process_kill_effect(0),
            },
        )
        .unwrap();
        let applied = transition_fault_outcome(
            &applicable,
            None,
            attempt.id,
            FaultStageKind::Applied {
                effect: process_kill_effect(0),
            },
        )
        .unwrap();
        let observation = FaultObservation::new(
            attempt.id,
            FaultObservationSubsystem::Process,
            0,
            FaultObservationEffect::ProcessSkipped,
        );
        let observed = transition_fault_outcome(
            &applied,
            None,
            attempt.id,
            FaultStageKind::Observed { observation },
        )
        .unwrap();
        assert_eq!(observed.events.len(), 4);
        assert_eq!(observed.counters.selected, 1);
        assert_eq!(observed.counters.applied, 1);
        assert_eq!(observed.counters.observed, 1);
        assert_eq!(observed.counters.rejected, 0);
        assert_eq!(observed.counters.application_failed, 0);
    }

    #[test]
    fn invalid_transitions_leave_authoritative_state_unchanged() {
        // r[verify chaoscontrol.fault_outcomes.validation.negative]
        let attempt = attempt(Fault::ProcessKill { target: 0 });
        let selected = select(&FaultOutcomeLedger::default(), &attempt).unwrap();
        let duplicate = select(&selected, &attempt);
        assert_eq!(duplicate, Err(FaultTransitionError::DuplicateAttempt));
        assert_eq!(selected.counters.selected, 1);
        assert_eq!(selected.events.len(), 1);

        let out_of_order = transition_fault_outcome(
            &selected,
            None,
            attempt.id,
            FaultStageKind::Applied {
                effect: process_kill_effect(0),
            },
        );
        assert!(matches!(
            out_of_order,
            Err(FaultTransitionError::InvalidTransition { .. })
        ));
        assert_eq!(selected.counters.applied, 0);
        assert_eq!(selected.events.len(), 1);
    }

    #[test]
    fn tampered_plan_and_observation_bindings_are_rejected_without_mutation() {
        // r[verify chaoscontrol.fault_outcomes.validation.negative]
        const ORIGINAL_OPERATION_SEQUENCE: u64 = 1;
        const TAMPERED_OPERATION_SEQUENCE: u64 = 2;
        let attempt = attempt(Fault::ProcessKill { target: 0 });
        let selected = select(&FaultOutcomeLedger::default(), &attempt).unwrap();
        let wrong_applicable = transition_fault_outcome(
            &selected,
            None,
            attempt.id,
            FaultStageKind::Applicable {
                effect: process_kill_effect(1),
            },
        );
        assert_eq!(
            wrong_applicable,
            Err(FaultTransitionError::ApplicablePlanMismatch)
        );
        assert_eq!(selected.events.len(), 1);

        let loss_fault = Fault::PacketLoss {
            target: 0,
            rate_ppm: 1,
        };
        let loss_schedule_id = fault_schedule_id([(TEST_SELECTED_AT_NS, None, &loss_fault)]);
        let loss_run_id = fault_run_id(TEST_SEED, TEST_SCHEDULE_SEQUENCE, loss_schedule_id);
        let loss_attempt = FaultAttempt::new(
            loss_run_id,
            TEST_SCHEDULE_SEQUENCE,
            loss_schedule_id,
            0,
            TEST_SELECTED_AT_NS,
            loss_fault,
        );
        let loss_selected = select(&FaultOutcomeLedger::default(), &loss_attempt).unwrap();
        let timing_tamper = FaultPlanEffect::PacketLoss {
            target: 0,
            rate_ppm: 0,
        };
        assert_eq!(timing_tamper.timing(), FaultEffectTiming::Immediate);
        let timing_result = transition_fault_outcome(
            &loss_selected,
            None,
            loss_attempt.id,
            FaultStageKind::Applicable {
                effect: timing_tamper,
            },
        );
        assert_eq!(
            timing_result,
            Err(FaultTransitionError::ApplicablePlanMismatch)
        );

        let applicable = transition_fault_outcome(
            &selected,
            None,
            attempt.id,
            FaultStageKind::Applicable {
                effect: process_kill_effect(0),
            },
        )
        .unwrap();

        let wrong_plan = transition_fault_outcome(
            &applicable,
            None,
            attempt.id,
            FaultStageKind::Applied {
                effect: process_kill_effect(1),
            },
        );
        assert_eq!(wrong_plan, Err(FaultTransitionError::AppliedPlanMismatch));
        assert_eq!(applicable.counters.applied, 0);

        let applied = transition_fault_outcome(
            &applicable,
            None,
            attempt.id,
            FaultStageKind::Applied {
                effect: process_kill_effect(0),
            },
        )
        .unwrap();
        let mut tampered_sequence = FaultObservation::new(
            attempt.id,
            FaultObservationSubsystem::Process,
            ORIGINAL_OPERATION_SEQUENCE,
            FaultObservationEffect::ProcessSkipped,
        );
        tampered_sequence.operation_sequence = TAMPERED_OPERATION_SEQUENCE;
        let identity_result = transition_fault_outcome(
            &applied,
            None,
            attempt.id,
            FaultStageKind::Observed {
                observation: tampered_sequence,
            },
        );
        assert_eq!(
            identity_result,
            Err(FaultTransitionError::ObservationIdentityMismatch)
        );

        let wrong_subsystem = FaultObservation::new(
            attempt.id,
            FaultObservationSubsystem::Network,
            ORIGINAL_OPERATION_SEQUENCE,
            FaultObservationEffect::ProcessSkipped,
        );
        let subsystem_result = transition_fault_outcome(
            &applied,
            None,
            attempt.id,
            FaultStageKind::Observed {
                observation: wrong_subsystem,
            },
        );
        assert_eq!(
            subsystem_result,
            Err(FaultTransitionError::ObservationSubsystemMismatch)
        );

        let wrong_effect = FaultObservation::new(
            attempt.id,
            FaultObservationSubsystem::Process,
            ORIGINAL_OPERATION_SEQUENCE,
            FaultObservationEffect::ProcessRestarted,
        );
        let effect_result = transition_fault_outcome(
            &applied,
            None,
            attempt.id,
            FaultStageKind::Observed {
                observation: wrong_effect,
            },
        );
        assert_eq!(
            effect_result,
            Err(FaultTransitionError::ObservationEffectMismatch)
        );
        assert_eq!(applied.counters.observed, 0);
        assert_eq!(applied.events.len(), 3);
    }

    #[test]
    fn clock_plan_converts_nanoseconds_with_vm_frequency() {
        const ONE_MILLISECOND_NS: i64 = 1_000_000;
        let clock_attempt = attempt(Fault::ClockSkew {
            target: 0,
            offset_ns: ONE_MILLISECOND_NS,
        });

        let plan =
            plan_fault_application(&clock_attempt, &facts(), &FaultApplicationPolicy::default())
                .unwrap();

        assert_eq!(
            plan.effect,
            FaultPlanEffect::VirtualClockSkew {
                target: 0,
                basis_tsc: TEST_SELECTED_AT_NS,
                tsc_khz: TEST_TSC_KHZ,
                offset_ns: ONE_MILLISECOND_NS,
                tsc_delta: i64::from(TEST_TSC_KHZ),
                target_tsc: TEST_SELECTED_AT_NS + u64::from(TEST_TSC_KHZ),
            }
        );
    }

    #[test]
    fn clock_plan_rejects_zero_and_sub_period_deltas() {
        let zero_attempt = attempt(Fault::ClockSkew {
            target: 0,
            offset_ns: 0,
        });
        assert_eq!(
            plan_fault_application(&zero_attempt, &facts(), &FaultApplicationPolicy::default(),),
            Err(FaultRejectionReason::ZeroClockDelta)
        );

        const SUB_PERIOD_DELTA_NS: i64 = 1;
        const LOW_TEST_TSC_KHZ: u32 = 1;
        let sub_period_attempt = attempt(Fault::ClockJump {
            target: 0,
            delta_ns: SUB_PERIOD_DELTA_NS,
        });
        let mut low_frequency_facts = facts();
        low_frequency_facts.vms[0].tsc_khz = LOW_TEST_TSC_KHZ;
        assert_eq!(
            plan_fault_application(
                &sub_period_attempt,
                &low_frequency_facts,
                &FaultApplicationPolicy::default(),
            ),
            Err(FaultRejectionReason::ClockDeltaRoundsToZero {
                value_ns: SUB_PERIOD_DELTA_NS,
                tsc_khz: LOW_TEST_TSC_KHZ,
            })
        );
    }

    #[test]
    fn clock_plan_rejects_target_and_derivation_tampering() {
        const CLOCK_OFFSET_NS: i64 = 1_000_000;
        let clock_attempt = attempt(Fault::ClockSkew {
            target: 0,
            offset_ns: CLOCK_OFFSET_NS,
        });
        let selected = select(&FaultOutcomeLedger::default(), &clock_attempt).unwrap();
        let effect =
            plan_fault_application(&clock_attempt, &facts(), &FaultApplicationPolicy::default())
                .unwrap()
                .effect;
        let mut tampered_effects = Vec::new();
        for field in 0..5 {
            let mut tampered = effect.clone();
            let FaultPlanEffect::VirtualClockSkew {
                basis_tsc,
                tsc_khz,
                offset_ns,
                tsc_delta,
                target_tsc,
                ..
            } = &mut tampered
            else {
                panic!("clock skew planner returned the wrong effect");
            };
            match field {
                0 => *basis_tsc += 1,
                1 => *tsc_khz += 1,
                2 => *offset_ns += 1,
                3 => *tsc_delta += 1,
                4 => *target_tsc += 1,
                _ => unreachable!(),
            }
            tampered_effects.push(tampered);
        }

        for tampered in tampered_effects {
            let result = transition_fault_outcome(
                &selected,
                None,
                clock_attempt.id,
                FaultStageKind::Applicable { effect: tampered },
            );
            assert_eq!(result, Err(FaultTransitionError::ApplicablePlanMismatch));
        }
        assert_eq!(selected.events.len(), 1);
    }

    #[test]
    fn stale_duplicate_and_counter_overflow_are_rejected() {
        let attempt = attempt(Fault::ProcessKill { target: 0 });
        let unknown = transition_fault_outcome(
            &FaultOutcomeLedger::default(),
            None,
            attempt.id,
            FaultStageKind::Applicable {
                effect: process_kill_effect(0),
            },
        );
        assert_eq!(unknown, Err(FaultTransitionError::UnknownAttempt));

        let mut overflow = FaultOutcomeLedger::default();
        overflow.counters.selected = u64::MAX;
        let result = select(&overflow, &attempt);
        assert_eq!(result, Err(FaultTransitionError::CounterOverflow));
        assert_eq!(overflow.counters.selected, u64::MAX);
        assert!(overflow.attempts.is_empty());
    }

    #[test]
    fn full_variant_matrix_has_a_supported_or_explicitly_unsupported_plan() {
        // r[verify chaoscontrol.fault_outcomes.validation]
        // r[verify chaoscontrol.fault_outcomes.validation.variant_matrix]
        let cases = representative_faults();
        assert_eq!(cases.len(), FaultVariant::ALL.len());
        for fault in cases {
            let variant = fault.variant();
            let result = plan_fault_application(
                &attempt(fault),
                &facts(),
                &FaultApplicationPolicy::default(),
            );
            if variant == FaultVariant::ProcessRestart {
                assert!(matches!(
                    result,
                    Err(FaultRejectionReason::VmNotCrashed { .. })
                ));
            } else {
                assert!(result.is_ok(), "{variant:?}: {result:?}");
            }
        }
    }

    #[test]
    fn new_fault_surface_plans_bind_windows_baselines_and_capabilities() {
        // r[verify chaoscontrol.fault_surface.validation]
        const WINDOW_TICKS: u64 = 3;
        const JITTER_BOUND_TSC: u64 = 17;
        const MEMORY_LIMIT_BYTES: u64 = TEST_MEMORY_SIZE_BYTES / 2;
        let facts = facts();
        let policy = FaultApplicationPolicy::default();

        let freeze = plan_fault_application(
            &attempt(Fault::ClockFreeze {
                target: 0,
                duration_ticks: WINDOW_TICKS,
            }),
            &facts,
            &policy,
        )
        .unwrap();
        assert_eq!(
            freeze.effect,
            FaultPlanEffect::VirtualClockFreeze {
                target: 0,
                frozen_tsc: TEST_SELECTED_AT_NS,
                release_at_tick: facts.current_tick + WINDOW_TICKS,
            }
        );

        let jitter = plan_fault_application(
            &attempt(Fault::ClockJitter {
                target: 0,
                bound_tsc: JITTER_BOUND_TSC,
            }),
            &facts,
            &policy,
        )
        .unwrap();
        assert_eq!(
            jitter.effect,
            FaultPlanEffect::VirtualClockJitter {
                target: 0,
                bound_tsc: JITTER_BOUND_TSC,
            }
        );

        let stall = plan_fault_application(
            &attempt(Fault::CpuStall {
                target: 0,
                vcpu: 1,
                duration_ticks: WINDOW_TICKS,
            }),
            &facts,
            &policy,
        )
        .unwrap();
        assert_eq!(
            stall.effect,
            FaultPlanEffect::CpuStall {
                target: 0,
                vcpu: 1,
                release_at_tick: facts.current_tick + WINDOW_TICKS,
            }
        );

        let pressure = plan_fault_application(
            &attempt(Fault::MemoryPressure {
                target: 0,
                limit_bytes: MEMORY_LIMIT_BYTES,
                duration_ticks: WINDOW_TICKS,
            }),
            &facts,
            &policy,
        )
        .unwrap();
        assert_eq!(
            pressure.effect,
            FaultPlanEffect::MemoryPressure {
                target: 0,
                limit_bytes: MEMORY_LIMIT_BYTES,
                baseline_bytes: TEST_MEMORY_SIZE_BYTES,
                release_at_tick: facts.current_tick + WINDOW_TICKS,
            }
        );

        let mut unsupported = facts.clone();
        unsupported.vms[0].supports_clock_freeze = false;
        assert!(matches!(
            plan_fault_application(
                &attempt(Fault::ClockFreeze {
                    target: 0,
                    duration_ticks: WINDOW_TICKS,
                }),
                &unsupported,
                &policy,
            ),
            Err(FaultRejectionReason::UnsupportedCapability { .. })
        ));
    }

    #[test]
    fn invalid_targets_parameters_ranges_and_capabilities_are_rejected() {
        const MISSING_TARGET: usize = 99;
        const OVERFLOWING_RANGE_LENGTH: usize = 2;
        let policy = FaultApplicationPolicy::default();
        let valid_facts = facts();
        let missing_target = attempt(Fault::ProcessKill {
            target: MISSING_TARGET,
        });
        assert!(matches!(
            plan_fault_application(&missing_target, &valid_facts, &policy),
            Err(FaultRejectionReason::MissingVm { .. })
        ));

        let invalid_rate = attempt(Fault::PacketLoss {
            target: 0,
            rate_ppm: PARTS_PER_MILLION_MAX + 1,
        });
        assert!(matches!(
            plan_fault_application(&invalid_rate, &valid_facts, &policy),
            Err(FaultRejectionReason::InvalidRate { .. })
        ));

        let overflow = attempt(Fault::DiskCorruption {
            target: 0,
            offset: u64::MAX,
            len: OVERFLOWING_RANGE_LENGTH,
        });
        assert!(matches!(
            plan_fault_application(&overflow, &valid_facts, &policy),
            Err(FaultRejectionReason::RangeOverflow { .. })
        ));

        let invalid_bit = attempt(Fault::CpuBitflip {
            target: 0,
            vcpu: 0,
            register: GpRegister::Rax,
            bit: GENERAL_REGISTER_BIT_COUNT,
        });
        assert!(matches!(
            plan_fault_application(&invalid_bit, &valid_facts, &policy),
            Err(FaultRejectionReason::InvalidRegisterBit { .. })
        ));

        let invalid_vcpu = attempt(Fault::InjectNmi {
            target: 0,
            vcpu: usize::MAX,
        });
        assert!(matches!(
            plan_fault_application(&invalid_vcpu, &valid_facts, &policy),
            Err(FaultRejectionReason::InvalidVcpu { .. })
        ));

        let zero_duration = attempt(Fault::ProcessPause {
            target: 0,
            duration_ns: 0,
        });
        assert!(matches!(
            plan_fault_application(&zero_duration, &valid_facts, &policy),
            Err(FaultRejectionReason::ZeroDuration)
        ));

        let mut missing_device_facts = valid_facts.clone();
        missing_device_facts.vms[0].block_device_size_bytes = None;
        let missing_device = attempt(Fault::DiskFull { target: 0 });
        assert!(matches!(
            plan_fault_application(&missing_device, &missing_device_facts, &policy),
            Err(FaultRejectionReason::MissingBlockDevice { .. })
        ));

        let mut overflowing_clock_facts = valid_facts.clone();
        overflowing_clock_facts.vms[0].virtual_tsc = u64::MAX;
        let overflowing_clock = attempt(Fault::ClockJump {
            target: 0,
            delta_ns: 1,
        });
        assert!(matches!(
            plan_fault_application(&overflowing_clock, &overflowing_clock_facts, &policy),
            Err(FaultRejectionReason::ArithmeticOverflow)
        ));

        let unsupported = attempt(Fault::MemoryPressure {
            target: 0,
            limit_bytes: 1,
            duration_ticks: 1,
        });
        let mut unsupported_facts = valid_facts.clone();
        unsupported_facts.vms[0].supports_memory_pressure = false;
        assert!(matches!(
            plan_fault_application(&unsupported, &unsupported_facts, &policy),
            Err(FaultRejectionReason::UnsupportedCapability { .. })
        ));
    }

    #[test]
    fn later_application_failure_preserves_prior_ordered_outcomes() {
        // r[verify chaoscontrol.fault_outcomes.application_failure]
        let first = attempt(Fault::ProcessKill { target: 0 });
        let mut second = attempt(Fault::InjectInterrupt { target: 0, irq: 1 });
        second.selection_index = 1;
        second.id = fault_attempt_id(
            second.run_id,
            second.schedule_id,
            second.selection_index,
            second.selected_at_ns,
            second.source,
            &second.fault,
        );

        let ledger = select(&FaultOutcomeLedger::default(), &first).unwrap();
        let ledger = transition_fault_outcome(
            &ledger,
            None,
            first.id,
            FaultStageKind::Applicable {
                effect: process_kill_effect(0),
            },
        )
        .unwrap();
        let ledger = transition_fault_outcome(
            &ledger,
            None,
            first.id,
            FaultStageKind::Applied {
                effect: process_kill_effect(0),
            },
        )
        .unwrap();
        let ledger = select(&ledger, &second).unwrap();
        let ledger = transition_fault_outcome(
            &ledger,
            None,
            second.id,
            FaultStageKind::Applicable {
                effect: FaultPlanEffect::IrqInjection { target: 0, irq: 1 },
            },
        )
        .unwrap();
        let ledger = transition_fault_outcome(
            &ledger,
            None,
            second.id,
            FaultStageKind::ApplicationFailed {
                reason: FaultApplicationFailureReason::BackendRejected,
                disposition: FaultApplicationFailureDisposition::NonRunnable,
            },
        )
        .unwrap();

        assert_eq!(ledger.events.len(), 6);
        assert_eq!(ledger.events[2].attempt_id, first.id);
        assert_eq!(ledger.events[5].attempt_id, second.id);
        assert_eq!(ledger.counters.selected, 2);
        assert_eq!(ledger.counters.applied, 1);
        assert_eq!(ledger.counters.application_failed, 1);
        assert_eq!(ledger.counters.observed, 0);
    }

    #[test]
    fn duplicate_observation_does_not_change_authoritative_state() {
        let attempt = attempt(Fault::ProcessKill { target: 0 });
        let ledger = select(&FaultOutcomeLedger::default(), &attempt).unwrap();
        let ledger = transition_fault_outcome(
            &ledger,
            None,
            attempt.id,
            FaultStageKind::Applicable {
                effect: process_kill_effect(0),
            },
        )
        .unwrap();
        let ledger = transition_fault_outcome(
            &ledger,
            None,
            attempt.id,
            FaultStageKind::Applied {
                effect: process_kill_effect(0),
            },
        )
        .unwrap();
        let observation = FaultObservation::new(
            attempt.id,
            FaultObservationSubsystem::Process,
            0,
            FaultObservationEffect::ProcessSkipped,
        );
        let observed = transition_fault_outcome(
            &ledger,
            None,
            attempt.id,
            FaultStageKind::Observed {
                observation: observation.clone(),
            },
        )
        .unwrap();
        let duplicate = transition_fault_outcome(
            &observed,
            None,
            attempt.id,
            FaultStageKind::Observed { observation },
        );
        assert_eq!(duplicate, Err(FaultTransitionError::DuplicateObservation));
        assert_eq!(observed.counters.observed, 1);
        assert_eq!(observed.events.len(), 4);
    }

    #[test]
    fn identical_facts_produce_identical_plans() {
        // r[verify chaoscontrol.fault_outcomes.applicability]
        let attempt = attempt(Fault::NetworkLatency {
            target: 0,
            latency_ns: NANOSECONDS_PER_SIMULATION_TICK,
        });
        let first = plan_fault_application(&attempt, &facts(), &FaultApplicationPolicy::default());
        let second = plan_fault_application(&attempt, &facts(), &FaultApplicationPolicy::default());
        assert_eq!(first, second);
        assert!(first.is_ok());
    }

    fn representative_faults() -> Vec<Fault> {
        vec![
            Fault::NetworkPartition {
                side_a: vec![0],
                side_b: vec![1],
            },
            Fault::NetworkLatency {
                target: 0,
                latency_ns: NANOSECONDS_PER_SIMULATION_TICK,
            },
            Fault::PacketLoss {
                target: 0,
                rate_ppm: 1,
            },
            Fault::PacketCorruption {
                target: 0,
                rate_ppm: 1,
            },
            Fault::PacketReorder {
                target: 0,
                window_ns: NANOSECONDS_PER_SIMULATION_TICK,
            },
            Fault::NetworkJitter {
                target: 0,
                jitter_ns: NANOSECONDS_PER_SIMULATION_TICK,
            },
            Fault::NetworkBandwidth {
                target: 0,
                bytes_per_sec: 1,
            },
            Fault::PacketDuplicate {
                target: 0,
                rate_ppm: 1,
            },
            Fault::NetworkHeal,
            Fault::DiskReadError {
                target: 0,
                offset: 0,
            },
            Fault::DiskWriteError {
                target: 0,
                offset: 0,
            },
            Fault::DiskTornWrite {
                target: 0,
                offset: 0,
                bytes_written: 1,
            },
            Fault::DiskCorruption {
                target: 0,
                offset: 0,
                len: 1,
            },
            Fault::DiskFull { target: 0 },
            Fault::ProcessKill { target: 0 },
            Fault::ProcessPause {
                target: 0,
                duration_ns: NANOSECONDS_PER_SIMULATION_TICK,
            },
            Fault::ProcessRestart { target: 0 },
            Fault::ClockSkew {
                target: 0,
                offset_ns: 1,
            },
            Fault::ClockJump {
                target: 0,
                delta_ns: 1,
            },
            Fault::MemoryPressure {
                target: 0,
                limit_bytes: 1,
                duration_ticks: 1,
            },
            Fault::InjectInterrupt { target: 0, irq: 1 },
            Fault::InjectNmi { target: 0, vcpu: 0 },
            Fault::DiskSlow {
                target: 0,
                delay_ns: 1,
            },
            Fault::DiskFsyncLie { target: 0 },
            Fault::DiskFsyncFlush { target: 0 },
            Fault::DiskPartialRead {
                target: 0,
                offset: 0,
                max_bytes: 1,
            },
            Fault::CpuBitflip {
                target: 0,
                vcpu: 0,
                register: GpRegister::Rax,
                bit: 0,
            },
            Fault::CpuStall {
                target: 0,
                vcpu: 0,
                duration_ticks: 1,
            },
            Fault::ClockFreeze {
                target: 0,
                duration_ticks: 1,
            },
            Fault::ClockJitter {
                target: 0,
                bound_tsc: 1,
            },
        ]
    }
}
