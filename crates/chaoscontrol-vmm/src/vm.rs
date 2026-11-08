//! Core VM implementation for the ChaosControl deterministic hypervisor.
//!
//! [`DeterministicVm`] is the main entry point: it creates a KVM-backed
//! virtual machine with deterministic CPU, memory, clock, and I/O
//! behaviour suitable for simulation testing.
//!
//! # Example
//!
//! ```no_run
//! use chaoscontrol_vmm::vm::{DeterministicVm, VmConfig};
//!
//! let config = VmConfig::default();
//! let mut vm = DeterministicVm::new(config).unwrap();
//! vm.load_kernel("/path/to/vmlinux", Some("/path/to/initrd.gz"))
//!     .unwrap();
//! vm.run().unwrap();
//! ```

use crate::acpi;
use crate::cpu::{self, CpuConfig, VirtualTsc};
use crate::devices::entropy::DeterministicEntropy;
use crate::devices::pit::DeterministicPit;
use crate::devices::virtio_mmio::{MmioWriteEffect, VirtioMmioDevice};
use crate::dlog::{DlogRecord, DlogTag, DlogWriter};
use crate::scheduler::core::{
    canonical_runnable_changes, plan_execution_observation, validate_progress_capabilities,
    BoundaryIoDirection, BoundaryProfileId, ExactStepState, ExecutionProgressObservation,
    KvmBoundaryKind, ProgressBudget, ProgressCapabilities, ProgressMode, ScheduleAction,
    ScheduleError, ScheduleEvent, ScheduleReservation, ScheduleTrace, ScheduleTransitionRecord,
    DEFAULT_SCHEDULE_JOURNAL_LIMIT, MAX_SCHEDULE_VCPUS,
};
use crate::scheduler::{
    SchedulerConfig, SchedulingStrategy, VcpuScheduler, DEFAULT_SMP_INSTRUCTION_QUANTUM,
};

use crate::memory::{
    self, build_e820_map, code64_segment, data_segment, tss_segment, GuestMemoryManager,
    BOOT_GDT_OFFSET, BOOT_IDT_OFFSET, BOOT_STACK_POINTER, CMDLINE_START, GDT_ENTRY_COUNT,
    HIMEM_START, PML4_START, RNG_SETUP_DATA_START, ZERO_PAGE_START,
};
use chaoscontrol_fault::engine::{
    AssertionHitEvent, EngineConfig, FaultEngine, MAX_ASSERTION_HIT_EVENTS,
};
use chaoscontrol_protocol::{
    HypercallPage, CMD_ASSERT_ALWAYS, CMD_ASSERT_REACHABLE, CMD_ASSERT_SOMETIMES,
    CMD_ASSERT_UNREACHABLE, CMD_LIFECYCLE_SEND_EVENT, COVERAGE_BITMAP_ADDR, COVERAGE_BITMAP_SIZE,
    COVERAGE_PORT, HYPERCALL_PAGE_ADDR, HYPERCALL_PAGE_SIZE, SDK_PORT, VMCALL_NR,
};
use chaoscontrol_sim_core::{
    build_guest_determinism_profile, derive_boot_entropy_seed, encode_linux_rng_seed_setup_data,
    plan_runtime_capacity, validate_guest_determinism_profile, CapacityUsageObservation,
    GuestClockMode, GuestDeterminismInput, GuestDeterminismProfile, RuntimeCapacityClaims,
    RuntimeCapacityLimits, RuntimeCapacityObservations, RuntimeCapacityPlan,
    RuntimeCapacityStartupResult, ScratchClassLimit, RUNTIME_CAPACITY_OBSERVATIONS_SCHEMA,
};

use kvm_bindings::{
    kvm_clock_data, kvm_enable_cap, kvm_fpu, kvm_guest_debug, kvm_pit_config, kvm_regs,
    kvm_userspace_memory_region, KVM_GUESTDBG_ENABLE, KVM_GUESTDBG_SINGLESTEP,
    KVM_MEM_LOG_DIRTY_PAGES, KVM_MP_STATE_RUNNABLE, KVM_PIT_FLAGS_HPET_LEGACY,
    KVM_PIT_SPEAKER_DUMMY,
};
use kvm_ioctls::{Cap, Kvm, VcpuExit, VcpuFd, VmFd};
use linux_loader::configurator::linux::LinuxBootConfigurator;
use linux_loader::configurator::{BootConfigurator, BootParams};
use linux_loader::loader::bootparam::boot_params;
use linux_loader::loader::elf::Elf;

use crate::unsafe_owner::SendTimerId;
use crate::vm_core::{
    capture_serial_bounded, hlt_snapshot_is_valid, plan_poison, plan_teardown,
    plan_vcpu_construction, sanitize_serial_for_terminal, serial_snapshot_fits,
    validate_snapshot_count,
};
use chaoscontrol_fault::faults::GpRegister;

/// Read a general-purpose register from KVM regs.
fn gp_register_get(regs: &kvm_regs, reg: GpRegister) -> u64 {
    match reg {
        GpRegister::Rax => regs.rax,
        GpRegister::Rbx => regs.rbx,
        GpRegister::Rcx => regs.rcx,
        GpRegister::Rdx => regs.rdx,
        GpRegister::Rsi => regs.rsi,
        GpRegister::Rdi => regs.rdi,
        GpRegister::Rbp => regs.rbp,
        GpRegister::Rsp => regs.rsp,
        GpRegister::R8 => regs.r8,
        GpRegister::R9 => regs.r9,
        GpRegister::R10 => regs.r10,
        GpRegister::R11 => regs.r11,
        GpRegister::R12 => regs.r12,
        GpRegister::R13 => regs.r13,
        GpRegister::R14 => regs.r14,
        GpRegister::R15 => regs.r15,
    }
}

/// Write a general-purpose register into KVM regs.
fn gp_register_set(regs: &mut kvm_regs, reg: GpRegister, val: u64) {
    match reg {
        GpRegister::Rax => regs.rax = val,
        GpRegister::Rbx => regs.rbx = val,
        GpRegister::Rcx => regs.rcx = val,
        GpRegister::Rdx => regs.rdx = val,
        GpRegister::Rsi => regs.rsi = val,
        GpRegister::Rdi => regs.rdi = val,
        GpRegister::Rbp => regs.rbp = val,
        GpRegister::Rsp => regs.rsp = val,
        GpRegister::R8 => regs.r8 = val,
        GpRegister::R9 => regs.r9 = val,
        GpRegister::R10 => regs.r10 = val,
        GpRegister::R11 => regs.r11 = val,
        GpRegister::R12 => regs.r12 = val,
        GpRegister::R13 => regs.r13 = val,
        GpRegister::R14 => regs.r14 = val,
        GpRegister::R15 => regs.r15 = val,
    }
}
use linux_loader::loader::KernelLoader;
use log::info;
use snafu::{ResultExt, Snafu};
use std::fs::File;
use std::io;
use std::path::PathBuf;
use vm_memory::{Address, Bytes, GuestAddress};

// ═══════════════════════════════════════════════════════════════════════
//  Constants
// ═══════════════════════════════════════════════════════════════════════

// x86_64 control register flags
const X86_CR0_PE: u64 = 0x1;
const X86_CR0_PG: u64 = 0x8000_0000;
const X86_CR4_PAE: u64 = 0x20;
const EFER_LME: u64 = 0x100;
const EFER_LMA: u64 = 0x400;

// Serial port I/O range (COM1)
const SERIAL_PORT_BASE: u16 = 0x3f8;
const SERIAL_PORT_END: u16 = 0x3ff;

/// ACPI PM Timer I/O port (standard address from FADT).
///
/// Reads to this port return a free-running 24-bit counter that ticks at
/// 3.579545 MHz.  We intercept it and return a deterministic value derived
/// from our virtual TSC to prevent guest code from reading host wall time.
const ACPI_PM_TIMER_PORT: u16 = 0x408;

/// ACPI PM Timer frequency in Hz (defined by ACPI spec: 3.579545 MHz).
const ACPI_PM_TIMER_FREQ_HZ: u64 = 3_579_545;

/// HPET MMIO base address (standard x86 location).
///
/// The HPET occupies a 1 KiB MMIO region at 0xFED0_0000.  We trap reads
/// to this region and return deterministic values to prevent the guest
/// from accessing a real-time clock source.
const HPET_MMIO_BASE: u64 = 0xFED0_0000;

/// HPET MMIO region size (1 KiB, covers all timer registers).
const HPET_MMIO_SIZE: u64 = 0x400;

/// HPET General Capabilities and ID Register offset.
const HPET_REG_CAP: u64 = 0x000;

/// HPET General Configuration Register offset.
const HPET_REG_CONFIG: u64 = 0x010;

/// HPET Main Counter Value Register offset.
const HPET_REG_COUNTER: u64 = 0x0F0;

/// COM1 IRQ line number (standard PC).
const SERIAL_IRQ: u32 = 4;
/// UART 16550 input FIFO capacity used by vm-superio.
const SERIAL_FIFO_CAPACITY: usize = 0x40;

/// Serial crash detection patterns (big-endian u64 sliding windows).
///
/// Each serial byte is shifted into a u64 sliding window. When it
/// matches any of these constants, we set `panic_detected`.
///
/// Multiple patterns catch crashes that don't print "Kernel panic"
/// (e.g., double faults where the GPF handler itself faults).
const PANIC_PATTERNS: [u64; 4] = [
    u64::from_be_bytes(*b"Kernel p"), // "Kernel panic - not syncing:"
    u64::from_be_bytes(*b"---[ end"), // "---[ end trace ... ]---" (every oops)
    u64::from_be_bytes(*b"RIP: 001"), // kernel-mode crash dump (CS=0x0010)
    u64::from_be_bytes(*b"end Kern"), // "end Kernel panic" (panic footer)
];

/// PIT timer IRQ line number (standard PC, IRQ 0).
const PIT_IRQ: u32 = 0;

/// Bootstrap processor index used by legacy PIC delivery.
const BSP_VCPU_INDEX: usize = 0;

/// Single-vCPU operational watchdog interval in microseconds.
const SINGLE_VCPU_WATCHDOG_MICROSECONDS: i64 = 100_000;

/// Named coarse per-entry recovery bound. This has abort authority only.
pub const DEFAULT_SMP_ENTRY_WATCHDOG_MICROSECONDS: u64 = 250_000;

/// Consecutive host-watchdog interrupts without an intervening KVM exit that
/// still count as a runnable guest making progress.
///
/// A deterministic single-vCPU guest may compute for a long stretch without a
/// VM exit, and a loaded host can stretch that wall-clock window further. Only
/// an uninterrupted run of host timer interrupts with no guest exit at all is
/// treated as an operational timeout: 50 intervals is about five seconds at the
/// default 100 ms interval.
const MAX_RUNNABLE_WATCHDOG_RETRIES: u32 = 50;

/// KVM TSS address — must be set before create_irq_chip.
/// Placed at the top of the 32-bit address space (3 pages needed by KVM).
const KVM_TSS_ADDRESS: usize = 0xfffb_d000;

/// Get the current CLOCK_MONOTONIC time in nanoseconds.
///
/// Used to synchronize KVM PIT's `count_load_time` with our virtual time.
#[allow(dead_code)]
fn monotonic_ns() -> i64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: Valid timespec pointer, CLOCK_MONOTONIC is always available.
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    ts.tv_sec * 1_000_000_000 + ts.tv_nsec
}

// Virtio MMIO device placement in guest physical memory
/// Base address for virtio MMIO device 0 (block).
const VIRTIO_MMIO_BASE_0: u64 = 0xD000_0000;
/// IRQ line for virtio MMIO device 0 (block).
const VIRTIO_MMIO_IRQ_0: u32 = 5;

/// Base address for virtio MMIO device 1 (net).
const VIRTIO_MMIO_BASE_1: u64 = 0xD000_1000;
/// IRQ line for virtio MMIO device 1 (net).
const VIRTIO_MMIO_IRQ_1: u32 = 6;

/// Base address for virtio MMIO device 2 (entropy/rng).
const VIRTIO_MMIO_BASE_2: u64 = 0xD000_2000;
/// IRQ line for virtio MMIO device 2 (entropy/rng).
const VIRTIO_MMIO_IRQ_2: u32 = 7;
const VIRTIO_SCRATCH_DEVICE_COUNT: usize = 3;
const BYTES_PER_MIB_USIZE: usize = 1024 * 1024;
const BYTES_PER_MIB_U64: u64 = 1024 * 1024;
const DEFAULT_EMPTY_DISK_BYTES: usize = 16 * BYTES_PER_MIB_USIZE;

// ═══════════════════════════════════════════════════════════════════════
//  Configuration
// ═══════════════════════════════════════════════════════════════════════

/// Configuration for creating a [`DeterministicVm`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmConfig {
    /// Guest memory size in bytes (default: 256 MB).
    pub memory_size: usize,
    /// CPU determinism configuration.
    pub cpu: CpuConfig,
    /// Number of vCPUs (default: 1).
    ///
    /// SMP requires explicit admission of either strict instruction progress
    /// or the separate settled KVM debug-boundary profile.
    pub num_vcpus: usize,
    /// Scheduling strategy for multi-vCPU VMs.
    ///
    /// Only meaningful when `num_vcpus > 1`. Controls how vCPU execution
    /// time is divided: fixed round-robin or randomized quantum.
    pub scheduling_strategy: SchedulingStrategy,
    /// Declared deterministic SMP progress mode.
    ///
    /// Both exact mode and PMU exact remainder require full event-entry
    /// exactness. Neither falls back to wall-clock or coarse boundaries.
    pub smp_progress_mode: ProgressMode,
    /// Typed initial and round-robin SMP turn budget.
    pub smp_progress_budget: ProgressBudget,
    /// Finite coarse-mode KVM entry deadline; host time has abort authority only.
    pub smp_entry_watchdog_microseconds: u64,
    /// Maximum canonical schedule records retained between evidence drains.
    pub smp_schedule_journal_limit: usize,
    /// Kernel command line (NUL-terminated).
    pub cmdline: Vec<u8>,
    /// Optional path to a disk image file for the virtio-blk device.
    ///
    /// When set, the block device is initialized from this file instead
    /// of an empty zero-filled buffer. The file is read once at VM
    /// creation; subsequent snapshot/restore uses copy-on-write.
    pub disk_image_path: Option<String>,
    /// Extra kernel command line parameters appended to the default.
    ///
    /// Useful for passing guest-specific options like `raft_bug=fig8`.
    pub extra_cmdline: Option<String>,

    /// Pin the VM's vCPU thread(s) to a specific physical CPU core.
    ///
    /// When set, `sched_setaffinity` is called to bind the current
    /// thread (which runs `vcpu.run()`) to this core index. This
    /// eliminates host scheduler jitter, cache evictions from core
    /// migration, and NUMA effects — all of which can affect
    /// determinism of host-side operations and PMC behavior.
    ///
    /// When `None` (default), no affinity is set and the OS scheduler
    /// decides core placement.
    pub core_affinity: Option<usize>,

    /// VM identifier for multi-VM networking (default: 0).
    ///
    /// Used to generate unique MAC addresses: `[0x52, 0x54, 0x00, 0x12, 0x34, vm_id as u8]`.
    /// Also passed to the guest kernel via `vm_id=N` cmdline parameter.
    pub vm_id: usize,

    /// Path to write a determinism log (dlog) file.
    ///
    /// When set, every VM exit and significant event is recorded as a
    /// fixed-size 64-byte binary record. Two logs from runs with the
    /// same seed can be compared with `dlog_diff` to find the exact
    /// exit where execution diverged.
    ///
    /// When `None` (default), no logging occurs and there is zero
    /// overhead on the hot path.
    pub dlog_path: Option<PathBuf>,

    /// Emit a full RegisterDump dlog record every N VM exits.
    ///
    /// When 0 (default), no RegisterDump records are emitted.
    /// A value of 100 means one dump every 100 exits.
    pub dlog_register_interval: u64,

    /// Hash guest memory pages at snapshot boundaries and emit
    /// MemoryHash dlog records with CRC32 per page.
    ///
    /// Off by default — adds ~50ms per snapshot for 256 MB guests.
    pub dlog_memory_hash: bool,
}

fn capacity_usage(
    observations: crate::devices::virtio_buffer::ScratchPoolObservations,
) -> CapacityUsageObservation {
    CapacityUsageObservation {
        allocated_slots: observations.allocated_slots,
        allocated_bytes: observations.allocated_bytes,
        in_use: observations.in_use,
        high_water: observations.high_water,
        exhaustion_count: observations.exhaustion_count,
        release_count: observations.release_count,
    }
}

fn runtime_capacity_limits(config: &VmConfig) -> Result<RuntimeCapacityLimits, VmError> {
    let packet_slot_bytes = usize::try_from(
        crate::devices::virtio_types::DEFAULT_MAX_NET_FRAME_BYTES,
    )
    .map_err(|_| VmError::RuntimeCapacity {
        message: "default network frame limit does not fit usize".to_string(),
    })?;
    let retained_packet_bytes =
        usize::try_from(crate::devices::virtio_types::DEFAULT_MAX_NET_TX_BYTES).map_err(|_| {
            VmError::RuntimeCapacity {
                message: "default retained network byte limit does not fit usize".to_string(),
            }
        })?;
    Ok(RuntimeCapacityLimits {
        journal_record_slots: config.smp_schedule_journal_limit,
        scratch_classes: vec![ScratchClassLimit {
            slot_bytes: crate::devices::virtio_types::DEFAULT_SCRATCH_BYTES,
            slots: VIRTIO_SCRATCH_DEVICE_COUNT,
        }],
        packet_slots: crate::devices::virtio_types::DEFAULT_MAX_NET_TX_PACKETS,
        packet_slot_bytes,
        retained_packet_bytes,
        queue_metadata_slots: crate::devices::virtio_types::DEFAULT_MAX_NET_TX_PACKETS,
    })
}

const SINGLE_VCPU_MEMORY_BYTES: usize = 268_435_456;
const SINGLE_VCPU_CPU_FAMILY: u8 = 6;
const SINGLE_VCPU_CPU_MODEL: u8 = 85;
const SINGLE_VCPU_CPU_STEPPING: u8 = 4;

impl Default for VmConfig {
    fn default() -> Self {
        Self::single_vcpu(0, false)
    }
}

impl VmConfig {
    /// CPU policy actually exposed by this machine. SMP always uses the
    /// instruction-driven PIT/jiffies clock and hides the TSC capability.
    #[must_use]
    pub fn effective_cpu_config(&self) -> CpuConfig {
        let mut cpu = self.cpu.clone();
        cpu.hide_tsc |= self.num_vcpus > 1;
        cpu
    }

    /// Build the standard single-vCPU machine with an explicit seed and TSC visibility.
    ///
    /// Optional disks, affinity, extra boot arguments, and diagnostic outputs remain absent.
    /// This constructor performs no I/O and does not establish guest determinism.
    pub fn single_vcpu(seed: u64, hide_tsc: bool) -> Self {
        Self {
            memory_size: SINGLE_VCPU_MEMORY_BYTES,
            num_vcpus: 1,
            scheduling_strategy: SchedulingStrategy::RoundRobin,
            smp_progress_mode: ProgressMode::ExactSingleStep,
            smp_progress_budget: ProgressBudget::Instructions(DEFAULT_SMP_INSTRUCTION_QUANTUM),
            smp_entry_watchdog_microseconds: DEFAULT_SMP_ENTRY_WATCHDOG_MICROSECONDS,
            smp_schedule_journal_limit: DEFAULT_SCHEDULE_JOURNAL_LIMIT,
            cpu: CpuConfig {
                // Hide KVM so guest doesn't use kvm-clock (reads host wall time).
                // Set fixed family=6 (Intel) so kernel's native_calibrate_tsc()
                // trusts CPUID leaf 0x15 for exact TSC frequency instead of
                // doing non-deterministic PIT-based calibration.
                hide_hypervisor: true,
                // Hide TSC feature for SMP: prevents early boot calibration
                // (PIT + TSC loop) which is non-deterministic due to
                // wall-clock PIT reads. Kernel falls back to CPUID 0x15.
                // (Set dynamically for SMP in new() below.)
                hide_tsc,
                fixed_family: Some(SINGLE_VCPU_CPU_FAMILY),
                fixed_model: Some(SINGLE_VCPU_CPU_MODEL),
                fixed_stepping: Some(SINGLE_VCPU_CPU_STEPPING),
                tsc_khz: crate::cpu::DEFAULT_TSC_KHZ,
                allow_avx2: false,
                allow_avx512: false,
                fixed_frequency_mhz: None,
                seed,
                tsc_advance_per_tick: crate::cpu::DEFAULT_TSC_ADVANCE,
                tsc_mode: crate::cpu::TscAdvanceMode::PerExit,
            },
            // Deterministic boot parameters:
            // clocksource=tsc tsc=reliable: use our pinned TSC as main clock
            // lpj=6000000: fixed loops_per_jiffy, skip runtime calibration
            // nokaslr norandmaps: disable address randomization
            // nosmp noapic: single CPU, no APIC probing
            // nohpet: disable HPET (real-time clock source, non-deterministic)
            // acpi_pm_timer_off: disable ACPI PM timer registration
            // kfence.sample_interval=0: disable kfence (timing-dependent)
            // no_hash_pointers: make pointer output deterministic
            // virtio_mmio.device=<size>@<baseaddr>:<irq>: notify kernel of virtio devices
            cmdline: coarse_configuration::DEFAULT_KERNEL_CMDLINE.to_vec(),
            disk_image_path: None,
            extra_cmdline: None,
            core_affinity: None,
            vm_id: 0,
            dlog_path: None,
            dlog_register_interval: 0,
            dlog_memory_hash: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Error type
// ═══════════════════════════════════════════════════════════════════════

/// Post-entry stage that made exact schedule evidence unrecoverable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulePoisonStage {
    PmuDisable,
    PmuGeneration,
    PmuRead,
    KvmRunResult,
    ProgressPlanning,
    JournalCommit,
    ExitHandling,
    ScheduleAction,
    SnapshotQuiesce,
    SnapshotRestore,
}

/// Permanent execution poison retained after possible unrecorded guest progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleExecutionPoison {
    pub stage: SchedulePoisonStage,
    pub detail: String,
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum VmError {
    #[snafu(display("Failed to create KVM instance"))]
    KvmCreate { source: kvm_ioctls::Error },

    #[snafu(display("Failed to create VM"))]
    VmCreate { source: kvm_ioctls::Error },

    #[snafu(display("Failed to create vCPU"))]
    VcpuCreate { source: kvm_ioctls::Error },

    #[snafu(display("Failed to set user memory region"))]
    SetUserMemoryRegion { source: kvm_ioctls::Error },

    #[snafu(display("Guest memory error"), context(false))]
    Memory { source: memory::MemoryError },

    #[snafu(display("CPU configuration error"), context(false))]
    Cpu { source: cpu::CpuError },

    #[snafu(display("Failed to load kernel"))]
    KernelLoad { source: linux_loader::loader::Error },

    #[snafu(display("Failed to write to guest memory"))]
    GuestMemoryWrite,

    #[snafu(display("Failed to get vCPU registers"))]
    GetRegisters { source: kvm_ioctls::Error },

    #[snafu(display("Failed to set vCPU registers"))]
    SetRegisters { source: kvm_ioctls::Error },

    #[snafu(display("Failed to set vCPU special registers"))]
    SetSregs { source: kvm_ioctls::Error },

    #[snafu(display("Failed to get vCPU special registers"))]
    GetSregs { source: kvm_ioctls::Error },

    #[snafu(display("Failed to set FPU"))]
    SetFpu { source: kvm_ioctls::Error },

    #[snafu(display("Failed to create in-kernel IRQ chip"))]
    CreateIrqChip { source: kvm_ioctls::Error },

    #[snafu(display("Failed to configure PIT"))]
    CreatePit { source: kvm_ioctls::Error },

    #[snafu(display("Failed to set KVM clock"))]
    SetClock { source: kvm_ioctls::Error },

    #[snafu(display("Failed to run vCPU"))]
    VcpuRun { source: kvm_ioctls::Error },

    #[snafu(
        display("Deterministic schedule rejected input: {source}"),
        context(false)
    )]
    Schedule { source: ScheduleError },

    #[snafu(display("Deterministic progress capability unavailable: {message}"))]
    ProgressCapability { message: String },

    #[snafu(display("Runtime capacity admission failed before activation: {message}"))]
    RuntimeCapacity { message: String },

    #[snafu(display(
        "Deterministic schedule execution is permanently poisoned at {stage:?}: {detail}"
    ))]
    ScheduleExecutionPoisoned {
        stage: SchedulePoisonStage,
        detail: String,
    },

    #[snafu(display(
        "Simulation controller is permanently poisoned by failed round {round} at tick {tick}: {detail}"
    ))]
    ControllerRoundPoisoned {
        round: u64,
        tick: u64,
        detail: String,
    },

    #[snafu(display("Host watchdog interrupted vCPU {vcpu}; this is not a guest crash verdict"))]
    // r[impl chaoscontrol.deterministic_smp.watchdog]
    HostWatchdogTimeout { vcpu: usize },

    #[snafu(display("KVM reported an internal error for vCPU {vcpu}"))]
    KvmInternalError { vcpu: usize },

    #[snafu(display("IO error"), context(false))]
    Io { source: io::Error },

    #[snafu(display(
        "Network packet made round non-runnable from VM {from} to VM {to}: {reason:?}"
    ))]
    NetworkPacketNonRunnable {
        from: usize,
        to: usize,
        reason: crate::controller::NetworkSendError,
    },

    #[snafu(display("Snapshot error: {message}"))]
    Snapshot { message: String },

    #[snafu(display("Snapshot restore failed during {phase:?}; VM is non-runnable: {detail}"))]
    SnapshotRestoreFailed {
        phase: SnapshotRestorePhase,
        detail: String,
    },

    #[snafu(display("Disk image error: {message}"))]
    DiskImage { message: String },

    #[snafu(display("Failed to deliver virtio IRQ {irq} at asserted={asserted}"))]
    VirtioInterrupt {
        irq: u32,
        asserted: bool,
        source: kvm_ioctls::Error,
    },

    #[snafu(display("Failed to get dirty page log"))]
    GetDirtyLog { source: kvm_ioctls::Error },
}

/// Dependency order of exact restore effects. Preflight is deliberately not an effect phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotRestorePhase {
    Quiesce,
    Cpuid,
    MemoryTracking,
    MemoryAndKernelState,
    Determinism,
    Scheduler,
    Virtio,
    Serial,
}

const VIRTIO_IRQ_LEVELS: [bool; 2] = [true, false];

fn deliver_virtio_interrupt_with(
    device: &mut VirtioMmioDevice,
    queue_index: usize,
    mut set_irq_line: impl FnMut(u32, bool) -> Result<(), kvm_ioctls::Error>,
) -> Result<(), VmError> {
    let irq = device.irq();
    for asserted in VIRTIO_IRQ_LEVELS {
        if let Err(source) = set_irq_line(irq, asserted) {
            device.record_interrupt_failure(queue_index, irq, asserted);
            return Err(VmError::VirtioInterrupt {
                irq,
                asserted,
                source,
            });
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
//  Serial I/O helpers
// ═══════════════════════════════════════════════════════════════════════

/// UART interrupt fact, committed synchronously at the guest-exit boundary.
#[derive(Default)]
struct SerialTrigger(std::cell::Cell<bool>);

impl vm_superio::Trigger for SerialTrigger {
    type E = io::Error;

    fn trigger(&self) -> Result<(), Self::E> {
        self.0.set(true);
        Ok(())
    }
}

/// A writer that outputs to stdout AND captures bytes in a shared buffer.
///
/// Used as the output sink for the serial port so that serial output is
/// both visible in real time and available for programmatic inspection
/// via [`DeterministicVm::take_serial_output`] and
/// [`DeterministicVm::run_until`].
///
/// The capture buffer is bounded: it retains at most
/// `MAX_SERIAL_CAPTURE_BYTES` of the most recent output. Older bytes are
/// dropped and counted in `dropped_byte_count`.
///
/// stdout receives sanitized bytes (terminal-control bytes replaced).
/// The capture buffer keeps the raw guest bytes for evidence.
#[derive(Clone)]
pub struct CapturingWriter {
    buffer: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    dropped: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl CapturingWriter {
    fn new() -> Self {
        Self {
            buffer: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            dropped: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Take the captured output, clearing the internal buffer.
    ///
    /// Returns at most `MAX_SERIAL_CAPTURE_BYTES` of the most recent
    /// output. The dropped-byte counter is not reset.
    pub fn take(&self) -> Vec<u8> {
        let mut buf = self.buffer.lock().unwrap();
        std::mem::take(&mut *buf)
    }

    /// Get the captured output as a string (lossy UTF-8).
    pub fn as_string(&self) -> String {
        let buf = self.buffer.lock().unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// Total captured bytes dropped since creation because the buffer
    /// reached `MAX_SERIAL_CAPTURE_BYTES`.
    pub fn dropped_byte_count(&self) -> u64 {
        self.dropped.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl io::Write for CapturingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let sanitized = sanitize_serial_for_terminal(buf);
        io::stdout().write_all(&sanitized)?;
        io::stdout().flush()?;
        let dropped = {
            let mut guard = self.buffer.lock().unwrap();
            capture_serial_bounded(&mut guard, buf)
        };
        if dropped > 0 {
            self.dropped
                .fetch_add(dropped as u64, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  DeterministicVm
// ═══════════════════════════════════════════════════════════════════════

/// A deterministic KVM-backed virtual machine.
///
/// All sources of non-determinism are controlled:
/// - CPUID is filtered to hide RDRAND, RDSEED, RDTSCP, etc.
/// - TSC is pinned to a fixed frequency
/// - A virtual TSC counter advances only on VM exits
/// - Serial I/O is captured for deterministic output comparison
///
/// The VM tracks execution statistics (exit counts) for use in
/// deterministic scheduling and progress measurement.
/// One accepted assertion hit with the deterministic VM boundary that observed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionHitRecord {
    pub event: AssertionHitEvent,
    pub exit_count: u64,
    pub virtual_tsc: u64,
    pub vcpu: usize,
}

pub struct DeterministicVm {
    #[allow(dead_code)]
    kvm: Kvm,
    /// Immutable capability facts admitted before activation, never queried during preflight.
    snapshot_msr_indices: Vec<u32>,
    vm: VmFd,
    vcpus: Vec<VcpuFd>,
    /// Index of the currently active vCPU (0 = BSP).
    active_vcpu: usize,
    memory: GuestMemoryManager,
    /// Current memory ceiling exposed through the guest SDK.
    memory_ceiling_bytes: u64,

    // Determinism state
    virtual_tsc: VirtualTsc,
    guest_determinism_profile: GuestDeterminismProfile,
    execution_descriptor: crate::snapshot::SnapshotExecutionDescriptor,
    coarse_clock_filter_identity: Option<[u8; 32]>,
    injection_ledger: crate::snapshot::InjectionLedger,
    injection_records: Vec<crate::snapshot::InjectionRecord>,
    injection_record_limit: usize,

    // Deterministic entropy source (seeded PRNG replacing virtio-rng)
    entropy: DeterministicEntropy,

    // Deterministic timer (mirrors KVM PIT state on virtual TSC timeline)
    pit: DeterministicPit,

    // Serial console
    serial: vm_superio::Serial<SerialTrigger, vm_superio::serial::NoEvents, CapturingWriter>,
    serial_writer: CapturingWriter,

    // KVM PIT mirroring state
    last_kvm_pit_mode: u8,

    // Fault injection engine (SDK hypercall handler + property oracle)
    fault_engine: FaultEngine,

    // Startup-admitted capacity for selected deterministic runtime paths.
    runtime_capacity_plan: RuntimeCapacityPlan,

    // Virtio MMIO devices
    virtio_devices: Vec<VirtioMmioDevice>,

    // Intra-VM vCPU scheduler (only meaningful when num_vcpus > 1)
    scheduler: VcpuScheduler,
    /// Evidence slot held across an unrelated host interrupt retry.
    pending_schedule_reservation: Option<ScheduleReservation>,
    /// Permanent failure after KVM may have progressed without verified evidence.
    schedule_execution_poison: Option<ScheduleExecutionPoison>,
    /// True only between a possibly executing entry and verified completion.
    schedule_entry_in_flight: bool,
    /// HLT observations that KVM leaves encoded as runnable MP state.
    hlt_latched_vcpus: Vec<bool>,
    /// vCPUs made non-runnable by an active fault window.
    forced_stalled_vcpus: std::collections::BTreeSet<usize>,

    /// Optional guest-only PMU overflow accelerator.
    instruction_counter: Option<crate::perf::InstructionCounter>,
    /// Scheduler progress at the most recent PMU reset.
    pmu_counter_base_progress: u64,
    /// Whether KVM guest debug single-step is currently active.
    singlestep_active: bool,
    /// Operational host watchdog interval for one vCPU run.
    watchdog_microseconds: i64,
    /// Consecutive host-watchdog interrupts with no intervening guest exit.
    host_watchdog_retries: u32,

    // Execution statistics
    exit_count: u64,
    /// Rebound by the controller before each round, including after restore.
    diagnostic_controller_tick: Option<u64>,
    io_exit_count: u64,
    /// Total exits since the last SDK hypercall.
    /// When this exceeds an idle threshold AND setup_complete has been
    /// signaled, `step()` treats the VM as halted (workload done).
    exits_since_last_sdk: u64,

    // Coverage tracking
    coverage_active: bool,

    /// Whether VMCALL-based SDK transport is active.
    ///
    /// When `true`, the guest triggers SDK hypercalls via `vmcall`
    /// (`KVM_EXIT_HYPERCALL`). When `false`, falls back to port I/O
    /// (`outb(0x510)` → `KVM_EXIT_IO`).
    vmcall_enabled: bool,

    /// Set when serial output contains "Kernel panic".
    /// Causes `step()` to return halted on the next iteration.
    panic_detected: bool,

    /// Sliding window for serial panic detection.
    /// Accumulates the last 8 serial output bytes as a big-endian u64.
    panic_match_state: u64,

    /// Accepted assertion hits awaiting explorer collection.
    assertion_hits: Vec<AssertionHitRecord>,

    /// Bounded protocol-observation envelopes with VMM-bound scheduler positions.
    protocol_observations: crate::protocol_observation::ProtocolObservationCollector,

    /// Accepted assertion hits dropped because the VM-local queue was full.
    assertion_hit_overflow: u64,
    /// Whether assertion hits request a checkpoint stop at the next boundary.
    assertion_checkpoint_stop: bool,
    /// Maximum checkpoint stops admitted for the current branch.
    assertion_checkpoint_stop_limit: u32,
    /// Checkpoint stops already requested for the current branch.
    assertion_checkpoint_stops: u32,
    /// Set when an eligible hit requires the current bounded run to stop.
    assertion_hit_stop_pending: bool,

    /// Per-thread POSIX timer for SIGALRM delivery.
    /// Created by `init_thread_timer()` for parallel worker threads.
    /// When `Some`, `arm_preemption_timer()` uses this instead of
    /// the process-wide `ITIMER_REAL`.
    ///
    /// Wrapped in `SendTimerId` because `timer_t` is `*mut c_void`
    /// which is `!Send`. The timer is only used from the thread that
    /// created it, so this is safe.
    thread_timer: Option<SendTimerId>,
    thread_timer_tid: Option<i32>,

    /// Extra kernel command line parameters (from VmConfig).
    extra_cmdline: Option<String>,

    /// Whether KVM dirty page logging is enabled on the memory slot.
    dirty_log_enabled: bool,

    /// VM identifier for multi-VM networking.
    vm_id: usize,

    /// Determinism log writer — records every VM exit when enabled.
    dlog: Option<DlogWriter>,
    dlog_register_interval: u64,
    dlog_memory_hash: bool,

    /// Bitmap of restored overlay pages and guest writes retained since the base.
    /// KVM dirty-log reads are destructive, so captures must keep their union.
    pages_differing_from_base: Vec<u64>,
    /// A full image needs comparison with the next supplied incremental base.
    incremental_base_needs_reconcile: bool,
}

/// Kernel images are ELF files, not guest-memory residents; the loader
/// validates every segment against guest memory. Bound the file itself so a
/// kernel larger than the guest memory ceiling can still boot.
const MAX_KERNEL_FILE_BYTES: u64 = 1 << 30;

fn open_boot_file(path: &std::path::Path, limit: u64) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "boot file is not a bounded regular file",
        ));
    }
    Ok(file)
}

#[cfg(test)]
fn read_boot_file(path: &std::path::Path, limit: u64) -> io::Result<Vec<u8>> {
    read_boot_handle(&open_boot_file(path, limit)?, limit)
}

fn read_boot_handle(file: &File, limit: u64) -> io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "boot file capability is not a bounded regular file",
        ));
    }
    let mut file = file.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let expected = file.metadata()?.len();
    let capacity = usize::try_from(expected)
        .unwrap_or(0)
        .min(usize::try_from(limit).unwrap_or(usize::MAX));
    let mut bytes = Vec::with_capacity(capacity);
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "boot file exceeds the byte limit",
        ));
    }
    if bytes.len() as u64 != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "boot file changed while reading",
        ));
    }
    Ok(bytes)
}

fn checked_guest_range_end(addr: u64, size: usize, memory_size: usize) -> Option<u64> {
    let memory_size = u64::try_from(memory_size).ok()?;
    let size = u64::try_from(size).ok()?;
    let end = addr.checked_add(size)?;
    (addr <= memory_size && end <= memory_size).then_some(end)
}

impl DeterministicVm {
    /// Pin the current thread to a specific physical CPU core.
    ///
    /// Uses `sched_setaffinity(2)` to restrict the thread to a single
    /// core. This matches the Antithesis approach: "each instance of the
    /// deterministic hypervisor runs on just one physical CPU core."
    ///
    /// Benefits:
    /// - Eliminates context switch jitter from the host scheduler
    /// - Prevents cache eviction from core migration
    /// - Ensures consistent PMC behavior (counters are per-core)
    /// - Avoids NUMA latency variation
    fn pin_to_core(core: usize) {
        // SAFETY: We're setting CPU affinity for the current thread.
        // cpu_set_t is zeroed first, then a single bit is set.
        unsafe {
            let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
            libc::CPU_ZERO(&mut cpuset);
            libc::CPU_SET(core, &mut cpuset);
            let ret = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &cpuset);
            if ret != 0 {
                log::warn!(
                    "Failed to pin to core {}: errno {}. Continuing without affinity.",
                    core,
                    *libc::__errno_location(),
                );
            }
        }
    }

    fn mp_state_is_runnable(mp_state: u32) -> bool {
        mp_state == KVM_MP_STATE_RUNNABLE
    }

    fn vcpu_fd_is_runnable(vcpu: usize, fd: &VcpuFd) -> Result<bool, VmError> {
        let state = fd.get_mp_state().map_err(|error| VmError::Snapshot {
            message: format!("failed to read vCPU {vcpu} MP state: {error}"),
        })?;
        Ok(Self::mp_state_is_runnable(state.mp_state))
    }

    fn set_vcpu_singlestep(fd: &VcpuFd, enabled: bool) -> Result<(), kvm_ioctls::Error> {
        let control = if enabled {
            KVM_GUESTDBG_ENABLE | KVM_GUESTDBG_SINGLESTEP
        } else {
            0
        };
        fd.set_guest_debug(&kvm_guest_debug {
            control,
            pad: 0,
            arch: Default::default(),
        })
    }

    // r[impl chaoscontrol.event_entry_exact.admission]
    // KVM_SET_GUEST_DEBUG acceptance is not a capability for exact retirement
    // through event entry. Portable KVM has no such backend guarantee. In
    // particular, rearming TF before RUN cannot repair TF lost inside IRQ/NMI
    // or synchronous exception entry; PMU exact remainder needs the same primitive.
    pub(crate) fn validate_production_progress(config: &VmConfig) -> Result<(), VmError> {
        if config.smp_progress_mode == ProgressMode::KvmDebugBoundary {
            if config.num_vcpus < 2
                || !matches!(
                    config.smp_progress_budget,
                    ProgressBudget::KvmBoundaries {
                        profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
                        count: 1..,
                    }
                )
                || config.smp_entry_watchdog_microseconds == 0
                || config.smp_entry_watchdog_microseconds > (i64::MAX / 1000) as u64
            {
                return Err(VmError::ProgressCapability {
                    message: "coarse SMP requires >=2 vCPUs, an explicit v1 boundary budget and a finite positive entry watchdog".into(),
                });
            }
            coarse_configuration::validate_coarse_clock_configuration(config)?;
        } else if !matches!(config.smp_progress_budget, ProgressBudget::Instructions(_)) {
            return Err(VmError::ProgressCapability {
                message: "strict progress requires instruction units".into(),
            });
        }
        if config.num_vcpus > 1 {
            validate_progress_capabilities(
                config.smp_progress_mode,
                ProgressCapabilities {
                    event_entry_exact_single_step: false,
                    guest_instruction_pmu: false,
                    pmu_overflow: false,
                    kvm_debug_boundary: true,
                    bounded_kvm_entry: true,
                },
            )?;
        }
        Ok(())
    }

    /// Install the signal-safe host recovery handler; it grants no progress units.
    /// Deterministic SMP does not use this signal as a progress source.
    fn install_sigalrm_handler() -> Result<(), VmError> {
        extern "C" fn noop_signal_handler(_sig: libc::c_int) {}
        static INSTALL: std::sync::LazyLock<Result<(), i32>> = std::sync::LazyLock::new(|| {
            // SAFETY: initialized sigaction and a signal-safe no-op handler;
            // SA_RESTART is deliberately absent so KVM_RUN returns control.
            unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = noop_signal_handler as *const () as usize;
                if libc::sigaction(libc::SIGALRM, &sa, std::ptr::null_mut()) != 0 {
                    Err(io::Error::last_os_error()
                        .raw_os_error()
                        .unwrap_or(libc::EINVAL))
                } else {
                    Ok(())
                }
            }
        });
        (*INSTALL).map_err(|errno| VmError::ProgressCapability {
            message: format!(
                "cannot install KVM recovery signal handler: {}",
                io::Error::from_raw_os_error(errno)
            ),
        })?;
        let mut current = std::mem::MaybeUninit::<libc::sigaction>::uninit();
        // SAFETY: query-only sigaction initializes the output on success.
        if unsafe { libc::sigaction(libc::SIGALRM, std::ptr::null(), current.as_mut_ptr()) } != 0
            || unsafe { current.assume_init_ref().sa_sigaction }
                != noop_signal_handler as *const () as usize
        {
            return Err(VmError::ProgressCapability {
                message: "KVM recovery signal handler was replaced or cannot be inspected".into(),
            });
        }
        Ok(())
    }

    /// Create a new deterministic VM with the given configuration.
    ///
    /// This sets up KVM, guest memory, IRQ chip, PIT, and the serial
    /// console. The VM is ready for [`load_kernel`](Self::load_kernel)
    /// after construction.
    pub fn new(config: VmConfig) -> Result<Self, VmError> {
        Self::new_with_disk_image(config, None)
    }

    pub(crate) fn new_with_disk_image(
        mut config: VmConfig,
        disk_image: Option<std::sync::Arc<Vec<u8>>>,
    ) -> Result<Self, VmError> {
        // r[impl chaoscontrol.event_entry_exact.early]
        // Reject before allocations, KVM creation, disk/log setup or guest loading.
        Self::validate_production_progress(&config)?;
        let execution_descriptor =
            crate::snapshot::SnapshotExecutionDescriptor::capture_host(&config)
                .map_err(|message| VmError::Snapshot { message })?;
        let runtime_capacity_limits = runtime_capacity_limits(&config)?;
        let runtime_capacity_plan =
            plan_runtime_capacity(&runtime_capacity_limits).map_err(|error| {
                VmError::RuntimeCapacity {
                    message: format!("{error:?}"),
                }
            })?;
        let kvm = Kvm::new().context(KvmCreateSnafu)?;
        if config.smp_progress_mode == ProgressMode::KvmDebugBoundary
            && kvm.check_extension_raw(kvm_bindings::KVM_CAP_X86_MSR_FILTER as libc::c_ulong) <= 0
        {
            return Err(VmError::ProgressCapability {
                message: "coarse clock profile requires enforced KVM_CAP_X86_MSR_FILTER".into(),
            });
        }
        if config.smp_progress_mode == ProgressMode::KvmDebugBoundary
            && !kvm.check_extension(Cap::ImmediateExit)
        {
            return Err(VmError::ProgressCapability {
                message: "coarse I/O settlement requires KVM_CAP_IMMEDIATE_EXIT".into(),
            });
        }
        let mut snapshot_msr_indices = kvm
            .get_msr_index_list()
            .map_err(|error| VmError::Snapshot {
                message: format!("cannot retain the KVM MSR capability inventory: {error}"),
            })?
            .as_slice()
            .to_vec();
        snapshot_msr_indices.sort_unstable();
        snapshot_msr_indices.dedup();
        if snapshot_msr_indices.is_empty() {
            return Err(VmError::Snapshot {
                message: "KVM returned an empty MSR capability inventory".to_string(),
            });
        }
        let vm = kvm.create_vm().context(VmCreateSnafu)?;
        let coarse_clock_filter_identity =
            if config.smp_progress_mode == ProgressMode::KvmDebugBoundary {
                Some(Self::install_coarse_clock_msr_filter(&vm)?)
            } else {
                None
            };

        // Create guest memory
        let memory = GuestMemoryManager::new(config.memory_size)?;
        let memory_ceiling_bytes = u64::try_from(memory.size()).map_err(|_| VmError::Snapshot {
            message: "guest memory size exceeds resource-observation bounds".to_string(),
        })?;

        // Register guest memory with KVM.
        // Enable dirty page logging unconditionally — the hardware tracks
        // dirty bits via EPT/NPT as a side effect of address translation,
        // so there's no runtime cost when we don't query. When we do
        // query via KVM_GET_DIRTY_LOG, we get a bitmap of pages the guest
        // has written since the last query.
        let mem_region = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0,
            memory_size: config.memory_size as u64,
            userspace_addr: memory.host_address(),
            flags: KVM_MEM_LOG_DIRTY_PAGES,
        };
        unsafe {
            vm.set_user_memory_region(mem_region)
                .context(SetUserMemoryRegionSnafu)?;
        }

        // Set TSS address — MUST be before create_irq_chip on x86_64
        vm.set_tss_address(KVM_TSS_ADDRESS)
            .context(CreateIrqChipSnafu)?;

        // Create in-kernel IRQ chip (PIC, IOAPIC, LAPIC) — MUST be before create_vcpu
        vm.create_irq_chip().context(CreateIrqChipSnafu)?;

        // Inhibited KVM PITs still consume guest PIO and expose host-clock
        // channel/speaker reads. Coarse execution must leave that I/O bus
        // device absent: every PIT access exits to DeterministicPit instead.
        if config.smp_progress_mode != ProgressMode::KvmDebugBoundary {
            let pit_config = kvm_pit_config {
                flags: KVM_PIT_SPEAKER_DUMMY,
                ..Default::default()
            };
            vm.create_pit2(pit_config).context(CreatePitSnafu)?;
            let mut pit_state = vm.get_pit2().context(CreatePitSnafu)?;
            pit_state.flags |= KVM_PIT_FLAGS_HPET_LEGACY;
            // Preserve KVM's unprogrammed mode (0xff). Installing mode 0
            // would unnecessarily request a maximum-count host one-shot.
            vm.set_pit2(&pit_state).context(CreatePitSnafu)?;
        }

        // VMCALL is outside the coarse profile's closed exit table. Its SDK
        // transport is explicitly PIO, not an advertised unsupported hypercall.
        // Other modes retain the host-admitted VMCALL transport when available.
        let vmcall_enabled = if config.smp_progress_mode != ProgressMode::KvmDebugBoundary
            && kvm.check_extension(Cap::ExitHypercall)
        {
            let cap = kvm_enable_cap {
                cap: Cap::ExitHypercall as u32,
                args: [1u64 << VMCALL_NR, 0, 0, 0],
                ..Default::default()
            };
            match vm.enable_cap(&cap) {
                Ok(_) => {
                    info!(
                        "VMCALL transport enabled (KVM_CAP_EXIT_HYPERCALL, nr={})",
                        VMCALL_NR
                    );
                    true
                }
                Err(e) => {
                    info!(
                        "VMCALL transport unavailable (enable_cap failed: {}), using port I/O",
                        e
                    );
                    false
                }
            }
        } else {
            info!("VMCALL transport unavailable (no KVM_CAP_EXIT_HYPERCALL), using port I/O");
            false
        };

        // DETERMINISM: Set KVM clock to zero so guest always sees the same
        // starting time. Without this, the guest reads host wall-clock time
        // via the KVM paravirt clock MSRs, breaking reproducibility.
        let clock_data = kvm_clock_data {
            clock: 0,
            ..Default::default()
        };
        vm.set_clock(&clock_data).context(SetClockSnafu)?;
        info!("KVM clock set to 0 (deterministic)");

        // Create vCPUs AFTER irqchip (so each gets an in-kernel LAPIC).
        // Only one vCPU runs at a time — deterministic serialized scheduling.
        let num_vcpus =
            plan_vcpu_construction(config.num_vcpus, MAX_SCHEDULE_VCPUS).map_err(|error| {
                ScheduleError::TooManyVcpus {
                    found: error.found,
                    limit: error.limit,
                }
            })?;
        // `notsc` marks Linux's TSC unstable, but does not clear its TSC
        // capability: the IOAPIC timer check would still busy-wait on RDTSC.
        // KVM's legacy IA32_TSC write synchronization can retain a host-time
        // offset, so such a wait can end before any virtual PIT tick. SMP uses
        // the instruction-driven PIT/jiffies clock, including this early check.
        config.cpu = config.effective_cpu_config();

        // Use the exact measured source table retained in the descriptor. A
        // second GET_SUPPORTED_CPUID can observe another host CPU's cache data.
        let cpuid = execution_descriptor
            .guest_cpuid_template()
            .map_err(|message| VmError::Snapshot { message })?;
        let mut vcpus = Vec::with_capacity(num_vcpus);
        for i in 0..num_vcpus {
            let vcpu = vm.create_vcpu(i as u64).context(VcpuCreateSnafu)?;
            // Each vCPU needs its own CPUID table with its unique APIC ID
            // in leaf 0x1 EBX[31:24] and leaf 0xB/0x1F EDX. Without this,
            // all vCPUs report APIC ID 0, causing "APIC ID mismatch"
            // firmware bug warnings from the kernel.
            let vcpu_cpuid = cpu::patch_cpuid_apic_id(&cpuid, i as u32, num_vcpus as u32)?;
            vcpu.set_cpuid2(&vcpu_cpuid).context(cpu::SetCpuidSnafu)?;
            cpu::setup_tsc(&vcpu, config.cpu.tsc_khz)?;
            vcpus.push(vcpu);
        }
        info!("Created {} vCPU(s)", num_vcpus);

        let profile_input = GuestDeterminismInput {
            run_seed: config.cpu.seed,
            vm_id: u32::try_from(config.vm_id).map_err(|_| VmError::RuntimeCapacity {
                message: "VM identifier exceeds the guest determinism profile bound".to_string(),
            })?,
            vcpu_count: u32::try_from(num_vcpus).map_err(|_| VmError::RuntimeCapacity {
                message: "vCPU count exceeds the guest determinism profile bound".to_string(),
            })?,
            tsc_khz: config.cpu.tsc_khz,
            clock_mode: if config.cpu.hide_tsc {
                GuestClockMode::DeterministicJiffies
            } else {
                GuestClockMode::VirtualTsc
            },
        };
        let guest_determinism_profile = build_guest_determinism_profile(profile_input);

        // Create virtual TSC for deterministic time tracking
        let virtual_tsc = VirtualTsc::from_config(&config.cpu);

        // Create deterministic entropy source seeded from master seed
        let entropy = DeterministicEntropy::new(config.cpu.seed);

        // Deterministic PIT driven by virtual TSC — delivers timer
        // interrupts at exact virtual-time points via set_irq_line.
        let pit = DeterministicPit::new(config.cpu.tsc_khz);

        // UART IRQ delivery must not depend on host irqfd work scheduling.
        let serial_trigger = SerialTrigger::default();
        let serial_writer = CapturingWriter::new();
        let serial = vm_superio::Serial::new(serial_trigger, serial_writer.clone());

        // Define the deterministic instruction schedule before any guest runs.
        let scheduler_config = SchedulerConfig {
            num_vcpus,
            progress_budget: config.smp_progress_budget,
            strategy: config.scheduling_strategy,
            seed: config.cpu.seed,
        };

        // Create fault injection engine for SDK hypercalls
        let fault_engine = FaultEngine::new(EngineConfig {
            seed: config.cpu.seed,
            num_vms: 1,
            ..EngineConfig::default()
        });

        // Create virtio MMIO devices
        let virtio_devices = Self::create_virtio_devices(
            config.cpu.seed,
            config.disk_image_path.as_deref(),
            config.vm_id,
            disk_image,
        )?;

        // Open determinism log writer if requested.
        let dlog = match &config.dlog_path {
            Some(path) => {
                let w = DlogWriter::create(path).map_err(|e| VmError::DiskImage {
                    message: format!("dlog create {}: {e}", path.display()),
                })?;
                info!("Determinism log: {}", path.display());
                Some(w)
            }
            None => None,
        };

        // Pin VM thread to a specific physical CPU core if requested.
        // This eliminates host scheduler jitter and ensures consistent
        // PMC behavior. Antithesis pins each VM to a dedicated core.
        if let Some(core) = config.core_affinity {
            Self::pin_to_core(core);
            info!("VM thread pinned to core {}", core);
        }

        info!(
            "VM created: {} MB memory, {} vCPU(s), TSC {} kHz, seed {}, {} virtio devices{}",
            config.memory_size / (1024 * 1024),
            num_vcpus,
            config.cpu.tsc_khz,
            config.cpu.seed,
            virtio_devices.len(),
            config
                .core_affinity
                .map(|c| format!(", pinned to core {c}"))
                .unwrap_or_default(),
        );

        let initial_runnable = vcpus
            .iter()
            .enumerate()
            .map(|(vcpu, fd)| Self::vcpu_fd_is_runnable(vcpu, fd))
            .collect::<Result<Vec<_>, _>>()?;
        let progress_mode = if num_vcpus > 1 {
            config.smp_progress_mode
        } else {
            ProgressMode::ExactSingleStep
        };
        let scheduler = VcpuScheduler::try_new_with_journal_limit(
            &scheduler_config,
            progress_mode,
            initial_runnable,
            config.smp_schedule_journal_limit,
        )?;
        let active_vcpu = scheduler.active();

        // Portable KVM cannot currently admit strict SMP. Keep the live PMU
        // owner absent; debug-control and PMU probes cannot confer exactness.
        let instruction_counter = None;

        Ok(Self {
            kvm,
            snapshot_msr_indices,
            vm,
            vcpus,
            active_vcpu,
            memory,
            memory_ceiling_bytes,
            virtual_tsc,
            guest_determinism_profile,
            execution_descriptor,
            coarse_clock_filter_identity,
            injection_ledger: crate::snapshot::InjectionLedger::new(),
            injection_records: Vec::new(),
            injection_record_limit: config.smp_schedule_journal_limit,
            entropy,
            pit,
            serial,
            serial_writer,
            scheduler,
            pending_schedule_reservation: None,
            schedule_execution_poison: None,
            schedule_entry_in_flight: false,
            hlt_latched_vcpus: vec![false; num_vcpus],
            forced_stalled_vcpus: std::collections::BTreeSet::new(),
            fault_engine,
            runtime_capacity_plan,
            virtio_devices,
            instruction_counter,
            pmu_counter_base_progress: 0,
            singlestep_active: false,
            watchdog_microseconds: if config.smp_progress_mode == ProgressMode::KvmDebugBoundary {
                config.smp_entry_watchdog_microseconds as i64
            } else {
                std::env::var("CHAOSCONTROL_WATCHDOG_US")
                    .ok()
                    .and_then(|value| value.parse::<i64>().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(SINGLE_VCPU_WATCHDOG_MICROSECONDS)
            },
            host_watchdog_retries: 0,
            last_kvm_pit_mode: 0xFF, // impossible value forces first sync
            exit_count: 0,
            diagnostic_controller_tick: None,
            io_exit_count: 0,
            exits_since_last_sdk: 0,
            panic_detected: false,
            panic_match_state: 0,
            assertion_hits: Vec::new(),
            protocol_observations:
                crate::protocol_observation::ProtocolObservationCollector::default(),
            assertion_hit_overflow: 0,
            assertion_checkpoint_stop: false,
            assertion_checkpoint_stop_limit: 0,
            assertion_checkpoint_stops: 0,
            assertion_hit_stop_pending: false,
            thread_timer: None,
            thread_timer_tid: None,
            coverage_active: false,
            vmcall_enabled,
            extra_cmdline: config.extra_cmdline.clone(),
            dirty_log_enabled: true,
            vm_id: config.vm_id,
            dlog,
            dlog_register_interval: config.dlog_register_interval,
            dlog_memory_hash: config.dlog_memory_hash,
            pages_differing_from_base: Vec::new(),
            incremental_base_needs_reconcile: false,
        })
    }

    pub fn runtime_capacity_plan(&self) -> &RuntimeCapacityPlan {
        &self.runtime_capacity_plan
    }

    pub fn runtime_capacity_observations(&self) -> Result<RuntimeCapacityObservations, VmError> {
        let mut scratch_pools = Vec::with_capacity(VIRTIO_SCRATCH_DEVICE_COUNT);
        let mut network = None;
        for device in &self.virtio_devices {
            if let Some(block) = device
                .backend()
                .as_any()
                .downcast_ref::<crate::devices::virtio_block::VirtioBlock>()
            {
                scratch_pools.push(capacity_usage(block.scratch_capacity_observations()));
            } else if let Some(net) = device
                .backend()
                .as_any()
                .downcast_ref::<crate::devices::virtio_net::VirtioNet>()
            {
                scratch_pools.push(capacity_usage(net.scratch_capacity_observations()));
                network = Some(net.net().capacity_observations());
            } else if let Some(entropy) = device
                .backend()
                .as_any()
                .downcast_ref::<crate::devices::virtio_entropy::VirtioEntropy>(
            ) {
                scratch_pools.push(capacity_usage(entropy.scratch_capacity_observations()));
            }
        }
        if scratch_pools.len() != VIRTIO_SCRATCH_DEVICE_COUNT {
            return Err(VmError::RuntimeCapacity {
                message: "live virtio scratch-pool inventory is incomplete".to_string(),
            });
        }
        let network = network.ok_or_else(|| VmError::RuntimeCapacity {
            message: "live network packet-pool inventory is missing".to_string(),
        })?;
        let leaked_scratch_slots = scratch_pools.iter().try_fold(0usize, |total, pool| {
            total
                .checked_add(pool.in_use)
                .ok_or_else(|| VmError::RuntimeCapacity {
                    message: "scratch leak counter overflow".to_string(),
                })
        })?;
        let packet_allocated_bytes = network
            .allocated_slots
            .checked_mul(network.slot_bytes)
            .ok_or_else(|| VmError::RuntimeCapacity {
                message: "packet allocation observation overflow".to_string(),
            })?;
        let retained_packet_bytes =
            usize::try_from(network.retained_bytes).map_err(|_| VmError::RuntimeCapacity {
                message: "retained packet observation does not fit usize".to_string(),
            })?;
        Ok(RuntimeCapacityObservations {
            schema: RUNTIME_CAPACITY_OBSERVATIONS_SCHEMA.to_string(),
            plan: self.runtime_capacity_plan.clone(),
            startup_result: RuntimeCapacityStartupResult::Admitted,
            scratch_pools,
            packet_pool: CapacityUsageObservation {
                allocated_slots: network.allocated_slots,
                allocated_bytes: packet_allocated_bytes,
                in_use: network.in_use,
                high_water: network.high_water,
                exhaustion_count: network.exhaustion_count,
                release_count: network.release_count,
            },
            queue_metadata_slots: network.queue_metadata_slots,
            queue_metadata_high_water: network.high_water,
            retained_packet_bytes,
            leaked_scratch_slots,
            leaked_packet_slots: network.in_use,
            claims: RuntimeCapacityClaims::default(),
        })
    }

    /// Create the virtio MMIO devices (block, net, entropy).
    ///
    /// If `disk_image_path` is `Some`, the block device is initialized
    /// from that file (read once, then copy-on-write for snapshots).
    /// Otherwise a zero-filled 16 MB disk is created.
    ///
    /// The `vm_id` is used to generate a unique MAC address for the network device.
    fn create_virtio_devices(
        seed: u64,
        disk_image_path: Option<&str>,
        vm_id: usize,
        disk_image: Option<std::sync::Arc<Vec<u8>>>,
    ) -> Result<Vec<VirtioMmioDevice>, VmError> {
        use crate::devices::block::DeterministicBlock;
        use crate::devices::net::DeterministicNet;
        use crate::devices::virtio_block::VirtioBlock;
        use crate::devices::virtio_entropy::VirtioEntropy;
        use crate::devices::virtio_net::VirtioNet;

        let mut devices = Vec::new();

        // Device 0: virtio-blk
        let disk = if let Some(image) = disk_image {
            DeterministicBlock::from_shared_image(image)
        } else {
            match disk_image_path {
                Some(path) => {
                    info!("Loading disk image: {}", path);
                    DeterministicBlock::from_image_file(path).map_err(|e| VmError::DiskImage {
                        message: e.to_string(),
                    })?
                }
                None => DeterministicBlock::new(DEFAULT_EMPTY_DISK_BYTES),
            }
        };
        info!(
            "  Block device: {} bytes ({} MB)",
            disk.size(),
            disk.size() / BYTES_PER_MIB_U64
        );
        let blk_backend =
            Box::new(
                VirtioBlock::try_new(disk).map_err(|error| VmError::RuntimeCapacity {
                    message: format!("virtio block scratch startup failed: {error:?}"),
                })?,
            );
        let blk_device = VirtioMmioDevice::new(VIRTIO_MMIO_BASE_0, VIRTIO_MMIO_IRQ_0, blk_backend);
        devices.push(blk_device);

        // Device 1: virtio-net (unique MAC per VM)
        let mac = [0x52, 0x54, 0x00, 0x12, 0x34, vm_id as u8];
        let packet_slot_bytes = usize::try_from(
            crate::devices::virtio_types::DEFAULT_MAX_NET_FRAME_BYTES,
        )
        .map_err(|_| VmError::RuntimeCapacity {
            message: "default network frame limit does not fit usize".to_string(),
        })?;
        let net = DeterministicNet::try_new(
            mac,
            crate::devices::virtio_types::DEFAULT_MAX_NET_TX_PACKETS,
            packet_slot_bytes,
        )
        .map_err(|error| VmError::RuntimeCapacity {
            message: format!("network packet startup allocation failed: {error:?}"),
        })?;
        let net_backend =
            Box::new(
                VirtioNet::try_new(net).map_err(|error| VmError::RuntimeCapacity {
                    message: format!("virtio network scratch startup failed: {error:?}"),
                })?,
            );
        let net_device = VirtioMmioDevice::new(VIRTIO_MMIO_BASE_1, VIRTIO_MMIO_IRQ_1, net_backend);
        devices.push(net_device);

        // Device 2: virtio-rng
        let entropy = DeterministicEntropy::new(seed);
        let rng_backend = Box::new(VirtioEntropy::try_new(entropy).map_err(|error| {
            VmError::RuntimeCapacity {
                message: format!("virtio entropy scratch startup failed: {error:?}"),
            }
        })?);
        let rng_device = VirtioMmioDevice::new(VIRTIO_MMIO_BASE_2, VIRTIO_MMIO_IRQ_2, rng_backend);
        devices.push(rng_device);

        info!("Created virtio MMIO devices:");
        info!(
            "  Device 0: virtio-blk  @ {:#010x} IRQ {}",
            VIRTIO_MMIO_BASE_0, VIRTIO_MMIO_IRQ_0
        );
        info!(
            "  Device 1: virtio-net  @ {:#010x} IRQ {} MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            VIRTIO_MMIO_BASE_1, VIRTIO_MMIO_IRQ_1,
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );
        info!(
            "  Device 2: virtio-rng  @ {:#010x} IRQ {}",
            VIRTIO_MMIO_BASE_2, VIRTIO_MMIO_IRQ_2
        );

        Ok(devices)
    }

    /// Build the kernel command line dynamically based on VM configuration.
    ///
    /// For single-vCPU mode: includes `nosmp noapic`.
    /// For multi-vCPU mode: includes `maxcpus=N` and omits `nosmp noapic`.
    /// Always includes `vm_id=N` for multi-VM networking identification.
    fn build_cmdline(&self, vm_id: usize) -> Vec<u8> {
        let num_vcpus = self.vcpus.len();
        let use_jiffies = matches!(
            self.guest_determinism_profile.input.clock_mode,
            GuestClockMode::DeterministicJiffies
        );
        let (smp_params, clock_params) = if num_vcpus > 1 {
            // SMP: use jiffies clocksource (driven by deterministic PIT).
            // notsc disables TSC entirely — no TSC calibration (which reads
            // hardware TSC + PIT, producing non-deterministic results).
            (
                format!("maxcpus={num_vcpus}"),
                "clocksource=jiffies notsc".to_string(),
            )
        } else if use_jiffies {
            (
                "nosmp noapic".to_string(),
                "clocksource=jiffies notsc".to_string(),
            )
        } else {
            (
                "nosmp noapic".to_string(),
                "clocksource=tsc tsc=reliable".to_string(),
            )
        };

        let extra = self.extra_cmdline.as_deref().unwrap_or("");

        let cmdline = format!(
            "console=ttyS0 earlyprintk=serial \
             {clock_params} \
             lpj=6000000 \
             nokaslr {smp_params} \
             nohpet \
             randomize_kstack_offset=off norandmaps \
             random.trust_cpu=off random.trust_bootloader=off \
             kfence.sample_interval=0 \
             no_hash_pointers \
             virtio_mmio.device=4K@0xd0000000:5 \
             virtio_mmio.device=4K@0xd0001000:6 \
             virtio_mmio.device=4K@0xd0002000:7 \
             vm_id={vm_id} \
             {extra} \
             panic=0\0"
        );

        cmdline.into_bytes()
    }

    /// Load a Linux kernel (and optional initrd) into guest memory.
    ///
    /// This sets up:
    /// - Kernel loaded at HIMEM_START (1 MB)
    /// - Optional initrd placed after the kernel (page-aligned)
    /// - Boot parameters (zero page) with E820 memory map
    /// - GDT, page tables, segment registers for 64-bit mode
    /// - General-purpose registers with entry point and stack pointer
    /// - ACPI tables (RSDP/RSDT/MADT) when `num_vcpus > 1`
    pub fn load_kernel(
        &mut self,
        kernel_path: &str,
        initrd_path: Option<&str>,
    ) -> Result<(), VmError> {
        info!("Loading kernel from {}", kernel_path);

        let kernel_file = open_boot_file(std::path::Path::new(kernel_path), MAX_KERNEL_FILE_BYTES)?;
        let initrd_file = initrd_path
            .map(|path| open_boot_file(std::path::Path::new(path), self.memory_ceiling_bytes))
            .transpose()?;
        self.load_kernel_from_files(&kernel_file, initrd_file.as_ref())
    }

    /// Load already-admitted bounded regular file capabilities without reopening
    /// them through paths. The ordinary path loader retains O_NOFOLLOW policy.
    pub fn load_kernel_from_files(
        &mut self,
        kernel: &File,
        initrd: Option<&File>,
    ) -> Result<(), VmError> {
        use std::io::{Seek, SeekFrom};
        let metadata = kernel.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_KERNEL_FILE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "kernel capability is not a bounded regular file",
            )
            .into());
        }
        let mut kernel_file = kernel.try_clone()?;
        kernel_file.seek(SeekFrom::Start(0))?;

        // Load kernel using linux-loader
        let kernel_load_result = Elf::load(
            self.memory.inner(),
            None,
            &mut kernel_file,
            Some(GuestAddress(HIMEM_START)),
        )
        .context(KernelLoadSnafu)?;

        let entry_point = kernel_load_result.kernel_load;
        let kernel_end = kernel_load_result.kernel_end;
        info!(
            "Kernel entry point: {:#x}, end: {:#x}",
            entry_point.raw_value(),
            kernel_end,
        );

        // Load initrd if provided (place it after the kernel, page-aligned)
        let initrd_info = if let Some(initrd) = initrd {
            let initrd_data = read_boot_handle(initrd, self.memory_ceiling_bytes)?;
            let initrd_addr = (kernel_end + 4095) & !4095;
            self.memory
                .inner()
                .write_slice(&initrd_data, GuestAddress(initrd_addr))
                .map_err(|_| GuestMemoryWriteSnafu.build())?;
            info!(
                "Initrd loaded at {:#x}, size: {} bytes",
                initrd_addr,
                initrd_data.len(),
            );
            Some((initrd_addr, initrd_data.len() as u64))
        } else {
            None
        };

        // Write boot data structures using memory module
        self.memory.setup_page_tables()?;
        self.memory.setup_gdt()?;

        // Set up boot parameters (zero page)
        self.setup_boot_params(initrd_info)?;

        // Set up x86_64 registers for BSP (vCPU 0)
        self.setup_sregs()?;
        self.setup_regs(entry_point)?;
        self.setup_fpu()?;

        // Write ACPI tables for SMP when num_vcpus > 1
        if self.vcpus.len() > 1 {
            acpi::write_acpi_tables(self.memory.inner(), self.vcpus.len()).map_err(|e| {
                SnapshotSnafu {
                    message: format!("ACPI table generation: {e}"),
                }
                .build()
            })?;
            info!("ACPI tables written for {} vCPUs", self.vcpus.len());
        }

        // Write the SDK transport mode to the hypercall page so the
        // guest SDK knows whether to use vmcall or port I/O.
        let transport = if self.vmcall_enabled {
            chaoscontrol_protocol::TRANSPORT_VMCALL
        } else {
            chaoscontrol_protocol::TRANSPORT_PORT_IO
        };
        self.memory
            .inner()
            .write_slice(
                &[transport],
                GuestAddress(HYPERCALL_PAGE_ADDR + chaoscontrol_protocol::TRANSPORT_MODE_OFFSET),
            )
            .map_err(|_| GuestMemoryWriteSnafu.build())?;
        info!(
            "SDK transport: {} (written to hypercall page)",
            if self.vmcall_enabled {
                "vmcall"
            } else {
                "port I/O"
            }
        );

        Ok(())
    }

    /// Return the exact guest operating-system determinism profile.
    #[must_use]
    pub fn guest_determinism_profile(&self) -> &GuestDeterminismProfile {
        &self.guest_determinism_profile
    }

    /// Reset the vCPU's TSC to 0 via MSR write.
    fn reset_tsc_to_zero(&self) -> Result<(), VmError> {
        self.write_tsc_to_guest(0)
    }

    /// Write a specific TSC value to the active vCPU's IA32_TSC MSR.
    ///
    /// KVM advances the guest-visible TSC based on real wall-clock time
    /// between VM entries and exits. By writing our virtual TSC value
    /// before every `vcpu.run()`, we ensure RDTSC always starts from a
    /// deterministic value, eliminating jitter from variable exit counts
    /// caused by host interrupts and serial polling.
    fn write_tsc_to_guest(&self, value: u64) -> Result<(), VmError> {
        use kvm_bindings::{kvm_msr_entry, Msrs};

        const MSR_IA32_TSC: u32 = 0x10;

        let msrs = Msrs::from_entries(&[kvm_msr_entry {
            index: MSR_IA32_TSC,
            data: value,
            ..Default::default()
        }])
        .map_err(|_| GuestMemoryWriteSnafu.build())?;

        let written = self.vcpus[self.active_vcpu]
            .set_msrs(&msrs)
            .context(SetRegistersSnafu)?;
        if self.coarse_mode() && written != 1 {
            return Err(VmError::ProgressCapability {
                message: "KVM did not overwrite the per-entry guest TSC".into(),
            });
        }
        Ok(())
    }

    /// Sync the virtual TSC to the guest vCPU before each run.
    ///
    /// This is the critical determinism fix: it overwrites KVM's
    /// real-time TSC drift with our deterministic counter so every
    /// guest execution slice starts at an exact, reproducible value.
    fn sync_tsc_to_guest(&self) -> Result<(), VmError> {
        self.write_tsc_to_guest(self.virtual_tsc.guest_read())
    }

    fn setup_boot_params(&self, initrd_info: Option<(u64, u64)>) -> Result<(), VmError> {
        const KERNEL_BOOT_FLAG_MAGIC: u16 = 0xaa55;
        const KERNEL_HDR_MAGIC: u32 = 0x5372_6448;
        const KERNEL_LOADER_OTHER: u8 = 0xff;
        const KERNEL_MIN_ALIGNMENT_BYTES: u32 = 0x0100_0000;

        // Write kernel command line (dynamic based on num_vcpus and vm_id)
        let cmdline = self.build_cmdline(self.vm_id);
        self.memory.write_cmdline(&cmdline)?;
        let effective_cmdline = String::from_utf8_lossy(&cmdline);
        validate_guest_determinism_profile(
            &self.guest_determinism_profile,
            effective_cmdline.trim_end_matches(char::from(0)),
        )
        .map_err(|error| VmError::RuntimeCapacity {
            message: format!("guest determinism profile admission failed: {error:?}"),
        })?;
        // r[impl chaoscontrol.guest_determinism.boot_entropy]
        let boot_entropy_seed = derive_boot_entropy_seed(self.guest_determinism_profile.input);
        let rng_setup_data = encode_linux_rng_seed_setup_data(boot_entropy_seed);
        self.memory
            .inner()
            .write_slice(&rng_setup_data, GuestAddress(RNG_SETUP_DATA_START))
            .map_err(|_| GuestMemoryWriteSnafu.build())?;

        let mut hdr = linux_loader::loader::bootparam::setup_header {
            type_of_loader: KERNEL_LOADER_OTHER,
            boot_flag: KERNEL_BOOT_FLAG_MAGIC,
            header: KERNEL_HDR_MAGIC,
            cmd_line_ptr: CMDLINE_START as u32,
            cmdline_size: cmdline.len() as u32,
            kernel_alignment: KERNEL_MIN_ALIGNMENT_BYTES,
            setup_data: RNG_SETUP_DATA_START,
            ..Default::default()
        };

        if let Some((initrd_addr, initrd_size)) = initrd_info {
            hdr.ramdisk_image = initrd_addr as u32;
            hdr.ramdisk_size = initrd_size as u32;
        }

        let mut params = boot_params {
            hdr,
            ..Default::default()
        };

        // Set up E820 memory map using memory module
        let e820_map = build_e820_map(self.memory.size() as u64);
        for (i, entry) in e820_map.iter().enumerate() {
            params.e820_table[i].addr = entry.addr;
            params.e820_table[i].size = entry.size;
            params.e820_table[i].type_ = entry.type_;
        }
        params.e820_entries = e820_map.len() as u8;

        // Write boot params to zero page
        let boot_params = BootParams::new(&params, GuestAddress(ZERO_PAGE_START));
        LinuxBootConfigurator::write_bootparams(&boot_params, self.memory.inner())
            .map_err(|_| GuestMemoryWriteSnafu.build())?;

        Ok(())
    }

    /// Set up segment registers for the BSP (vCPU 0).
    fn setup_sregs(&self) -> Result<(), VmError> {
        let mut sregs = self.vcpus[0].get_sregs().context(GetSregsSnafu)?;

        // Use segment helpers from memory module
        sregs.cs = code64_segment();

        let data_seg = data_segment();
        sregs.ds = data_seg;
        sregs.es = data_seg;
        sregs.fs = data_seg;
        sregs.gs = data_seg;
        sregs.ss = data_seg;

        sregs.tr = tss_segment();

        // GDT and IDT
        sregs.gdt.base = BOOT_GDT_OFFSET;
        sregs.gdt.limit = (GDT_ENTRY_COUNT as u16) * 8 - 1;
        sregs.idt.base = BOOT_IDT_OFFSET;
        sregs.idt.limit = 8 - 1;

        // Enable protected mode and long mode
        sregs.cr0 |= X86_CR0_PE | X86_CR0_PG;
        sregs.cr3 = PML4_START;
        sregs.cr4 |= X86_CR4_PAE;
        sregs.efer |= EFER_LME | EFER_LMA;

        self.vcpus[0].set_sregs(&sregs).context(SetSregsSnafu)?;
        Ok(())
    }

    /// Set up general-purpose registers for the BSP (vCPU 0).
    fn setup_regs(&self, entry_point: GuestAddress) -> Result<(), VmError> {
        let regs = kvm_regs {
            rip: entry_point.raw_value(),
            rsp: BOOT_STACK_POINTER,
            rbp: BOOT_STACK_POINTER,
            rsi: ZERO_PAGE_START, // Pointer to boot params
            rflags: 0x2,          // Reserved bit must be set
            ..Default::default()
        };
        self.vcpus[0].set_regs(&regs).context(SetRegistersSnafu)?;
        Ok(())
    }

    /// Set up FPU state for the BSP (vCPU 0).
    fn setup_fpu(&self) -> Result<(), VmError> {
        let fpu = kvm_fpu {
            fcw: 0x37f,
            mxcsr: 0x1f80,
            ..Default::default()
        };
        self.vcpus[0].set_fpu(&fpu).context(SetFpuSnafu)?;
        Ok(())
    }

    /// Reset all time-dependent state to deterministic values.
    ///
    /// Called immediately before the first `vcpu.run()` to ensure the
    /// guest sees identical starting conditions regardless of how long
    /// host-side setup took.
    fn reset_time_state(&self) -> Result<(), VmError> {
        // Reset TSC to 0 — the guest will read TSC=0 on first instruction
        self.reset_tsc_to_zero()?;

        // Reset KVM clock to 0 — any paravirt clock reads start at 0
        let clock_data = kvm_clock_data {
            clock: 0,
            ..Default::default()
        };
        self.vm.set_clock(&clock_data).context(SetClockSnafu)?;

        // KVM PIT's HPET_LEGACY flag inhibits its host timer.
        // Our DeterministicPit delivers IRQ 0 instead.

        Ok(())
    }

    // ─── Public API: execution ───────────────────────────────────────

    /// Run the VM until it halts or shuts down.
    pub fn run(&mut self) -> Result<(), VmError> {
        self.ensure_schedule_execution_healthy()?;
        // Reset time state as close to first vcpu.run() as possible
        self.reset_time_state()?;
        info!("Starting VM execution");
        loop {
            if self.step()? {
                break;
            }
        }
        info!(
            "VM stopped after {} exits ({} I/O), virtual TSC: {}",
            self.exit_count,
            self.io_exit_count,
            self.virtual_tsc.read(),
        );
        Ok(())
    }

    /// Run until the serial output contains `pattern`.
    ///
    /// Returns the captured serial output since the call.
    pub fn run_until(&mut self, pattern: &str) -> Result<String, VmError> {
        self.ensure_schedule_execution_healthy()?;
        if self.exit_count == 0 {
            self.reset_time_state()?;
        }
        self.serial_writer.take();
        loop {
            if self.step()? {
                break;
            }
            let s = self.serial_writer.as_string();
            if s.contains(pattern) {
                return Ok(s);
            }
        }
        Ok(self.serial_writer.as_string())
    }

    /// Run for a bounded number of vCPU exits.
    ///
    /// Returns `(exits_executed, halted)`.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn run_bounded(&mut self, max_exits: u64) -> Result<(u64, bool), VmError> {
        self.ensure_schedule_execution_healthy()?;
        if self.exit_count == 0 {
            self.reset_time_state()?;
        }
        /// After setup_complete, if no SDK calls happen for this many
        /// consecutive exits, treat the VM as idle (workload done).
        ///
        /// Must be far above any normal gap between SDK calls.  Active
        /// guests make SDK calls every ~50-100 exits, but kernel code
        /// paths (especially serial I/O from println) can produce long
        /// stretches without SDK port accesses.  50K exits = ~500 rounds
        /// of pure kernel serial polling — if the guest hasn't made a
        /// single SDK call in that time, it's truly idle.
        const SDK_IDLE_THRESHOLD: u64 = 50_000;

        // Track guest exits through `exit_count`, not loop iterations.
        // Operational interrupts do not consume the deterministic exit budget.
        let start_exits = self.exit_count;

        loop {
            let real_exits = self.exit_count - start_exits;
            if real_exits >= max_exits {
                break;
            }

            if self.step()? {
                // Disarm timer on early exit to prevent stale SIGALRMs
                // from leaking into subsequent VM runs in the same process.
                self.disarm_preemption_timer();
                return Ok((self.exit_count - start_exits, true));
            }
            if self.assertion_hit_stop_pending {
                self.disarm_preemption_timer();
                return Ok((self.exit_count - start_exits, false));
            }
            // Idle counter incremented in step() on every exit except
            // SDK/coverage port accesses (which reset it to 0).
            // Detect idle: workload done, guest stopped making SDK calls.

            if self.fault_engine.is_setup_complete()
                && self.exits_since_last_sdk > SDK_IDLE_THRESHOLD
            {
                info!(
                    "VM idle (no SDK calls for {} exits, exit_count={}, io_exits={}), treating as halted",
                    self.exits_since_last_sdk,
                    self.exit_count,
                    self.io_exit_count,
                );
                self.disarm_preemption_timer();
                return Ok((self.exit_count - start_exits, true));
            }
        }
        // Disarm the single-vCPU watchdog after the bounded run.
        self.disarm_preemption_timer();
        Ok((self.exit_count - start_exits, false))
    }

    // ─── Public API: serial output ───────────────────────────────────

    /// Take all serial output captured since the last call.
    ///
    /// Returns at most `MAX_SERIAL_CAPTURE_BYTES` of the most recent
    /// output. Check [`Self::serial_dropped_byte_count`] to detect
    /// dropped output.
    pub fn take_serial_output(&mut self) -> String {
        String::from_utf8_lossy(&self.serial_writer.take()).into_owned()
    }

    /// Total serial bytes dropped since VM creation because the capture
    /// buffer reached its bound. Non-zero means earlier serial output is
    /// no longer available.
    pub fn serial_dropped_byte_count(&self) -> u64 {
        self.serial_writer.dropped_byte_count()
    }

    // ─── Public API: determinism state ───────────────────────────────

    /// Get the current virtual TSC value.
    pub fn virtual_tsc(&self) -> u64 {
        self.virtual_tsc.read()
    }

    /// Get the total number of VM exits since creation.
    pub fn exit_count(&self) -> u64 {
        self.exit_count
    }

    /// Get the number of I/O exits since creation.
    pub fn io_exit_count(&self) -> u64 {
        self.io_exit_count
    }

    /// Get a reference to the virtual TSC tracker.
    pub fn virtual_tsc_ref(&self) -> &VirtualTsc {
        &self.virtual_tsc
    }

    /// Get a mutable reference to the virtual TSC tracker.
    pub fn virtual_tsc_mut(&mut self) -> &mut VirtualTsc {
        &mut self.virtual_tsc
    }

    /// Get a reference to the deterministic entropy source.
    pub fn entropy(&self) -> &DeterministicEntropy {
        &self.entropy
    }

    /// Get a mutable reference to the deterministic entropy source.
    pub fn entropy_mut(&mut self) -> &mut DeterministicEntropy {
        &mut self.entropy
    }

    /// Get the number of vCPUs in this VM.
    pub fn num_vcpus(&self) -> usize {
        self.vcpus.len()
    }

    /// Get the index of the currently active vCPU.
    pub fn active_vcpu(&self) -> usize {
        self.active_vcpu
    }

    /// Drain bounded canonical SMP progress evidence since the prior drain.
    pub fn take_schedule_trace(&mut self) -> Result<ScheduleTrace, VmError> {
        self.ensure_schedule_execution_healthy()?;
        Ok(self.scheduler.drain_trace()?)
    }

    /// Get the KVM MP state for each vCPU (for diagnostics).
    ///
    /// Returns `(vcpu_index, mp_state_u32)` for each vCPU.
    /// States: 0=RUNNABLE, 1=UNINITIALIZED, 2=INIT_RECEIVED, 3=HALTED, 4=SIPI_RECEIVED
    pub fn vcpu_mp_states(&self) -> Vec<(usize, u32)> {
        self.vcpus
            .iter()
            .enumerate()
            .map(|(i, vcpu)| {
                let state = vcpu.get_mp_state().map(|mp| mp.mp_state).unwrap_or(99);
                (i, state)
            })
            .collect()
    }

    /// Get a reference to the guest memory manager.
    pub fn memory(&self) -> &GuestMemoryManager {
        &self.memory
    }

    /// Return the current guest-visible memory ceiling.
    pub fn memory_ceiling_bytes(&self) -> u64 {
        self.memory_ceiling_bytes
    }

    /// Set the guest-visible memory ceiling without changing mapped memory.
    pub fn set_memory_ceiling_bytes(&mut self, limit_bytes: u64) -> Result<(), VmError> {
        let baseline_bytes = u64::try_from(self.memory.size()).map_err(|_| VmError::Snapshot {
            message: "guest memory size exceeds resource-observation bounds".to_string(),
        })?;
        if limit_bytes == 0 || limit_bytes > baseline_bytes {
            return Err(VmError::Snapshot {
                message: format!("memory ceiling {limit_bytes} is outside 1..={baseline_bytes}"),
            });
        }
        self.memory_ceiling_bytes = limit_bytes;
        Ok(())
    }

    /// Mark one vCPU stalled or runnable through the deterministic scheduler.
    pub fn set_vcpu_stalled(&mut self, vcpu: usize, stalled: bool) -> Result<(), VmError> {
        if vcpu >= self.vcpus.len() {
            return Err(VmError::Snapshot {
                message: format!("vCPU {vcpu} is outside the VM topology"),
            });
        }
        if stalled {
            self.forced_stalled_vcpus.insert(vcpu);
        } else {
            self.forced_stalled_vcpus.remove(&vcpu);
        }
        self.refresh_runnable_state()?;
        Ok(())
    }

    /// Return whether one vCPU is held by a fault window.
    pub fn vcpu_is_stalled(&self, vcpu: usize) -> bool {
        self.forced_stalled_vcpus.contains(&vcpu)
    }

    /// Get the KVM dirty page bitmap and atomically reset it.
    ///
    /// Returns a bitmap where each bit represents a 4 KB page. Bit N is
    /// set if the guest wrote to page N since the last call. The bitmap
    /// is packed as `Vec<u64>` — bit 0 of element 0 covers page 0, bit
    /// 63 of element 0 covers page 63, bit 0 of element 1 covers page
    /// 64, and so on.
    /// Observed pages remain tracked for later incremental capture and restore.
    ///
    /// Requires `KVM_MEM_LOG_DIRTY_PAGES` on the memory slot (enabled
    /// by default at VM creation).
    pub fn get_dirty_bitmap(&mut self) -> Result<Vec<u64>, VmError> {
        let bitmap = self.read_guest_dirty_bitmap()?;
        crate::vm_core::merge_dirty_pages(
            &mut self.pages_differing_from_base,
            &bitmap,
            self.memory.size().div_ceil(crate::snapshot::PAGE_SIZE),
        )
        .map_err(|error| VmError::Snapshot {
            message: format!("retain observed dirty pages: {error:?}"),
        })?;
        Ok(bitmap)
    }

    fn read_guest_dirty_bitmap(&self) -> Result<Vec<u64>, VmError> {
        self.vm
            .get_dirty_log(0, self.memory.size())
            .context(GetDirtyLogSnafu)
    }

    /// Returns whether dirty page logging is enabled.
    pub fn dirty_log_enabled(&self) -> bool {
        self.dirty_log_enabled
    }

    // ─── Public API: coverage ────────────────────────────────────

    /// Clear the coverage bitmap in guest memory (zero 64 KB).
    ///
    /// Call this before each execution quantum to get per-run coverage.
    pub fn clear_coverage_bitmap(&self) {
        let zeros = vec![0u8; COVERAGE_BITMAP_SIZE];
        let _ = self
            .memory
            .inner()
            .write_slice(&zeros, vm_memory::GuestAddress(COVERAGE_BITMAP_ADDR));
    }

    /// Read the coverage bitmap from guest memory.
    ///
    /// Returns the raw 64 KB bitmap. Use with
    /// `CoverageBitmap::from_slice`.
    pub fn read_coverage_bitmap(&self) -> Vec<u8> {
        let mut buf = vec![0u8; COVERAGE_BITMAP_SIZE];
        let _ = self
            .memory
            .inner()
            .read_slice(&mut buf, vm_memory::GuestAddress(COVERAGE_BITMAP_ADDR));
        buf
    }

    /// Check if guest has activated coverage instrumentation.
    pub fn coverage_active(&self) -> bool {
        self.coverage_active
    }

    /// Check if VMCALL-based SDK transport is enabled.
    ///
    /// When `true`, the guest uses `vmcall` instructions for SDK
    /// communication. When `false`, port I/O (`outb(0x510)`) is used.
    pub fn vmcall_enabled(&self) -> bool {
        self.vmcall_enabled
    }

    // ─── Public API: virtio devices ──────────────────────────────────

    /// Get a reference to the virtio MMIO devices.
    pub fn virtio_devices(&self) -> &[VirtioMmioDevice] {
        &self.virtio_devices
    }

    /// Get a mutable reference to the virtio MMIO devices.
    pub fn virtio_devices_mut(&mut self) -> &mut [VirtioMmioDevice] {
        &mut self.virtio_devices
    }

    /// Inject a fault into the VM's block device.
    ///
    /// Returns `true` if the block device was found and the fault was injected,
    /// `false` if no block device exists.
    pub fn inject_disk_fault(&mut self, fault: crate::devices::block::BlockFault) -> bool {
        self.with_block_device(|disk| disk.inject_fault(fault))
    }

    /// Inject a block fault bound to one selected attempt.
    pub fn inject_disk_fault_with_attempt(
        &mut self,
        fault: crate::devices::block::BlockFault,
        attempt_id: chaoscontrol_fault::outcomes::FaultAttemptId,
    ) -> bool {
        self.with_block_device(|disk| disk.inject_fault_with_attempt(fault, attempt_id))
    }

    /// Return the block-device size when the VM has a block backend.
    pub fn block_device_size_bytes(&mut self) -> Option<u64> {
        let mut size_bytes = None;
        self.with_block_device(|disk| size_bytes = Some(disk.size()));
        size_bytes
    }

    /// Set persistent disk-full behavior.
    pub fn set_disk_full(&mut self, is_full: bool) -> bool {
        self.with_block_device(|disk| disk.set_full(is_full))
    }

    /// Set persistent disk-full behavior for one selected attempt.
    pub fn set_disk_full_with_attempt(
        &mut self,
        is_full: bool,
        attempt_id: chaoscontrol_fault::outcomes::FaultAttemptId,
    ) -> bool {
        self.with_block_device(|disk| disk.set_full_with_attempt(is_full, attempt_id))
    }

    /// Set per-I/O delay on the block device (DiskSlow fault).
    pub fn set_disk_slow_delay(&mut self, delay_ns: u64) -> bool {
        self.with_block_device(|disk| disk.set_slow_delay_ns(delay_ns))
    }

    /// Set per-I/O delay for one selected attempt.
    pub fn set_disk_slow_delay_with_attempt(
        &mut self,
        delay_ns: u64,
        attempt_id: chaoscontrol_fault::outcomes::FaultAttemptId,
    ) -> bool {
        self.with_block_device(|disk| disk.set_slow_delay_with_attempt(delay_ns, attempt_id))
    }

    /// Enable fsync-lie mode on the block device.
    pub fn enable_disk_fsync_lie(&mut self) -> bool {
        self.with_block_device(|disk| disk.enable_fsync_lie())
    }

    /// Enable fsync-lie mode for one selected attempt.
    pub fn enable_disk_fsync_lie_with_attempt(
        &mut self,
        attempt_id: chaoscontrol_fault::outcomes::FaultAttemptId,
    ) -> bool {
        self.with_block_device(|disk| disk.enable_fsync_lie_with_attempt(attempt_id))
    }

    /// Flush volatile writes on the block device (DiskFsyncFlush).
    pub fn flush_disk_volatile(&mut self) -> bool {
        self.with_block_device(|disk| disk.flush_volatile())
    }

    /// Discard volatile writes on the block device (crash semantics).
    pub fn discard_disk_volatile(&mut self) -> bool {
        self.with_block_device(|disk| disk.discard_volatile())
    }

    /// Snapshot the block device's dirty + volatile pages for preservation
    /// across a VM restart (crash-recovery testing).
    pub fn snapshot_block_dirty(&mut self) -> Option<crate::devices::block::DirtyOverlay> {
        let mut result = None;
        for device in &mut self.virtio_devices {
            if device.backend().device_id() == 2 {
                if let Some(virtio_block) = device
                    .backend_mut()
                    .as_any_mut()
                    .downcast_mut::<crate::devices::virtio_block::VirtioBlock>(
                ) {
                    result = Some(virtio_block.disk_mut().snapshot_dirty());
                    break;
                }
            }
        }
        result
    }

    /// Restore dirty + volatile pages into the block device after a restart.
    pub fn restore_block_dirty(&mut self, overlay: crate::devices::block::DirtyOverlay) -> bool {
        self.with_block_device(|disk| disk.restore_dirty(overlay))
    }

    /// Return the central-ledger reservation for active block mechanisms.
    pub fn block_fault_observation_reservation(&mut self) -> usize {
        let mut reservation = 0;
        self.with_block_device(|disk| {
            reservation = disk.central_observation_reservation();
        });
        reservation
    }

    /// Drain block-path observations from all block backends.
    pub fn drain_block_fault_observations(
        &mut self,
    ) -> (Vec<chaoscontrol_fault::outcomes::FaultObservation>, u64) {
        let mut observations = Vec::new();
        let mut overflowed = 0u64;
        self.with_block_device(|disk| {
            let (drained, dropped) = disk.drain_fault_observations();
            observations = drained;
            overflowed = dropped;
        });
        (observations, overflowed)
    }

    /// Restore a failed block observation batch in its original order.
    pub fn requeue_block_fault_observations(
        &mut self,
        observations: Vec<chaoscontrol_fault::outcomes::FaultObservation>,
        overflowed: u64,
    ) -> bool {
        self.with_block_device(|disk| disk.requeue_fault_observations(observations, overflowed))
    }

    /// Return the configured vCPU count.
    pub fn vcpu_count(&self) -> usize {
        self.vcpus.len()
    }

    /// Helper: run a closure on the virtio-blk disk, if present.
    fn with_block_device(
        &mut self,
        f: impl FnOnce(&mut crate::devices::block::DeterministicBlock),
    ) -> bool {
        for device in &mut self.virtio_devices {
            if device.backend().device_id() == 2 {
                if let Some(virtio_block) = device
                    .backend_mut()
                    .as_any_mut()
                    .downcast_mut::<crate::devices::virtio_block::VirtioBlock>(
                ) {
                    f(virtio_block.disk_mut());
                    return true;
                }
            }
        }
        false
    }

    /// Flip a single bit in a vCPU's general-purpose register.
    pub fn bitflip_register(
        &self,
        vcpu_idx: usize,
        register: chaoscontrol_fault::faults::GpRegister,
        bit: u8,
    ) -> Result<(), VmError> {
        if vcpu_idx >= self.vcpus.len() {
            return Err(VmError::Snapshot {
                message: format!(
                    "bitflip: vCPU index {} out of range (have {})",
                    vcpu_idx,
                    self.vcpus.len()
                ),
            });
        }
        const GENERAL_REGISTER_BIT_COUNT: u8 = 64;
        if bit >= GENERAL_REGISTER_BIT_COUNT {
            return Err(VmError::Snapshot {
                message: format!("bitflip: bit {bit} is out of range"),
            });
        }
        let vcpu = &self.vcpus[vcpu_idx];
        let mut regs = vcpu.get_regs().context(GetRegistersSnafu)?;
        let val = gp_register_get(&regs, register);
        let flipped = val ^ (1u64 << bit);
        gp_register_set(&mut regs, register, flipped);
        vcpu.set_regs(&regs).context(SetRegistersSnafu)?;
        Ok(())
    }

    /// Drain all TX packets from the network device.
    ///
    /// Returns packets transmitted by the guest since the last drain.
    /// The network device is at index 1 in the virtio devices array.
    pub fn drain_net_tx(&mut self) -> Vec<Vec<u8>> {
        // Device index 1 is virtio-net (0=block, 1=net, 2=rng)
        if let Some(device) = self.virtio_devices.get_mut(1) {
            if let Some(virtio_net) = device
                .backend_mut()
                .as_any_mut()
                .downcast_mut::<crate::devices::virtio_net::VirtioNet>()
            {
                return virtio_net.net_mut().drain_tx();
            }
        }
        Vec::new()
    }

    /// Inject a packet into the network device's RX queue.
    ///
    /// The packet will be delivered to the guest on the next virtqueue kick.
    /// The network device is at index 1 in the virtio devices array.
    pub fn inject_net_rx(&mut self, packet: Vec<u8>) {
        // Device index 1 is virtio-net (0=block, 1=net, 2=rng)
        if let Some(device) = self.virtio_devices.get_mut(1) {
            if let Some(virtio_net) = device
                .backend_mut()
                .as_any_mut()
                .downcast_mut::<crate::devices::virtio_net::VirtioNet>()
            {
                virtio_net.net_mut().inject_packet(packet);
            }
        }
    }

    /// Get the MAC address of the network device.
    ///
    /// Returns `None` if the network device is not found.
    /// The network device is at index 1 in the virtio devices array.
    pub fn net_mac(&self) -> Option<[u8; 6]> {
        // Device index 1 is virtio-net (0=block, 1=net, 2=rng)
        if let Some(device) = self.virtio_devices.get(1) {
            if let Some(virtio_net) = device
                .backend()
                .as_any()
                .downcast_ref::<crate::devices::virtio_net::VirtioNet>()
            {
                return Some(*virtio_net.net().mac());
            }
        }
        None
    }

    pub(crate) fn write_execution_registers(
        &mut self,
        registers: &kvm_regs,
    ) -> Result<(), VmError> {
        self.ensure_schedule_execution_healthy()?;
        self.vcpus[0].set_regs(registers).context(SetRegistersSnafu)
    }

    // ─── Public API: interrupt injection ─────────────────────────────

    /// Inject a hardware interrupt (IRQ) into the VM.
    ///
    /// Pulses the specified IRQ line in the in-kernel IRQ chip
    /// (edge-triggered: assert then deassert). The guest's interrupt
    /// handler will be invoked on the next VM entry.
    ///
    /// Standard x86 IRQs: 0 = PIT timer, 4 = COM1 serial,
    /// 5-7 = virtio MMIO devices.
    pub fn inject_interrupt(&mut self, irq: u32) -> Result<(), VmError> {
        self.inject_irq_from(irq, crate::snapshot::InjectionSource::Shell)
    }

    /// Inject a non-maskable interrupt (NMI) into a specific vCPU.
    ///
    /// NMIs bypass interrupt masking and are delivered immediately on
    /// the next VM entry. Used to test crash handlers, watchdog paths,
    /// and profiling code.
    ///
    /// Uses the raw `KVM_NMI` ioctl since kvm-ioctls 0.19 doesn't
    /// expose a typed wrapper.
    pub fn inject_nmi(&mut self, vcpu_idx: usize) -> Result<(), VmError> {
        self.ensure_schedule_execution_healthy()?;
        if !self.supports_nmi_injection() {
            return Err(VmError::ProgressCapability {
                message:
                    "NMI/nested-NMI delivery is outside the admitted coarse clock/device profile"
                        .into(),
            });
        }
        if vcpu_idx >= self.vcpus.len() {
            return Err(VmError::Snapshot {
                message: format!(
                    "vCPU index {} out of range (have {})",
                    vcpu_idx,
                    self.vcpus.len()
                ),
            });
        }

        use std::os::unix::io::AsRawFd;
        // KVM_NMI = _IO(KVMIO, 0x9a) = _IO(0xAE, 0x9a) = 0xae9a
        const KVM_NMI: libc::c_ulong = 0xae9a;

        let vcpu_fd = self.vcpus[vcpu_idx].as_raw_fd();
        // SAFETY: KVM_NMI is a well-defined ioctl on valid vCPU fds.
        // It takes no arguments (third parameter is ignored).
        let ret = unsafe { libc::ioctl(vcpu_fd, KVM_NMI, 0) };

        if ret == 0 {
            log::debug!(
                "Injected NMI into vCPU {} (exit_count={}, vtsc={})",
                vcpu_idx,
                self.exit_count,
                self.virtual_tsc.read()
            );
            Ok(())
        } else {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ENOTTY) {
                return Err(VmError::Snapshot {
                    message: "KVM_NMI is not supported on this kernel".to_string(),
                });
            }
            Err(VmError::Io { source: err })
        }
    }

    // ─── Public API: snapshot / restore ──────────────────────────────

    fn snapshot_topology(&self) -> Result<crate::snapshot::SnapshotTopology, VmError> {
        use crate::snapshot::VirtioDeviceIdentity;

        let vcpu_count = u32::try_from(self.vcpus.len()).map_err(|_| VmError::Snapshot {
            message: "vCPU count exceeds snapshot schema width".to_string(),
        })?;
        let msr_indices = self.snapshot_msr_indices.clone();
        let mut virtio_devices = Vec::with_capacity(self.virtio_devices.len());
        let mut identities = std::collections::BTreeSet::new();
        for device in &self.virtio_devices {
            let queue_count =
                u32::try_from(device.queue_count()).map_err(|_| VmError::Snapshot {
                    message: "virtio queue count exceeds snapshot schema width".to_string(),
                })?;
            let identity = VirtioDeviceIdentity {
                base_addr: device.base_addr(),
                irq: device.irq(),
                device_id: device.backend().device_id(),
            };
            if !identities.insert(identity.clone()) {
                return Err(VmError::Snapshot {
                    message: format!("duplicate live virtio identity {identity:?}"),
                });
            }
            virtio_devices.push((identity, queue_count));
        }
        virtio_devices.sort();
        Ok(crate::snapshot::SnapshotTopology {
            vcpu_count,
            msr_indices,
            virtio_devices,
        })
    }

    /// Settle pending KVM I/O bookkeeping at the same boundary as snapshots.
    /// Immediate exit completes an already executed instruction, but cannot
    /// execute the next guest instruction or create scheduler progress evidence.
    /// No guest memory, device state, or snapshot payload is copied.
    pub(crate) fn settle_exact_observation(&mut self) -> Result<(), VmError> {
        self.ensure_schedule_execution_healthy()?;
        if self.pending_schedule_reservation.is_some() {
            return Err(VmError::Snapshot {
                message: "cannot inspect while a host-interrupt retry holds schedule evidence"
                    .into(),
            });
        }
        self.complete_pending_kvm_exits_for_snapshot()
    }

    fn complete_pending_kvm_exits_for_snapshot(&mut self) -> Result<(), VmError> {
        if self.coarse_mode() {
            // Every admitted I/O boundary was settled before its commit.
            return self.validate_coarse_clock_devices();
        }
        for vcpu_id in 0..self.vcpus.len() {
            self.vcpus[vcpu_id].set_kvm_immediate_exit(1);
            let interrupted = match self.vcpus[vcpu_id].run() {
                Ok(VcpuExit::Intr) => true,
                Err(error) => error.errno() == libc::EINTR,
                Ok(_) => false,
            };
            self.vcpus[vcpu_id].set_kvm_immediate_exit(0);
            if !interrupted {
                return Err(self.latch_schedule_execution_poison(
                    SchedulePoisonStage::SnapshotQuiesce,
                    format!("vCPU {vcpu_id} did not stop at the immediate-exit snapshot boundary"),
                ));
            }
        }
        Ok(())
    }

    fn capture_virtio_snapshots(
        &self,
    ) -> Result<Vec<crate::snapshot::VirtioDeviceSnapshot>, VmError> {
        use crate::snapshot::{VirtioBackendSnapshot, VirtioDeviceSnapshot};

        self.virtio_devices
            .iter()
            .map(|device| {
                let transport = device.snapshot().map_err(|failure| VmError::Snapshot {
                    message: format!("virtio transport is not quiescent: {failure:?}"),
                })?;
                let backend = if let Some(block) = device
                    .backend()
                    .as_any()
                    .downcast_ref::<crate::devices::virtio_block::VirtioBlock>(
                ) {
                    VirtioBackendSnapshot::Block(Box::new(block.disk().snapshot()))
                } else if let Some(net) = device
                    .backend()
                    .as_any()
                    .downcast_ref::<crate::devices::virtio_net::VirtioNet>()
                {
                    VirtioBackendSnapshot::Net(Box::new(net.net().snapshot()))
                } else if let Some(entropy) = device
                    .backend()
                    .as_any()
                    .downcast_ref::<crate::devices::virtio_entropy::VirtioEntropy>(
                ) {
                    VirtioBackendSnapshot::Entropy(Box::new(entropy.entropy().snapshot()))
                } else {
                    return Err(VmError::Snapshot {
                        message: format!(
                            "virtio device {} has no exact backend snapshot adapter",
                            device.backend().device_id()
                        ),
                    });
                };
                match &backend {
                    VirtioBackendSnapshot::Block(snapshot) => {
                        snapshot
                            .validate_structure()
                            .map_err(|error| VmError::Snapshot {
                                message: format!(
                                    "live block backend cannot produce an exact snapshot: {error:?}"
                                ),
                            })?;
                    }
                    VirtioBackendSnapshot::Net(snapshot) => {
                        snapshot
                            .validate_structure(device.limits())
                            .map_err(|error| VmError::Snapshot {
                                message: format!(
                                    "live network backend cannot produce an exact snapshot: {error:?}"
                                ),
                            })?;
                    }
                    VirtioBackendSnapshot::Entropy(_) => {}
                }
                Ok(VirtioDeviceSnapshot { transport, backend })
            })
            .collect()
    }

    fn find_virtio_device_index(
        &self,
        identity: &crate::snapshot::VirtioDeviceIdentity,
    ) -> Option<usize> {
        self.virtio_devices.iter().position(|device| {
            device.base_addr() == identity.base_addr
                && device.irq() == identity.irq
                && device.backend().device_id() == identity.device_id
        })
    }

    fn validate_virtio_snapshot(
        &self,
        snapshot: &crate::snapshot::VmSnapshot,
    ) -> Result<(), VmError> {
        use crate::snapshot::VirtioBackendSnapshot;

        for device_snapshot in &snapshot.virtio_snapshots {
            let identity = device_snapshot.identity();
            let index =
                self.find_virtio_device_index(&identity)
                    .ok_or_else(|| VmError::Snapshot {
                        message: format!("snapshot has unknown virtio identity {identity:?}"),
                    })?;
            let device = &self.virtio_devices[index];
            device
                .validate_snapshot_facts(
                    &device_snapshot.transport,
                    &[crate::devices::virtio_validation::MemoryRegion {
                        start: 0,
                        length: snapshot.memory.memory_size() as u64,
                    }],
                    |address| {
                        snapshot
                            .memory
                            .read_u16(address)
                            .ok_or(crate::devices::virtio_types::VirtioFailure::GuestMemoryRead)
                    },
                )
                .map_err(|failure| VmError::Snapshot {
                    message: format!("invalid virtio transport snapshot: {failure:?}"),
                })?;
            if device_snapshot.backend.device_id() != identity.device_id {
                return Err(VmError::Snapshot {
                    message: format!("virtio backend type conflicts with identity {identity:?}"),
                });
            }
            let backend_matches = match &device_snapshot.backend {
                VirtioBackendSnapshot::Block(block_snapshot) => {
                    block_snapshot
                        .validate_structure()
                        .map_err(|error| VmError::Snapshot {
                            message: format!(
                                "invalid block backend snapshot for {identity:?}: {error:?}"
                            ),
                        })?;
                    if let Some(block) = device
                        .backend()
                        .as_any()
                        .downcast_ref::<crate::devices::virtio_block::VirtioBlock>(
                    ) {
                        block_snapshot
                            .validate_device_size(block.disk().size())
                            .map_err(|error| VmError::Snapshot {
                                message: format!(
                                    "block backend topology mismatch for {identity:?}: {error:?}"
                                ),
                            })?;
                        true
                    } else {
                        false
                    }
                }
                VirtioBackendSnapshot::Net(net_snapshot) => {
                    net_snapshot
                        .validate_structure(device.limits())
                        .map_err(|error| VmError::Snapshot {
                            message: format!(
                                "invalid network backend snapshot for {identity:?}: {error:?}"
                            ),
                        })?;
                    if let Some(net) = device
                        .backend()
                        .as_any()
                        .downcast_ref::<crate::devices::virtio_net::VirtioNet>()
                    {
                        net_snapshot
                            .validate_mac(*net.net().mac())
                            .map_err(|error| VmError::Snapshot {
                                message: format!(
                                    "network backend topology mismatch for {identity:?}: {error:?}"
                                ),
                            })?;
                        true
                    } else {
                        false
                    }
                }
                VirtioBackendSnapshot::Entropy(_) => device
                    .backend()
                    .as_any()
                    .is::<crate::devices::virtio_entropy::VirtioEntropy>(
                ),
            };
            if !backend_matches {
                return Err(VmError::Snapshot {
                    message: format!("virtio backend adapter mismatch for {identity:?}"),
                });
            }
        }
        Ok(())
    }

    fn restore_virtio_snapshot(
        &mut self,
        snapshot: &crate::snapshot::VmSnapshot,
    ) -> Result<(), VmError> {
        use crate::snapshot::VirtioBackendSnapshot;

        for device_snapshot in &snapshot.virtio_snapshots {
            let identity = device_snapshot.identity();
            let index =
                self.find_virtio_device_index(&identity)
                    .ok_or_else(|| VmError::Snapshot {
                        message: format!("snapshot has unknown virtio identity {identity:?}"),
                    })?;
            let device = &mut self.virtio_devices[index];
            device
                .restore_snapshot(&device_snapshot.transport, self.memory.inner())
                .map_err(|failure| VmError::Snapshot {
                    message: format!("virtio transport restore failed: {failure:?}"),
                })?;
            match &device_snapshot.backend {
                VirtioBackendSnapshot::Block(block_snapshot) => {
                    let block = device
                        .backend_mut()
                        .as_any_mut()
                        .downcast_mut::<crate::devices::virtio_block::VirtioBlock>()
                        .ok_or_else(|| VmError::Snapshot {
                            message: "validated block backend disappeared".to_string(),
                        })?;
                    *block.disk_mut() =
                        crate::devices::block::DeterministicBlock::restore(block_snapshot);
                }
                VirtioBackendSnapshot::Net(net_snapshot) => {
                    let net = device
                        .backend_mut()
                        .as_any_mut()
                        .downcast_mut::<crate::devices::virtio_net::VirtioNet>()
                        .ok_or_else(|| VmError::Snapshot {
                            message: "validated net backend disappeared".to_string(),
                        })?;
                    *net.net_mut() = crate::devices::net::DeterministicNet::restore(net_snapshot);
                }
                VirtioBackendSnapshot::Entropy(entropy_snapshot) => {
                    let entropy = device
                        .backend_mut()
                        .as_any_mut()
                        .downcast_mut::<crate::devices::virtio_entropy::VirtioEntropy>()
                        .ok_or_else(|| VmError::Snapshot {
                            message: "validated entropy backend disappeared".to_string(),
                        })?;
                    *entropy.entropy_mut() =
                        crate::devices::entropy::DeterministicEntropy::restore(entropy_snapshot);
                }
            }
        }
        Ok(())
    }

    /// Take a snapshot of the current VM state.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn snapshot(&mut self) -> Result<crate::snapshot::VmSnapshot, VmError> {
        use crate::snapshot::CaptureParams;

        self.ensure_schedule_execution_healthy()?;
        if self.pending_schedule_reservation.is_some() {
            return Err(VmError::Snapshot {
                message: "cannot snapshot while a host-interrupt retry holds schedule evidence"
                    .to_string(),
            });
        }

        let topology = self.snapshot_topology()?;
        self.complete_pending_kvm_exits_for_snapshot()?;
        let virtio_snapshots = self.capture_virtio_snapshots()?;

        let params = CaptureParams {
            execution_descriptor: self.execution_descriptor.clone(),
            boundary_state: self.snapshot_boundary_state(),
            topology,
            serial_state: self.serial.state(),
            entropy: self.entropy.snapshot(),
            virtual_tsc: self.virtual_tsc.read(),
            exit_count: self.exit_count,
            io_exit_count: self.io_exit_count,
            exits_since_last_sdk: self.exits_since_last_sdk,
            panic_detected: self.panic_detected,
            panic_match_state: self.panic_match_state,
            pit_snapshot: self.pit.snapshot(),
            last_kvm_pit_mode: self.last_kvm_pit_mode,
            fault_engine_snapshot: self.fault_engine.snapshot(),
            virtio_snapshots,
            coverage_active: self.coverage_active,
            scheduler_snapshot: self.scheduler.snapshot(),
            hlt_latched_vcpus: self.hlt_latched_vcpus.clone(),
        };

        let result = crate::snapshot::VmSnapshot::capture(
            &self.vcpus,
            &self.vm,
            self.memory.inner(),
            params,
        )
        .map_err(|e| {
            SnapshotSnafu {
                message: e.to_string(),
            }
            .build()
        });
        // Caller emits the snapshot receipt after this capture succeeds.
        result
    }

    fn reconcile_incremental_base(&mut self, base: &[u8]) -> Result<(), VmError> {
        if !self.incremental_base_needs_reconcile {
            return Ok(());
        }
        let page_count = base.len().div_ceil(crate::snapshot::PAGE_SIZE);
        self.pages_differing_from_base
            .resize(page_count.div_ceil(u64::BITS as usize), 0);
        self.pages_differing_from_base.fill(0);
        let mut page = [0u8; crate::snapshot::PAGE_SIZE];
        for (index, base_page) in base.chunks(crate::snapshot::PAGE_SIZE).enumerate() {
            self.memory
                .inner()
                .read_slice(
                    &mut page[..base_page.len()],
                    GuestAddress((index * crate::snapshot::PAGE_SIZE) as u64),
                )
                .map_err(|error| VmError::Snapshot {
                    message: format!("compare restored memory with incremental base: {error}"),
                })?;
            if page[..base_page.len()] != *base_page {
                self.pages_differing_from_base[index / u64::BITS as usize] |=
                    1u64 << (index % u64::BITS as usize);
            }
        }
        self.incremental_base_needs_reconcile = false;
        Ok(())
    }

    /// Take an incremental snapshot using KVM dirty page tracking.
    ///
    /// Captures the union of inherited overlay pages and guest writes since
    /// the last dirty-log query. Every overlay is complete relative to the
    /// retained bootstrap `base`, independent of its lineage depth.
    ///
    /// Returns the snapshot and the number of dirty pages captured.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn snapshot_incremental(
        &mut self,
        base: &std::sync::Arc<Vec<u8>>,
    ) -> Result<(crate::snapshot::VmSnapshot, usize), VmError> {
        use crate::snapshot::{CaptureParams, SnapshotMemory};

        self.ensure_schedule_execution_healthy()?;
        if self.pending_schedule_reservation.is_some() {
            return Err(VmError::Snapshot {
                message: "cannot snapshot while a host-interrupt retry holds schedule evidence"
                    .to_string(),
            });
        }

        if base.len() != self.memory.size() {
            return Err(VmError::Snapshot {
                message: "incremental snapshot base size differs from guest memory".to_string(),
            });
        }
        let topology = self.snapshot_topology()?;
        self.complete_pending_kvm_exits_for_snapshot()?;
        self.reconcile_incremental_base(base)?;
        let _ = self.get_dirty_bitmap()?;
        let memory =
            SnapshotMemory::from_dirty(base, &self.pages_differing_from_base, self.memory.inner());
        let dirty_count = memory.dirty_page_count();
        let virtio_snapshots = self.capture_virtio_snapshots()?;

        let params = CaptureParams {
            execution_descriptor: self.execution_descriptor.clone(),
            boundary_state: self.snapshot_boundary_state(),
            topology,
            serial_state: self.serial.state(),
            entropy: self.entropy.snapshot(),
            virtual_tsc: self.virtual_tsc.read(),
            exit_count: self.exit_count,
            io_exit_count: self.io_exit_count,
            exits_since_last_sdk: self.exits_since_last_sdk,
            panic_detected: self.panic_detected,
            panic_match_state: self.panic_match_state,
            pit_snapshot: self.pit.snapshot(),
            last_kvm_pit_mode: self.last_kvm_pit_mode,
            fault_engine_snapshot: self.fault_engine.snapshot(),
            virtio_snapshots,
            coverage_active: self.coverage_active,
            scheduler_snapshot: self.scheduler.snapshot(),
            hlt_latched_vcpus: self.hlt_latched_vcpus.clone(),
        };

        // Build the VmSnapshot with overlay memory instead of full copy.
        // We bypass VmSnapshot::capture() because it always does a full
        // memory read. Instead, construct the struct directly with the
        // overlay memory we already built.
        let mut vcpu_snapshots = Vec::with_capacity(self.vcpus.len());
        for vcpu in &self.vcpus {
            vcpu_snapshots.push(
                crate::snapshot::VcpuSnapshot::capture(vcpu, &params.topology.msr_indices)
                    .map_err(|e| {
                        SnapshotSnafu {
                            message: e.to_string(),
                        }
                        .build()
                    })?,
            );
        }

        let pic_master = {
            let mut chip = kvm_bindings::kvm_irqchip {
                chip_id: kvm_bindings::KVM_IRQCHIP_PIC_MASTER,
                ..Default::default()
            };
            self.vm.get_irqchip(&mut chip).map_err(|e| {
                SnapshotSnafu {
                    message: format!("get_irqchip(PIC_MASTER): {e}"),
                }
                .build()
            })?;
            chip
        };
        let pic_slave = {
            let mut chip = kvm_bindings::kvm_irqchip {
                chip_id: kvm_bindings::KVM_IRQCHIP_PIC_SLAVE,
                ..Default::default()
            };
            self.vm.get_irqchip(&mut chip).map_err(|e| {
                SnapshotSnafu {
                    message: format!("get_irqchip(PIC_SLAVE): {e}"),
                }
                .build()
            })?;
            chip
        };
        let ioapic = {
            let mut chip = kvm_bindings::kvm_irqchip {
                chip_id: kvm_bindings::KVM_IRQCHIP_IOAPIC,
                ..Default::default()
            };
            self.vm.get_irqchip(&mut chip).map_err(|e| {
                SnapshotSnafu {
                    message: format!("get_irqchip(IOAPIC): {e}"),
                }
                .build()
            })?;
            chip
        };
        let pit = if self.execution_descriptor.kernel_pit_present() {
            crate::snapshot::SnapshotKernelPitState::Present(self.vm.get_pit2().map_err(|e| {
                SnapshotSnafu {
                    message: format!("get_pit2: {e}"),
                }
                .build()
            })?)
        } else {
            crate::snapshot::SnapshotKernelPitState::Absent
        };
        let clock = self.vm.get_clock().map_err(|e| {
            SnapshotSnafu {
                message: format!("get_clock: {e}"),
            }
            .build()
        })?;

        let metadata = crate::snapshot::SnapshotMetadata {
            state_schema_version: crate::snapshot::SNAPSHOT_STATE_SCHEMA_VERSION,
            completeness_profile: crate::snapshot::SNAPSHOT_PROFILE_EXACT_X86_KVM_V2.to_string(),
            inventory: crate::snapshot::build_snapshot_inventory(&params.topology),
            topology: params.topology.clone(),
            execution_descriptor: params.execution_descriptor,
        };
        let snap = crate::snapshot::VmSnapshot {
            metadata: Some(metadata),
            vcpu_snapshots,
            pic_master,
            pic_slave,
            ioapic,
            pit,
            clock,
            memory,
            serial_state: params.serial_state,
            entropy: params.entropy,
            virtual_tsc: params.virtual_tsc,
            exit_count: params.exit_count,
            io_exit_count: params.io_exit_count,
            exits_since_last_sdk: params.exits_since_last_sdk,
            panic_detected: params.panic_detected,
            panic_match_state: params.panic_match_state,
            pit_snapshot: params.pit_snapshot,
            last_kvm_pit_mode: params.last_kvm_pit_mode,
            fault_engine_snapshot: params.fault_engine_snapshot,
            virtio_snapshots: params.virtio_snapshots,
            coverage_active: params.coverage_active,
            scheduler_snapshot: params.scheduler_snapshot,
            hlt_latched_vcpus: params.hlt_latched_vcpus,
            boundary_state: params.boundary_state,
        };

        info!(
            "Incremental snapshot: {} dirty pages ({} KB), {} vCPUs",
            dirty_count,
            dirty_count * 4,
            snap.vcpu_snapshots.len()
        );

        Ok((snap, dirty_count))
    }

    /// Validate all untrusted snapshot state before any restore effect.
    pub fn validate_fault_engine_snapshot(
        &self,
        snapshot: &crate::snapshot::VmSnapshot,
    ) -> Result<(), VmError> {
        self.ensure_schedule_execution_healthy()?;
        snapshot
            .validate_kernel_restore(self.vcpus.len())
            .map_err(|error| VmError::Snapshot {
                message: format!("snapshot continuation preflight failed: {error}"),
            })?;
        if self.coarse_mode()
            && snapshot.vcpu_snapshots.iter().any(|vcpu| {
                vcpu.msrs.iter().any(|msr| {
                    coarse_boundary::UNSUPPORTED_CLOCK_MSRS.contains(&msr.index) && msr.data != 0
                })
            })
        {
            return Err(VmError::ProgressCapability {
                message:
                    "coarse checkpoint enabled an unsupported PV host clock or TSC-deadline source"
                        .into(),
            });
        }
        if self.pending_schedule_reservation.is_some() {
            return Err(VmError::Snapshot {
                message: "cannot restore while a host-interrupt retry holds schedule evidence"
                    .to_string(),
            });
        }
        let expected_topology = self.snapshot_topology()?;
        crate::snapshot::validate_snapshot_metadata(
            snapshot.metadata.as_ref(),
            &expected_topology,
            &self.execution_descriptor,
        )
        .map_err(|error| VmError::Snapshot {
            message: format!("snapshot completeness preflight failed: {error:?}"),
        })?;
        if let Err((snapshot_count, current_count)) =
            validate_snapshot_count(snapshot.vcpu_snapshots.len(), self.vcpus.len())
        {
            return Err(VmError::Snapshot {
                message: format!(
                    "snapshot vCPU count {snapshot_count} differs from VM count {current_count}"
                ),
            });
        }
        for vcpu_snapshot in &snapshot.vcpu_snapshots {
            vcpu_snapshot
                .validate_msr_inventory(&expected_topology.msr_indices)
                .map_err(|error| VmError::Snapshot {
                    message: format!("invalid vCPU snapshot: {error}"),
                })?;
        }
        snapshot
            .memory
            .validate_for_guest_size(self.memory.size())
            .map_err(|message| VmError::Snapshot { message })?;
        for (component, actual, expected) in [
            (
                "pic_master",
                snapshot.pic_master.chip_id,
                kvm_bindings::KVM_IRQCHIP_PIC_MASTER,
            ),
            (
                "pic_slave",
                snapshot.pic_slave.chip_id,
                kvm_bindings::KVM_IRQCHIP_PIC_SLAVE,
            ),
            (
                "ioapic",
                snapshot.ioapic.chip_id,
                kvm_bindings::KVM_IRQCHIP_IOAPIC,
            ),
        ] {
            if actual != expected {
                return Err(VmError::Snapshot {
                    message: format!(
                        "snapshot {component} identity {actual} differs from required {expected}"
                    ),
                });
            }
        }
        snapshot
            .validate_kernel_state()
            .map_err(|error| VmError::Snapshot {
                message: format!("invalid in-kernel snapshot: {error}"),
            })?;
        if snapshot.pit_snapshot.tsc_khz == 0
            || snapshot.pit_snapshot.tsc_khz != self.virtual_tsc.tsc_khz()
        {
            return Err(VmError::Snapshot {
                message: "snapshot PIT frequency differs from the admitted virtual TSC profile"
                    .to_string(),
            });
        }
        if snapshot.last_kvm_pit_mode > 7 && snapshot.last_kvm_pit_mode != u8::MAX {
            return Err(VmError::Snapshot {
                message: "snapshot PIT mirror mode is neither a hardware mode nor unsynchronized"
                    .to_string(),
            });
        }
        if !serial_snapshot_fits(snapshot.serial_state.in_buffer.len(), SERIAL_FIFO_CAPACITY) {
            return Err(VmError::Snapshot {
                message: "snapshot serial input exceeds the UART FIFO capacity".to_string(),
            });
        }
        if let Err((snapshot_count, current_count)) =
            validate_snapshot_count(snapshot.virtio_snapshots.len(), self.virtio_devices.len())
        {
            return Err(VmError::Snapshot {
                message: format!(
                    "snapshot virtio count {snapshot_count} differs from VM count {current_count}"
                ),
            });
        }
        let mut snapshot_topology = crate::snapshot::SnapshotTopology {
            vcpu_count: expected_topology.vcpu_count,
            msr_indices: expected_topology.msr_indices.clone(),
            virtio_devices: snapshot
                .virtio_snapshots
                .iter()
                .map(|device| {
                    u32::try_from(device.transport.queues.len())
                        .map(|queue_count| (device.identity(), queue_count))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| VmError::Snapshot {
                    message: "snapshot virtio queue count exceeds schema width".to_string(),
                })?,
        };
        snapshot_topology.virtio_devices.sort();
        if snapshot_topology != expected_topology {
            return Err(VmError::Snapshot {
                message: "snapshot virtio identities do not match VM topology".to_string(),
            });
        }
        self.validate_virtio_snapshot(snapshot)?;
        self.scheduler
            .validate_snapshot(&snapshot.scheduler_snapshot)?;
        if !hlt_snapshot_is_valid(
            &snapshot.hlt_latched_vcpus,
            &snapshot.scheduler_snapshot.state.runnable_vcpus,
            self.vcpus.len(),
        ) {
            return Err(VmError::Snapshot {
                message: "snapshot HLT latches conflict with scheduler state".to_string(),
            });
        }
        self.fault_engine
            .validate_snapshot(&snapshot.fault_engine_snapshot)
            .map_err(|error| {
                SnapshotSnafu {
                    message: format!("invalid fault-engine snapshot: {error}"),
                }
                .build()
            })
    }

    /// Restore VM state from a snapshot.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn restore(&mut self, snapshot: &crate::snapshot::VmSnapshot) -> Result<(), VmError> {
        self.assertion_hits.clear();
        self.protocol_observations.reset();
        self.assertion_hit_overflow = 0;
        self.assertion_hit_stop_pending = false;
        self.validate_fault_engine_snapshot(snapshot)?;
        let mut phase = SnapshotRestorePhase::Quiesce;
        match self.restore_validated(snapshot, &mut phase) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.latch_schedule_execution_poison(
                    SchedulePoisonStage::SnapshotRestore,
                    error.to_string(),
                );
                Err(VmError::SnapshotRestoreFailed {
                    phase,
                    detail: error.to_string(),
                })
            }
        }
    }

    fn restore_validated(
        &mut self,
        snapshot: &crate::snapshot::VmSnapshot,
        phase: &mut SnapshotRestorePhase,
    ) -> Result<(), VmError> {
        self.complete_pending_kvm_exits_for_snapshot()?;
        // KVM rejects a changed static table on an already-run vCPU. Attempt
        // it before dirty tracking, guest memory or device restoration; partial
        // multi-vCPU installation remains inside the restore poison fence.
        *phase = SnapshotRestorePhase::Cpuid;
        snapshot
            .restore_cpuid(&self.vcpus)
            .map_err(|error| VmError::Snapshot {
                message: format!("restore guest CPUID: {error}"),
            })?;
        *phase = SnapshotRestorePhase::MemoryTracking;
        let overlay_pages = match &snapshot.memory {
            crate::snapshot::SnapshotMemory::Overlay { dirty_pages, base } => Some(
                crate::vm_core::overlay_page_bitmap(
                    std::mem::take(&mut self.pages_differing_from_base),
                    dirty_pages.keys().copied(),
                    base.len().div_ceil(crate::snapshot::PAGE_SIZE),
                )
                .map_err(|error| VmError::Snapshot {
                    message: format!("full overlay restore page plan: {error:?}"),
                })?,
            ),
            crate::snapshot::SnapshotMemory::Full(_) => None,
        };
        // Drain guest writes before the full host-side memory replacement.
        let _ = self.read_guest_dirty_bitmap()?;
        *phase = SnapshotRestorePhase::MemoryAndKernelState;
        snapshot
            .restore_after_cpuid(&self.vcpus, &self.vm, self.memory.inner())
            .map_err(|e| {
                SnapshotSnafu {
                    message: e.to_string(),
                }
                .build()
            })?;

        *phase = SnapshotRestorePhase::Determinism;
        // Restore deterministic entropy PRNG state
        self.entropy = DeterministicEntropy::restore(&snapshot.entropy);

        // Restore VMM-side counters. Controller-owned active fault windows
        // are reapplied after this base VM restore.
        self.virtual_tsc.set(snapshot.virtual_tsc);
        self.virtual_tsc.set_frozen(false);
        self.virtual_tsc.set_jitter_bound(0);
        self.forced_stalled_vcpus.clear();
        self.memory_ceiling_bytes =
            u64::try_from(self.memory.size()).map_err(|_| VmError::Snapshot {
                message: "guest memory size exceeds resource-observation bounds".to_string(),
            })?;
        self.exit_count = snapshot.exit_count;
        self.diagnostic_controller_tick = None;
        self.io_exit_count = snapshot.io_exit_count;
        self.exits_since_last_sdk = snapshot.exits_since_last_sdk;
        self.panic_detected = snapshot.panic_detected;
        self.panic_match_state = snapshot.panic_match_state;

        // Restore DeterministicPit state
        self.pit = DeterministicPit::restore(&snapshot.pit_snapshot);
        self.last_kvm_pit_mode = snapshot.last_kvm_pit_mode;

        // Restore fault and assertion state. The lower layer validates again.
        self.fault_engine
            .restore(&snapshot.fault_engine_snapshot)
            .map_err(|error| {
                SnapshotSnafu {
                    message: format!("invalid fault-engine snapshot: {error}"),
                }
                .build()
            })?;

        // Restore coverage flag
        self.coverage_active = snapshot.coverage_active;

        *phase = SnapshotRestorePhase::Scheduler;
        // Restore one exact scheduler state after all admission checks.
        let previous_active_vcpu = self.active_vcpu;
        self.disable_singlestep(previous_active_vcpu)?;
        self.scheduler.restore(&snapshot.scheduler_snapshot)?;
        self.injection_ledger = snapshot.boundary_state.injection_ledger.clone();
        self.injection_records.clear();
        self.hlt_latched_vcpus = Self::hlt_latches_from_snapshot(snapshot);
        self.active_vcpu = self.scheduler.active();
        self.configure_progress_source_for_current_turn()?;

        *phase = SnapshotRestorePhase::Virtio;
        self.restore_virtio_snapshot(snapshot)?;

        *phase = SnapshotRestorePhase::Serial;
        // The captured IRQ chips already contain the UART's delivered edge.
        let serial_trigger = SerialTrigger::default();
        self.serial_writer = CapturingWriter::new();
        self.serial = vm_superio::Serial::from_state(
            &snapshot.serial_state,
            serial_trigger,
            vm_superio::serial::NoEvents,
            self.serial_writer.clone(),
        )
        .map_err(|e| {
            SnapshotSnafu {
                message: format!("serial restore: {e}"),
            }
            .build()
        })?;

        // from_state reasserts a pending UART condition. Do not manufacture
        // another edge on top of the authoritative restored PIC state.
        self.serial.interrupt_evt().0.set(false);

        self.dlog_emit(
            self.dlog_record(DlogTag::SnapshotRestored)
                .with_data_u64(snapshot.exit_count),
        );
        self.dlog_flush();

        // Wall-clock watchdog state is shell-owned and excluded from the
        // deterministic snapshot. Ensure no stale single-vCPU timer remains.
        self.disarm_preemption_timer();

        info!("VM restored from snapshot (BSP RIP={:#x})", snapshot.rip());

        if let Some(pages) = overlay_pages {
            self.pages_differing_from_base = pages;
            self.incremental_base_needs_reconcile = false;
        } else {
            // Full images do not identify the retained incremental base.
            self.pages_differing_from_base.clear();
            self.incremental_base_needs_reconcile = true;
        }

        // Adopt the restored guest table's provenance only after every restore
        // stage succeeds; a failed restore must not relabel the live machine.
        self.execution_descriptor = snapshot
            .execution_descriptor()
            .map_err(|error| VmError::Snapshot {
                message: format!("restored execution descriptor: {error:?}"),
            })?
            .clone();

        Ok(())
    }

    /// Revert inherited overlay pages and all intervening guest writes, then
    /// apply the target overlay. No capture between sibling restores is needed.
    ///
    /// **Precondition:** `base` is the retained bootstrap image. Establish it
    /// with a full `restore()` before the first incremental restore, or follow
    /// a prior incremental restore on the same VM and base.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn restore_incremental(
        &mut self,
        snapshot: &crate::snapshot::VmSnapshot,
        base: &[u8],
    ) -> Result<(), VmError> {
        self.assertion_hits.clear();
        self.protocol_observations.reset();
        self.assertion_hit_overflow = 0;
        self.assertion_hit_stop_pending = false;
        self.validate_fault_engine_snapshot(snapshot)?;
        if base.len() != self.memory.size() {
            return Err(VmError::Snapshot {
                message: "incremental restore base size differs from guest memory".to_string(),
            });
        }
        let mut phase = SnapshotRestorePhase::Quiesce;
        match self.restore_incremental_validated(snapshot, base, &mut phase) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.latch_schedule_execution_poison(
                    SchedulePoisonStage::SnapshotRestore,
                    error.to_string(),
                );
                Err(VmError::SnapshotRestoreFailed {
                    phase,
                    detail: error.to_string(),
                })
            }
        }
    }

    fn restore_incremental_validated(
        &mut self,
        snapshot: &crate::snapshot::VmSnapshot,
        base: &[u8],
        phase: &mut SnapshotRestorePhase,
    ) -> Result<(), VmError> {
        use crate::snapshot::SnapshotMemory;

        self.complete_pending_kvm_exits_for_snapshot()?;
        *phase = SnapshotRestorePhase::Cpuid;
        snapshot
            .restore_cpuid(&self.vcpus)
            .map_err(|error| VmError::Snapshot {
                message: format!("restore guest CPUID: {error}"),
            })?;
        *phase = SnapshotRestorePhase::MemoryTracking;
        self.reconcile_incremental_base(base)?;

        let dirty_bitmap = self.read_guest_dirty_bitmap()?;
        let overlay_pages = match &snapshot.memory {
            SnapshotMemory::Overlay { dirty_pages, .. } => Some(dirty_pages),
            SnapshotMemory::Full(_) => None,
        };
        let page_plan = crate::vm_core::plan_incremental_restore(
            std::mem::take(&mut self.pages_differing_from_base),
            dirty_bitmap,
            overlay_pages
                .into_iter()
                .flat_map(|pages| pages.keys().copied()),
            base.len().div_ceil(crate::snapshot::PAGE_SIZE),
        )
        .map_err(|error| VmError::Snapshot {
            message: format!("incremental restore page plan: {error:?}"),
        })?;
        *phase = SnapshotRestorePhase::MemoryAndKernelState;
        SnapshotMemory::revert_pages_from_base(
            base,
            crate::vm_core::dirty_page_indices(&page_plan.revert),
            self.memory.inner(),
        )
        .map_err(|error| VmError::Snapshot {
            message: format!("revert dirty pages: {error}"),
        })?;
        snapshot
            .memory
            .write_to_guest(self.memory.inner())
            .map_err(|error| VmError::Snapshot {
                message: format!("write incremental snapshot memory: {error}"),
            })?;
        self.pages_differing_from_base = page_plan.next;
        self.incremental_base_needs_reconcile = matches!(snapshot.memory, SnapshotMemory::Full(_));

        // Step 3: Restore KVM device state (registers, IRQ chips, etc.)
        // Same as VmSnapshot::restore minus the memory write.
        snapshot
            .restore_devices_only_after_cpuid(&self.vcpus, &self.vm)
            .map_err(|e| {
                SnapshotSnafu {
                    message: e.to_string(),
                }
                .build()
            })?;

        *phase = SnapshotRestorePhase::Determinism;
        // Step 4: Restore VMM-side state (same as full restore).
        self.entropy = DeterministicEntropy::restore(&snapshot.entropy);
        self.virtual_tsc.set(snapshot.virtual_tsc);
        self.virtual_tsc.set_frozen(false);
        self.virtual_tsc.set_jitter_bound(0);
        self.forced_stalled_vcpus.clear();
        self.memory_ceiling_bytes =
            u64::try_from(self.memory.size()).map_err(|_| VmError::Snapshot {
                message: "guest memory size exceeds resource-observation bounds".to_string(),
            })?;
        self.exit_count = snapshot.exit_count;
        self.diagnostic_controller_tick = None;
        self.io_exit_count = snapshot.io_exit_count;
        self.exits_since_last_sdk = snapshot.exits_since_last_sdk;
        self.panic_detected = snapshot.panic_detected;
        self.panic_match_state = snapshot.panic_match_state;
        self.pit = DeterministicPit::restore(&snapshot.pit_snapshot);
        self.last_kvm_pit_mode = snapshot.last_kvm_pit_mode;
        self.fault_engine
            .restore(&snapshot.fault_engine_snapshot)
            .map_err(|error| {
                SnapshotSnafu {
                    message: format!("invalid fault-engine snapshot: {error}"),
                }
                .build()
            })?;
        self.coverage_active = snapshot.coverage_active;
        *phase = SnapshotRestorePhase::Scheduler;
        let previous_active_vcpu = self.active_vcpu;
        self.disable_singlestep(previous_active_vcpu)?;
        self.scheduler.restore(&snapshot.scheduler_snapshot)?;
        self.injection_ledger = snapshot.boundary_state.injection_ledger.clone();
        self.injection_records.clear();
        self.hlt_latched_vcpus = Self::hlt_latches_from_snapshot(snapshot);
        self.active_vcpu = self.scheduler.active();
        self.configure_progress_source_for_current_turn()?;

        *phase = SnapshotRestorePhase::Virtio;
        self.restore_virtio_snapshot(snapshot)?;

        *phase = SnapshotRestorePhase::Serial;
        let serial_trigger = SerialTrigger::default();
        self.serial_writer = CapturingWriter::new();
        self.serial = vm_superio::Serial::from_state(
            &snapshot.serial_state,
            serial_trigger,
            vm_superio::serial::NoEvents,
            self.serial_writer.clone(),
        )
        .map_err(|e| {
            SnapshotSnafu {
                message: format!("serial restore: {e}"),
            }
            .build()
        })?;
        // Pending UART identification is not a second interrupt edge.
        self.serial.interrupt_evt().0.set(false);

        self.dlog_emit(
            self.dlog_record(DlogTag::SnapshotRestored)
                .with_data_u64(snapshot.exit_count),
        );
        self.dlog_flush();

        self.disarm_preemption_timer();

        self.execution_descriptor = snapshot
            .execution_descriptor()
            .map_err(|error| VmError::Snapshot {
                message: format!("restored execution descriptor: {error:?}"),
            })?
            .clone();

        Ok(())
    }

    // ─── Internal: VM exit handling ──────────────────────────────────

    /// Deliver virtual-time IRQ 0, mirroring the inhibited kernel PIT only
    /// in the legacy single-vCPU profile. Coarse PIO programs software directly.
    /// Returns whether a software IRQ 0 edge was delivered.
    fn sync_and_suppress_pit(&mut self) -> Result<bool, VmError> {
        let current_tsc = self.virtual_tsc.read();
        if self.coarse_mode() {
            return self.deliver_virtual_pit_irq(current_tsc);
        }
        let mut pit_state = self.vm.get_pit2().context(CreatePitSnafu)?;

        // ── Channel 0: mirror config + suppress KVM timer ──────────
        let ch0 = &pit_state.channels[0];
        let reload = ch0.count as u16;
        let mode = ch0.mode;
        if mode <= 5
            && ch0.gate != 0
            && (reload != self.pit.channel_reload(0) || mode != self.last_kvm_pit_mode)
        {
            // Program our DeterministicPit with the same config
            let cmd = 0x30 | ((mode & 0x7) << 1);
            self.pit.write_port(0x43, cmd, current_tsc);
            self.pit.write_port(0x40, reload as u8, current_tsc);
            self.pit.write_port(0x40, (reload >> 8) as u8, current_tsc);
            self.last_kvm_pit_mode = mode;
        }
        // KVM's create_pit_timer() uses ktime_get() + reload period, not
        // channel 0's count_load_time. HPET_LEGACY both cancels an existing
        // timer on transition and prevents guest reloads/SET_PIT2 rearming it.
        // See Linux arch/x86/kvm/i8254.c, kvm_vm_ioctl_set_pit2().
        pit_state.flags |= KVM_PIT_FLAGS_HPET_LEGACY;

        // ── Channel 2: freeze for deterministic calibration ────────
        // The kernel uses channel 2 for TSC calibration, reading port 0x42.
        // KVM's in-kernel PIT computes elapsed time via
        //   ktime_get() - count_load_time
        // which depends on real wall-clock time (non-deterministic).
        //
        // Fix: push count_load_time into the far future so the elapsed
        // time is always negative → KVM clamps to 0 → counter always
        // reads its initial (reload) value. This makes the "fast TSC
        // calibration" read a frozen counter, producing a deterministic
        // result. The kernel falls back to a fixed lpj (set via cmdline).
        //
        // Mirror channel 2 config changes to our software PIT for
        // snapshot consistency.
        let ch2 = &pit_state.channels[2];
        let ch2_reload = ch2.count as u16;
        if ch2.gate != 0 && ch2_reload != self.pit.channel_reload(2) {
            let ch2_mode = ch2.mode;
            let cmd = 0x80 | 0x30 | ((ch2_mode & 0x7) << 1);
            self.pit.write_port(0x43, cmd, current_tsc);
            self.pit.write_port(0x42, ch2_reload as u8, current_tsc);
            self.pit
                .write_port(0x42, (ch2_reload >> 8) as u8, current_tsc);
        }
        // Freeze channel 2: set count_load_time far in the future.
        pit_state.channels[2].count_load_time = i64::MAX / 2;

        self.vm.set_pit2(&pit_state).context(CreatePitSnafu)?;

        self.deliver_virtual_pit_irq(current_tsc)
    }

    fn deliver_virtual_pit_irq(&mut self, current_tsc: u64) -> Result<bool, VmError> {
        if self.pit.pending_irq(current_tsc) {
            self.inject_irq_from(PIT_IRQ, crate::snapshot::InjectionSource::Pit)?;
            self.pit.acknowledge_irq();
            return Ok(true);
        }
        Ok(false)
    }

    fn hlt_latches_from_snapshot(snapshot: &crate::snapshot::VmSnapshot) -> Vec<bool> {
        snapshot.hlt_latched_vcpus.clone()
    }

    /// A software HLT latch is released only by an architecturally deliverable
    /// event. KVM leaves MP_STATE_RUNNABLE after single-stepped HLT, so MP state
    /// alone cannot distinguish idle from an interrupt-woken processor.
    fn vcpu_has_hlt_wake(&self, vcpu: usize) -> Result<bool, VmError> {
        let fd = &self.vcpus[vcpu];
        let events = fd.get_vcpu_events().context(SetRegistersSnafu)?;
        if events.nmi.injected != 0
            || (events.nmi.pending != 0 && events.nmi.masked == 0 && events.interrupt.shadow == 0)
        {
            return Ok(true);
        }
        let regs = fd.get_regs().context(SetRegistersSnafu)?;
        if regs.rflags & (1 << 9) == 0 || events.interrupt.shadow != 0 {
            return Ok(false);
        }
        if events.interrupt.injected != 0 {
            return Ok(true);
        }
        let lapic = fd.get_lapic().context(SetRegistersSnafu)?;
        let register = |offset: usize| {
            u32::from_le_bytes(std::array::from_fn(|i| lapic.regs[offset + i] as u8))
        };
        let sregs = fd.get_sregs().context(SetRegistersSnafu)?;
        let hardware_enabled = sregs.apic_base & (1 << 11) != 0;
        if hardware_enabled && register(0xf0) & (1 << 8) != 0 {
            // PPR is derived from TPR and the highest in-service vector;
            // recompute it rather than trusting KVM's lazily updated PPR image.
            let highest_vector = |base: usize| {
                (0..8).rev().find_map(|bank| {
                    let bits = register(base + bank * 16);
                    (bits != 0).then(|| bank as u32 * 32 + 31 - bits.leading_zeros())
                })
            };
            let priority = (register(0x80) & 0xf0).max(highest_vector(0x100).unwrap_or(0) & 0xf0);
            if highest_vector(0x200).is_some_and(|vector| vector & 0xf0 > priority) {
                return Ok(true);
            }
        }
        let lint0 = register(0x350);
        let accepts_pic = !hardware_enabled || lint0 & ((1 << 16) | (7 << 8)) == 7 << 8;
        if vcpu == BSP_VCPU_INDEX && accepts_pic {
            let mut chip = kvm_bindings::kvm_irqchip {
                chip_id: kvm_bindings::KVM_IRQCHIP_PIC_MASTER,
                ..Default::default()
            };
            self.vm.get_irqchip(&mut chip).context(CreateIrqChipSnafu)?;
            // SAFETY: the master PIC chip identity selects this ABI arm.
            let pic = unsafe { chip.chip.pic };
            let priority = |mask: u8| {
                (0..8u8)
                    .find(|offset| mask & (1 << ((offset + pic.priority_add) & 7)) != 0)
                    .unwrap_or(8)
            };
            let in_service = if pic.special_mask != 0 {
                pic.isr & !pic.imr
            } else {
                pic.isr
            };
            return Ok(priority(pic.irr & !pic.imr) < priority(in_service));
        }
        Ok(false)
    }

    fn refresh_hlt_wakes(&mut self) -> Result<(), VmError> {
        for vcpu in 0..self.vcpus.len() {
            if self.hlt_latched_vcpus[vcpu]
                && !self.forced_stalled_vcpus.contains(&vcpu)
                && self.vcpu_has_hlt_wake(vcpu)?
            {
                self.record_explicit_vcpu_wake(vcpu)?;
            }
        }
        Ok(())
    }

    fn observe_runnable_vcpus(&self) -> Result<Vec<bool>, VmError> {
        self.vcpus
            .iter()
            .enumerate()
            .map(|(vcpu, fd)| {
                let state = fd.get_mp_state().map_err(|error| VmError::Snapshot {
                    message: format!("failed to read vCPU {vcpu} MP state: {error}"),
                })?;
                // KVM consumes an in-kernel HLT wake only on the next RUN.
                let kvm_runnable = Self::mp_state_is_runnable(state.mp_state)
                    || (state.mp_state == kvm_bindings::KVM_MP_STATE_HALTED
                        && self.vcpu_has_hlt_wake(vcpu)?);
                Ok(kvm_runnable
                    && !self.hlt_latched_vcpus[vcpu]
                    && !self.forced_stalled_vcpus.contains(&vcpu))
            })
            .collect()
    }

    fn refresh_runnable_state(&mut self) -> Result<bool, VmError> {
        if self.vcpus.len() <= 1 {
            return Ok(false);
        }
        let observed = self.observe_runnable_vcpus()?;
        let changes =
            canonical_runnable_changes(&self.scheduler.state().runnable_vcpus, &observed)?;
        if changes.is_empty() {
            return Ok(false);
        }

        let reservation = self.scheduler.reserve_transition()?;
        let event = ScheduleEvent::RunnableObservation {
            expected_state_id: self.scheduler.state_id(),
            runnable_changes: changes,
        };
        let planned = self.scheduler.plan(&event)?;
        let action = planned.record.action.clone();
        let record = self.scheduler.commit(reservation, planned)?;
        self.emit_schedule_record(&record);
        let action_result = self.apply_schedule_action(&action);
        self.finish_after_committed_progress(SchedulePoisonStage::ScheduleAction, action_result)
    }

    fn ensure_schedule_execution_healthy(&self) -> Result<(), VmError> {
        if let Some(poison) = &self.schedule_execution_poison {
            return Err(VmError::ScheduleExecutionPoisoned {
                stage: poison.stage,
                detail: poison.detail.clone(),
            });
        }
        Ok(())
    }

    fn poison_after_possible_guest_progress(
        &mut self,
        reservation: Option<ScheduleReservation>,
        stage: SchedulePoisonStage,
        detail: impl Into<String>,
    ) -> VmError {
        debug_assert!(self.vcpus.len() > 1);
        debug_assert!(reservation.is_some() || self.scheduler.reservation_outstanding());
        debug_assert!(self.scheduler.reservation_outstanding());
        let plan = plan_poison();
        let _consumed_reservation_token = reservation;
        if plan.clear_pending_reservation {
            self.pending_schedule_reservation = None;
        }
        debug_assert!(plan.latch_permanent_poison);
        self.latch_schedule_execution_poison(stage, detail)
    }

    fn poison_after_committed_progress(
        &mut self,
        stage: SchedulePoisonStage,
        detail: impl Into<String>,
    ) -> VmError {
        let plan = plan_poison();
        if plan.clear_pending_reservation {
            self.pending_schedule_reservation = None;
        }
        debug_assert!(plan.latch_permanent_poison);
        self.latch_schedule_execution_poison(stage, detail)
    }

    fn latch_schedule_execution_poison(
        &mut self,
        stage: SchedulePoisonStage,
        detail: impl Into<String>,
    ) -> VmError {
        let poison = ScheduleExecutionPoison {
            stage,
            detail: detail.into(),
        };
        self.schedule_execution_poison = Some(poison.clone());
        VmError::ScheduleExecutionPoisoned {
            stage: poison.stage,
            detail: poison.detail,
        }
    }

    /// Return the permanent schedule poison after losing verified progress evidence.
    pub fn schedule_execution_poison(&self) -> Option<&ScheduleExecutionPoison> {
        self.schedule_execution_poison.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn inject_controller_test_progress(&mut self) -> Result<(), VmError> {
        let vcpu = self.scheduler.active();
        let observed_progress = self.scheduler.state().progress_counts()[vcpu]
            .checked_add(1)
            .ok_or_else(|| VmError::Snapshot {
                message: "test schedule progress exhausted".to_string(),
            })?;
        let reservation = self.scheduler.reserve_transition()?;
        let event = ScheduleEvent::InstructionProgress {
            expected_state_id: self.scheduler.state_id(),
            vcpu,
            observed_progress,
            runnable_changes: Vec::new(),
            source: crate::scheduler::core::ProgressSource::ExactSingleStep,
        };
        let planned = self.scheduler.plan(&event)?;
        self.scheduler.commit(reservation, planned)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn inject_controller_test_poison(&mut self) -> Result<VmError, VmError> {
        let reservation = self.scheduler.reserve_transition()?;
        Ok(self.poison_after_possible_guest_progress(
            Some(reservation),
            SchedulePoisonStage::JournalCommit,
            "injected VM poison after another VM advanced",
        ))
    }

    #[cfg(test)]
    pub(crate) fn controller_test_schedule_sequence(&self) -> u64 {
        self.scheduler.state().sequence
    }

    fn reserve_guest_progress(&mut self) -> Result<Option<ScheduleReservation>, VmError> {
        if self.vcpus.len() <= 1 {
            return Ok(None);
        }
        if let Some(reservation) = self.pending_schedule_reservation.take() {
            return Ok(Some(reservation));
        }
        Ok(Some(self.scheduler.reserve_transition()?))
    }

    fn release_guest_progress(
        &mut self,
        reservation: Option<ScheduleReservation>,
    ) -> Result<(), VmError> {
        if let Some(reservation) = reservation {
            self.scheduler.release_transition(reservation)?;
        }
        Ok(())
    }

    fn emit_schedule_record(&mut self, record: &ScheduleTransitionRecord) {
        let tag = if matches!(record.action, ScheduleAction::Switch { .. }) {
            DlogTag::SchedulerSwitch
        } else {
            DlogTag::ScheduleProgress
        };
        self.dlog_emit(
            self.dlog_record(tag)
                .with_data(&record.pre_state_id.0[..8])
                .with_extra(&record.post_state_id.0[..8]),
        );
    }

    fn finish_after_committed_progress<T>(
        &mut self,
        stage: SchedulePoisonStage,
        result: Result<T, VmError>,
    ) -> Result<T, VmError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => Err(self.poison_after_committed_progress(stage, error.to_string())),
        }
    }

    fn apply_schedule_action(&mut self, action: &ScheduleAction) -> Result<bool, VmError> {
        match action {
            ScheduleAction::Continue => Ok(false),
            ScheduleAction::EnterExactStep { .. } => {
                self.enable_singlestep(self.scheduler.active())?;
                Ok(false)
            }
            ScheduleAction::Switch { to_vcpu, .. } => {
                self.disable_singlestep(self.active_vcpu)?;
                self.active_vcpu = *to_vcpu;
                self.configure_progress_source_for_current_turn()?;
                Ok(false)
            }
            ScheduleAction::Halt => Ok(true),
        }
    }

    fn handle_hlt_exit(&mut self) -> Result<(bool, DlogRecord, Option<usize>), VmError> {
        self.exit_count += 1;
        self.exits_since_last_sdk += 1;

        let (halted, wake_vcpu) = if self.vcpus.len() == 1 {
            // KVM consumes HLT internally with an in-kernel LAPIC. Whether
            // observed directly or through MP_STATE_HALTED after a host
            // interrupt, only the architectural halt permits this time jump.
            // Mirror guest PIT programming that occurred within KVM_RUN.
            let mut delivered = self.sync_and_suppress_pit()?;
            if !delivered {
                if let Some(deadline) = self.pit.next_irq_tsc() {
                    // Generic advance_to rounds up to an exit-time quantum.
                    // Architectural idle instead reaches this exact deadline.
                    if !self.virtual_tsc.is_frozen() {
                        self.virtual_tsc.set(self.virtual_tsc.read().max(deadline));
                    }
                    delivered = self.sync_and_suppress_pit()?;
                    if delivered {
                        log::debug!(
                            "HLT woken by PIT deadline={} tsc={}",
                            deadline,
                            self.virtual_tsc.read()
                        );
                    }
                }
            }
            (!delivered, delivered.then_some(BSP_VCPU_INDEX))
        } else {
            self.virtual_tsc.tick();
            let delivered = self.sync_and_suppress_pit()?;
            // An idle AP must not accelerate the clock while another vCPU is
            // runnable. Only whole-machine idle permits a PIT deadline jump.
            if self.scheduler.state().halted && !delivered {
                if let Some(deadline) = self.pit.next_irq_tsc() {
                    if !self.virtual_tsc.is_frozen() {
                        self.virtual_tsc.set(self.virtual_tsc.read().max(deadline));
                    }
                    self.sync_and_suppress_pit()?;
                }
            }
            // The committed schedule action owns the halted verdict; pending
            // architectural interrupts are observed after applying that action.
            (false, None)
        };
        if halted {
            info!(
                "VM halted (exit_count={}, vtsc={})",
                self.exit_count,
                self.virtual_tsc.read()
            );
        }
        let record = DlogRecord::new(
            0,
            self.virtual_tsc.read(),
            self.exit_count,
            0,
            DlogTag::Hlt,
            self.active_vcpu as u8,
        );
        Ok((halted, record, wake_vcpu))
    }

    fn record_explicit_vcpu_wake(&mut self, vcpu: usize) -> Result<bool, VmError> {
        if vcpu >= self.vcpus.len() {
            return Err(VmError::Snapshot {
                message: format!("explicit wake names invalid vCPU {vcpu}"),
            });
        }
        if !self.hlt_latched_vcpus[vcpu] {
            return Ok(false);
        }
        let reservation = self.scheduler.reserve_transition()?;
        let event = ScheduleEvent::RunnableObservation {
            expected_state_id: self.scheduler.state_id(),
            runnable_changes: vec![crate::scheduler::core::RunnableChange {
                vcpu,
                runnable: true,
            }],
        };
        let planned = self.scheduler.plan(&event)?;
        let action = planned.record.action.clone();
        let record = self.scheduler.commit(reservation, planned)?;
        self.hlt_latched_vcpus[vcpu] = false;
        self.emit_schedule_record(&record);
        self.apply_schedule_action(&action)
    }

    /// Arm the single-vCPU operational watchdog.
    ///
    /// A timeout can stop host execution. It cannot change deterministic SMP
    /// state, virtual time, or any guest crash verdict.
    fn arm_preemption_timer(&self, us: i64) {
        Self::set_preemption_timer(self.thread_timer.as_ref(), us);
    }

    fn set_preemption_timer(thread_timer: Option<&SendTimerId>, us: i64) {
        // Try thread-targeted timer first (works in parallel workers).
        if let Some(tid) = thread_timer {
            let ts = libc::itimerspec {
                it_interval: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                },
                it_value: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: if us > 0 { us * 1000 } else { 0 },
                },
            };
            // SAFETY: the explicit owner holds a valid timer_create result.
            unsafe {
                libc::timer_settime(tid.raw(), 0, &ts, std::ptr::null_mut());
            }
            return;
        }

        // Fallback: process-wide ITIMER_REAL (only safe for single-worker).
        let timer_spec = libc::itimerval {
            it_interval: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            it_value: libc::timeval {
                tv_sec: 0,
                tv_usec: us,
            },
        };
        // SAFETY: setitimer is safe with valid pointer; ITIMER_REAL sends SIGALRM.
        unsafe {
            libc::setitimer(libc::ITIMER_REAL, &timer_spec, std::ptr::null_mut());
        }
    }

    /// Disarm the preemption timer.
    fn disarm_preemption_timer(&self) {
        self.arm_preemption_timer(0);
    }

    /// Bind host-local execution helpers to the current worker thread.
    pub fn init_thread_timer(&mut self) -> Result<(), VmError> {
        let coarse = self.scheduler.progress_mode() == ProgressMode::KvmDebugBoundary;
        if self.vcpus.len() == 1 || coarse {
            Self::install_sigalrm_handler()?;
            // SAFETY: syscall returns the calling Linux thread identity.
            let tid = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
            if self.thread_timer_tid != Some(tid) {
                self.thread_timer = None;
                self.thread_timer_tid = None;
                // SAFETY: initialized ABI values; SIGEV_THREAD_ID targets this worker.
                unsafe {
                    let mut event: libc::sigevent = std::mem::zeroed();
                    event.sigev_notify = libc::SIGEV_THREAD_ID;
                    event.sigev_signo = libc::SIGALRM;
                    event.sigev_notify_thread_id = tid;
                    let mut timer_id: libc::timer_t = std::ptr::null_mut();
                    if libc::timer_create(libc::CLOCK_MONOTONIC, &mut event, &mut timer_id) == 0 {
                        self.thread_timer = Some(SendTimerId::from_created(timer_id));
                        self.thread_timer_tid = Some(tid);
                    } else if coarse {
                        return Err(VmError::ProgressCapability {
                            message: format!(
                                "coarse entry requires a thread-targeted watchdog: {}",
                                io::Error::last_os_error()
                            ),
                        });
                    }
                }
            }
            if coarse {
                let mut mask = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
                // SAFETY: queries the current mask without changing it.
                let result = unsafe {
                    libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), mask.as_mut_ptr())
                };
                if result != 0
                    || unsafe { libc::sigismember(mask.assume_init_ref(), libc::SIGALRM) } != 0
                {
                    return Err(VmError::ProgressCapability {
                        message:
                            "coarse watchdog requires unblocked SIGALRM on the executing thread"
                                .into(),
                    });
                }
            }
        }
        if self.vcpus.len() > 1
            && self.instruction_counter.is_none()
            && matches!(
                self.scheduler.progress_mode(),
                ProgressMode::PmuAccelerated { .. }
            )
        {
            self.configure_progress_source_for_current_turn()?;
        }
        Ok(())
    }

    fn arm_coarse_watchdog(&mut self) -> Result<(), VmError> {
        self.init_thread_timer()?;
        let timer = self
            .thread_timer
            .as_ref()
            .ok_or_else(|| VmError::ProgressCapability {
                message: "coarse entry has no admitted thread watchdog".into(),
            })?;
        let deadline_ns = self.watchdog_microseconds * 1000;
        let spec = libc::itimerspec {
            it_interval: libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: libc::timespec {
                tv_sec: deadline_ns / 1_000_000_000,
                tv_nsec: deadline_ns % 1_000_000_000,
            },
        };
        // SAFETY: timer owner and timespec remain valid throughout the call.
        if unsafe { libc::timer_settime(timer.raw(), 0, &spec, std::ptr::null_mut()) } != 0 {
            return Err(VmError::ProgressCapability {
                message: format!(
                    "cannot arm coarse entry watchdog: {}",
                    io::Error::last_os_error()
                ),
            });
        }
        Ok(())
    }

    fn enable_singlestep(&mut self, vcpu: usize) -> Result<(), VmError> {
        // KVM_GUESTDBG_SINGLESTEP is not a persistent instruction boundary:
        // POPF/IRET and interrupt entry can clear hardware TF. SET_GUEST_DEBUG
        // refreshes KVM's singlestep_rip and injects TF at this entry's RIP.
        // Re-arm every debug-boundary or exact RUN, including PMU remainder.
        Self::set_vcpu_singlestep(&self.vcpus[vcpu], true).map_err(|error| {
            VmError::ProgressCapability {
                message: format!("vCPU {vcpu} could not enable guest debug stepping: {error}"),
            }
        })?;
        self.singlestep_active = true;
        Ok(())
    }

    fn disable_singlestep(&mut self, vcpu: usize) -> Result<(), VmError> {
        if !self.singlestep_active {
            return Ok(());
        }
        Self::set_vcpu_singlestep(&self.vcpus[vcpu], false).map_err(|error| {
            VmError::ProgressCapability {
                message: format!("vCPU {vcpu} could not disable exact single-step: {error}"),
            }
        })?;
        self.singlestep_active = false;
        Ok(())
    }

    fn configure_progress_source_for_current_turn(&mut self) -> Result<(), VmError> {
        let active = self.scheduler.active();
        if self.coarse_mode() {
            return self.enable_singlestep(active);
        }
        let progress = self.scheduler.state().progress_counts()[active];
        self.pmu_counter_base_progress = progress;
        match (self.scheduler.progress_mode(), self.scheduler.exact_step()?) {
            (ProgressMode::ExactSingleStep, ExactStepState::Inactive) => Ok(()),
            (ProgressMode::PmuAccelerated { .. }, ExactStepState::Active { .. }) => {
                self.enable_singlestep(active)
            }
            (ProgressMode::PmuAccelerated { exact_step_margin }, ExactStepState::Inactive) => {
                self.disable_singlestep(active)?;
                let overflow_period = self
                    .scheduler
                    .remaining_budget()
                    .count()
                    .checked_sub(exact_step_margin)
                    .filter(|period| *period > 0)
                    .ok_or_else(|| VmError::ProgressCapability {
                        message: "PMU overflow period must be positive".to_string(),
                    })?;
                let counter = crate::perf::InstructionCounter::with_overflow(overflow_period)
                    .map_err(|error| VmError::ProgressCapability {
                        message: format!(
                            "guest-only instruction PMU overflow became unavailable: {error}"
                        ),
                    })?;
                counter.reset_and_enable()?;
                counter.disable()?;
                self.instruction_counter = Some(counter);
                Ok(())
            }
            (ProgressMode::LegacyWallClock, _) => {
                Err(ScheduleError::UnsupportedProgressMode.into())
            }
            (ProgressMode::KvmDebugBoundary, _) => unreachable!("coarse mode handled separately"),
            (ProgressMode::ExactSingleStep, ExactStepState::Active { .. }) => {
                Err(VmError::ProgressCapability {
                    message: "exact single-step mode has invalid PMU remainder state".to_string(),
                })
            }
        }
    }

    fn prepare_progress_source_for_run(&mut self, vcpu: usize) -> Result<Option<u64>, VmError> {
        // Single-vCPU execution is bounded by ordinary guest exits and its
        // operational watchdog, not the SMP retired-instruction scheduler.
        if self.num_vcpus() <= 1 {
            return Ok(None);
        }
        if self.coarse_mode() {
            self.validate_coarse_clock_devices()?;
            self.enable_singlestep(vcpu)?;
            return Ok(None);
        }
        match (self.scheduler.progress_mode(), self.scheduler.exact_step()?) {
            (ProgressMode::ExactSingleStep, ExactStepState::Inactive)
            | (ProgressMode::PmuAccelerated { .. }, ExactStepState::Active { .. }) => {
                self.enable_singlestep(vcpu)?;
                Ok(None)
            }
            (ProgressMode::PmuAccelerated { .. }, ExactStepState::Inactive) => {
                let counter = self.instruction_counter.as_ref().ok_or_else(|| {
                    VmError::ProgressCapability {
                        message: "PMU mode has no admitted instruction counter".to_string(),
                    }
                })?;
                // Block and drain stale signals before this fd is armed.
                let generation = counter.arm_overflow()?;
                Ok(Some(generation))
            }
            (ProgressMode::LegacyWallClock, _) => {
                Err(ScheduleError::UnsupportedProgressMode.into())
            }
            (ProgressMode::KvmDebugBoundary, _) => unreachable!("coarse mode handled separately"),
            (ProgressMode::ExactSingleStep, ExactStepState::Active { .. }) => {
                Err(VmError::ProgressCapability {
                    message: "exact single-step mode has invalid PMU remainder state".to_string(),
                })
            }
        }
    }

    fn step(&mut self) -> Result<bool, VmError> {
        self.ensure_schedule_execution_healthy()?;
        let result = self.step_inner();
        if self.schedule_entry_in_flight {
            if let Err(error) = &result {
                if self.schedule_execution_poison.is_none() {
                    self.poison_after_committed_progress(
                        SchedulePoisonStage::KvmRunResult,
                        error.to_string(),
                    );
                }
            }
            self.schedule_entry_in_flight = false;
        }
        result
    }

    fn step_inner(&mut self) -> Result<bool, VmError> {
        self.ensure_schedule_execution_healthy()?;
        self.sync_and_suppress_pit()?;

        let num_vcpus = self.vcpus.len();
        let coarse = self.coarse_mode();
        if self.pending_schedule_reservation.is_none() {
            self.refresh_hlt_wakes()?;
            if self.refresh_runnable_state()? {
                return Ok(true);
            }
        }
        self.sync_tsc_to_guest()?;
        debug_assert_eq!(self.active_vcpu, self.scheduler.active());
        if self.vcpus.len() > 1
            && matches!(
                self.scheduler.progress_mode(),
                ProgressMode::PmuAccelerated { .. }
            )
            && self.instruction_counter.is_none()
        {
            self.configure_progress_source_for_current_turn()?;
        }
        let executed_vcpu = self.active_vcpu;
        let mut reservation = self.reserve_guest_progress()?;
        let pmu_generation_before_run = match self.prepare_progress_source_for_run(executed_vcpu) {
            Ok(generation) => generation,
            Err(error) => {
                self.release_guest_progress(reservation.take())?;
                return Err(error);
            }
        };

        // Post-match action flag: when a VMCALL exit is processed, we
        // must handle the SDK hypercall after the KVM exit borrow closes.
        let mut vmcall_sdk_pending = false;

        // Bootstrap can halt inside KVM too. The operational timer returns
        // control so MP_STATE_HALTED can be observed; elapsed host time never
        // determines a virtual deadline or an IRQ. Runnable timeouts retain
        // their existing operational-error meaning, before and after setup.
        // Coarse SMP uses a separately checked thread watchdog for abort only.
        if num_vcpus == 1 {
            Self::install_sigalrm_handler()?;
            self.arm_preemption_timer(self.watchdog_microseconds);
        }
        if coarse {
            if let Err(error) = self.arm_coarse_watchdog() {
                self.release_guest_progress(reservation.take())?;
                return Err(error);
            }
        }
        self.schedule_entry_in_flight = num_vcpus > 1;
        let run_result = self.vcpus[executed_vcpu].run().or_else(|error| {
            if error.errno() == libc::EINTR {
                Ok(VcpuExit::Intr)
            } else {
                Err(error)
            }
        });
        let coarse_watchdog_expired = if coarse {
            Self::coarse_watchdog_deadline_elapsed(self.thread_timer.as_ref())?
        } else {
            false
        };
        if coarse {
            Self::disarm_coarse_watchdog(self.thread_timer.as_ref())?;
        } else if num_vcpus == 1 {
            Self::set_preemption_timer(self.thread_timer.as_ref(), 0);
        }
        let pmu_overflow_attributed = if let Some(generation) = pmu_generation_before_run {
            let disarm_result = self
                .instruction_counter
                .as_ref()
                .ok_or_else(|| io::Error::other("PMU mode lost its admitted instruction counter"))
                .and_then(|counter| counter.disarm_overflow(generation));
            match disarm_result {
                Ok(attributed) => attributed,
                Err(error) => {
                    let stage = if error.to_string().starts_with("PMU disable failed:") {
                        SchedulePoisonStage::PmuDisable
                    } else {
                        SchedulePoisonStage::PmuGeneration
                    };
                    return Err(self.poison_after_possible_guest_progress(
                        reservation.take(),
                        stage,
                        error.to_string(),
                    ));
                }
            }
        } else {
            false
        };

        let interrupted = match &run_result {
            Ok(VcpuExit::Intr) => true,
            Err(error) => error.errno() == libc::EINTR,
            Ok(_) => false,
        };
        let coarse_boundary = if coarse {
            if interrupted && coarse_watchdog_expired {
                return Err(VmError::HostWatchdogTimeout {
                    vcpu: executed_vcpu,
                });
            }
            Some(Self::normalize_coarse_exit(&run_result, executed_vcpu)?)
        } else {
            None
        };
        let guest_progress_observed = matches!(
            &run_result,
            Ok(VcpuExit::IoIn(_, _))
                | Ok(VcpuExit::IoOut(_, _))
                | Ok(VcpuExit::Hlt)
                | Ok(VcpuExit::Shutdown)
                | Ok(VcpuExit::MmioRead(_, _))
                | Ok(VcpuExit::MmioWrite(_, _))
                | Ok(VcpuExit::Hypercall(_))
                | Ok(VcpuExit::Debug(_))
        );
        let exact_progress_boundary_missing = !interrupted && !guest_progress_observed;
        if guest_progress_observed {
            self.host_watchdog_retries = 0;
        }
        if num_vcpus > 1 && exact_progress_boundary_missing {
            return Err(self.poison_after_possible_guest_progress(
                reservation.take(),
                SchedulePoisonStage::KvmRunResult,
                "KVM returned without an exact retired-instruction boundary",
            ));
        }
        let retired_hlt = matches!(&run_result, Ok(VcpuExit::Hlt));
        let progress_observation = if num_vcpus <= 1 || coarse || exact_progress_boundary_missing {
            ExecutionProgressObservation::NoGuestProgress
        } else if interrupted && !pmu_overflow_attributed {
            ExecutionProgressObservation::HostInterrupt
        } else if interrupted {
            let read_result = self
                .instruction_counter
                .as_ref()
                .ok_or_else(|| io::Error::other("attributed PMU overflow has no counter"))
                .and_then(crate::perf::InstructionCounter::read);
            let counter_value = match read_result {
                Ok(value) => value,
                Err(error) => {
                    return Err(self.poison_after_possible_guest_progress(
                        reservation.take(),
                        SchedulePoisonStage::PmuRead,
                        error.to_string(),
                    ));
                }
            };
            ExecutionProgressObservation::PmuInterrupt {
                vcpu: executed_vcpu,
                counter_base_progress: self.pmu_counter_base_progress,
                counter_value,
            }
        } else {
            match (self.scheduler.progress_mode(), self.scheduler.exact_step()?) {
                (ProgressMode::PmuAccelerated { .. }, ExactStepState::Inactive) => {
                    let read_result = self
                        .instruction_counter
                        .as_ref()
                        .ok_or_else(|| io::Error::other("PMU mode lost its admitted counter"))
                        .and_then(crate::perf::InstructionCounter::read);
                    let counter_value = match read_result {
                        Ok(value) => value,
                        Err(error) => {
                            return Err(self.poison_after_possible_guest_progress(
                                reservation.take(),
                                SchedulePoisonStage::PmuRead,
                                error.to_string(),
                            ));
                        }
                    };
                    if retired_hlt {
                        ExecutionProgressObservation::PmuGuestHalt {
                            vcpu: executed_vcpu,
                            counter_base_progress: self.pmu_counter_base_progress,
                            counter_value,
                        }
                    } else {
                        ExecutionProgressObservation::PmuGuestExit {
                            vcpu: executed_vcpu,
                            counter_base_progress: self.pmu_counter_base_progress,
                            counter_value,
                        }
                    }
                }
                _ if retired_hlt => ExecutionProgressObservation::ExactInstructionHalt {
                    vcpu: executed_vcpu,
                },
                _ => ExecutionProgressObservation::ExactInstruction {
                    vcpu: executed_vcpu,
                },
            }
        };
        let planned_transition = if num_vcpus <= 1 || coarse {
            None
        } else {
            match plan_execution_observation(self.scheduler.state(), progress_observation) {
                Ok(planned) => planned,
                Err(error) => {
                    return Err(self.poison_after_possible_guest_progress(
                        reservation.take(),
                        SchedulePoisonStage::ProgressPlanning,
                        format!("{error:?}"),
                    ));
                }
            }
        };
        let mut pending_schedule_record = if coarse {
            None
        } else {
            match (reservation.take(), planned_transition) {
                (Some(reservation), Some(planned)) => {
                    match self.scheduler.commit(reservation, planned) {
                        Ok(record) => Some(record),
                        Err(error) => {
                            return Err(self.poison_after_possible_guest_progress(
                                None,
                                SchedulePoisonStage::JournalCommit,
                                format!("{error:?}"),
                            ));
                        }
                    }
                }
                (Some(reservation), None)
                    if progress_observation == ExecutionProgressObservation::HostInterrupt =>
                {
                    self.pending_schedule_reservation = Some(reservation);
                    None
                }
                (Some(reservation), None) => {
                    return Err(self.poison_after_possible_guest_progress(
                        Some(reservation),
                        SchedulePoisonStage::KvmRunResult,
                        "post-KVM execution produced no committable evidence",
                    ));
                }
                (None, None) => None,
                (None, Some(_)) => {
                    return Err(VmError::ProgressCapability {
                        message: "single-vCPU execution produced an SMP transition".to_string(),
                    });
                }
            }
        };
        if num_vcpus > 1 && retired_hlt && pending_schedule_record.is_some() {
            self.hlt_latched_vcpus[executed_vcpu] = true;
        }

        // Deferred dlog record — built inside the match arm (copying
        // data out of the KVM exit struct), emitted after the match
        // closes so there is no borrow conflict with `self.vcpus`.
        let mut pending_dlog: Option<DlogRecord> = None;
        let mut pending_explicit_wake = None;
        let mut pending_virtio_irq = None;

        let result = match run_result {
            Ok(VcpuExit::IoIn(port, data)) => {
                self.exit_count += 1;
                self.io_exit_count += 1;
                // SDK/coverage access resets idle counter; all other
                // exits increment it.  This counts total exits since
                // the last SDK interaction, regardless of exit type.
                if port == SDK_PORT || port == COVERAGE_PORT {
                    self.exits_since_last_sdk = 0;
                } else {
                    self.exits_since_last_sdk += 1;
                }
                self.virtual_tsc.tick();

                let tsc = self.virtual_tsc.read();
                if port == SDK_PORT {
                    // SDK hypercall result — guest reads status byte
                    data[0] = 0; // STATUS_OK
                } else if port == COVERAGE_PORT {
                    data[0] = if self.coverage_active { 1 } else { 0 };
                } else if (SERIAL_PORT_BASE..=SERIAL_PORT_END).contains(&port) {
                    let offset = (port - SERIAL_PORT_BASE) as u8;
                    data[0] = self.serial.read(offset);
                } else if DeterministicPit::handles_port(port) {
                    data[0] = self.pit.read_port(port, tsc);
                } else if port == ACPI_PM_TIMER_PORT {
                    // DETERMINISM: Return a deterministic PM timer value
                    // derived from our virtual TSC. The PM timer is a
                    // 24-bit counter at 3.579545 MHz. We convert virtual
                    // TSC ticks to PM timer ticks.
                    let tsc_khz = self.virtual_tsc.tsc_khz() as u64;
                    let pm_ticks = if tsc_khz > 0 {
                        (tsc as u128 * ACPI_PM_TIMER_FREQ_HZ as u128 / (tsc_khz as u128 * 1000))
                            as u32
                            & 0x00FF_FFFF // 24-bit wrap
                    } else {
                        0
                    };
                    let bytes = pm_ticks.to_le_bytes();
                    for (i, byte) in data.iter_mut().enumerate() {
                        *byte = if i < 4 { bytes[i] } else { 0 };
                    }
                } else {
                    for byte in data.iter_mut() {
                        *byte = 0xff;
                    }
                }
                if self.dlog.is_some() {
                    let mut dbuf = [0u8; 8];
                    let n = data.len().min(8);
                    dbuf[..n].copy_from_slice(&data[..n]);
                    pending_dlog = Some(
                        DlogRecord::new(
                            0,
                            self.virtual_tsc.read(),
                            self.exit_count,
                            0,
                            DlogTag::IoIn,
                            self.active_vcpu as u8,
                        )
                        .with_port(port)
                        .with_data(&dbuf),
                    );
                }
                Ok(false)
            }
            Ok(VcpuExit::IoOut(port, data)) => {
                self.exit_count += 1;
                self.io_exit_count += 1;
                // SDK/coverage access resets idle counter; all other
                // exits (including serial writes) increment it.
                if port == SDK_PORT || port == COVERAGE_PORT {
                    self.exits_since_last_sdk = 0;
                } else {
                    self.exits_since_last_sdk += 1;
                }
                self.virtual_tsc.tick();

                // Copy data and port before any &mut self calls — data
                // borrows from self.vcpus via the KVM run struct.
                let io_port = port;
                let io_byte = data[0];
                if self.dlog.is_some() {
                    pending_dlog = Some(
                        DlogRecord::new(
                            0,
                            self.virtual_tsc.read(),
                            self.exit_count,
                            0,
                            DlogTag::IoOut,
                            self.active_vcpu as u8,
                        )
                        .with_port(io_port)
                        .with_data(&[io_byte]),
                    );
                }

                let tsc = self.virtual_tsc.read();
                if io_port == SDK_PORT {
                    if coarse {
                        vmcall_sdk_pending = true;
                    } else {
                        self.handle_sdk_hypercall();
                    }
                } else if io_port == COVERAGE_PORT {
                    self.coverage_active = true;
                    log::info!("Coverage instrumentation activated by guest");
                } else if (SERIAL_PORT_BASE..=SERIAL_PORT_END).contains(&io_port) {
                    let offset = (io_port - SERIAL_PORT_BASE) as u8;
                    let _ = self.serial.write(offset, io_byte);
                    // Sliding window crash detection: shift in each
                    // serial byte and compare against multiple patterns.
                    if offset == 0 {
                        self.panic_match_state = (self.panic_match_state << 8) | (io_byte as u64);
                        if PANIC_PATTERNS.contains(&self.panic_match_state) {
                            self.panic_detected = true;
                        }
                    }
                } else if DeterministicPit::handles_port(io_port) {
                    self.pit.write_port(io_port, io_byte, tsc);
                }
                Ok(false)
            }
            Ok(VcpuExit::Hlt) if coarse => Ok(false),
            Ok(VcpuExit::Hlt) => match self.handle_hlt_exit() {
                Ok((halted, record, wake_vcpu)) => {
                    pending_dlog = Some(record);
                    pending_explicit_wake = wake_vcpu;
                    Ok(halted)
                }
                Err(error) => Err(error),
            },
            Ok(VcpuExit::Shutdown) => {
                self.exit_count += 1;
                self.panic_detected = true;
                pending_dlog = Some(DlogRecord::new(
                    0,
                    self.virtual_tsc.read(),
                    self.exit_count,
                    0,
                    DlogTag::Shutdown,
                    self.active_vcpu as u8,
                ));
                info!(
                    "VM shutdown/triple-fault — marking as crashed (exit_count={})",
                    self.exit_count
                );
                Ok(true)
            }
            Ok(VcpuExit::MmioRead(addr, data)) => {
                self.exit_count += 1;
                self.exits_since_last_sdk += 1;
                self.virtual_tsc.tick();

                // DETERMINISM: Trap HPET MMIO reads and return
                // deterministic values derived from virtual TSC.
                // Even with nohpet cmdline, a clever guest could
                // still read the HPET registers directly via MMIO.
                if (HPET_MMIO_BASE..HPET_MMIO_BASE + HPET_MMIO_SIZE).contains(&addr) {
                    let offset = addr - HPET_MMIO_BASE;
                    let value: u64 = match offset {
                        HPET_REG_CAP => {
                            // Capabilities: 1 timer, 64-bit, ~10ns period.
                            // Bits [31:16] = vendor ID (0x0000)
                            // Bit 13 = COUNTER_SIZE_CAP (1 = 64-bit)
                            // Bits [12:8] = NUM_TIM_CAP (0 = 1 timer)
                            // Bit 0 = REV_ID (1)
                            // Upper 32 bits = period in femtoseconds
                            // (~10ns = 10_000_000 fs for 100MHz)
                            let period_fs: u64 = 10_000_000;
                            (period_fs << 32) | (1 << 13) | 1
                        }
                        HPET_REG_CONFIG => {
                            // Config: disabled (ENABLE_CNF = 0).
                            // Guest sees HPET as present but stopped.
                            0
                        }
                        HPET_REG_COUNTER => {
                            // Main counter: deterministic value from vTSC.
                            // HPET at ~100 MHz = TSC / 30 at 3 GHz.
                            const HPET_FREQUENCY_KHZ: u64 = 100_000;
                            let tsc = self.virtual_tsc.read();
                            let tsc_khz = u64::from(self.virtual_tsc.tsc_khz());
                            tsc.saturating_mul(HPET_FREQUENCY_KHZ)
                                .checked_div(tsc_khz)
                                .unwrap_or_default()
                        }
                        _ => 0,
                    };
                    let bytes = value.to_le_bytes();
                    for (i, byte) in data.iter_mut().enumerate() {
                        *byte = if i < 8 { bytes[i] } else { 0 };
                    }
                } else {
                    // Find the virtio device that handles this address
                    let mut handled = false;
                    for dev in &self.virtio_devices {
                        if dev.handles(addr) {
                            let offset = addr - dev.base_addr();
                            dev.read(offset, data);
                            handled = true;
                            break;
                        }
                    }

                    if !handled {
                        // Unknown MMIO region — return zeros
                        for byte in &mut *data {
                            *byte = 0;
                        }
                    }
                }

                if self.dlog.is_some() {
                    pending_dlog = Some(
                        DlogRecord::new(
                            0,
                            self.virtual_tsc.read(),
                            self.exit_count,
                            0,
                            DlogTag::MmioRead,
                            self.active_vcpu as u8,
                        )
                        .with_mmio_addr(addr)
                        .with_data(data),
                    );
                }
                Ok(false)
            }
            Ok(VcpuExit::MmioWrite(addr, data)) => {
                self.exit_count += 1;
                self.exits_since_last_sdk += 1;
                self.virtual_tsc.tick();

                // Find the virtio device that handles this address.
                for (device_index, dev) in self.virtio_devices.iter_mut().enumerate() {
                    if dev.handles(addr) {
                        let effect = dev.write_at(addr, data, self.memory.inner());

                        // Process only the queue named by a validated notification.
                        if let Ok(MmioWriteEffect::NotifyQueue(queue_index)) = effect {
                            if dev.process_queue(queue_index, self.memory.inner()) {
                                if coarse {
                                    pending_virtio_irq = Some((device_index, queue_index));
                                } else {
                                    let vm = &self.vm;
                                    deliver_virtio_interrupt_with(
                                        dev,
                                        queue_index,
                                        |irq, asserted| vm.set_irq_line(irq, asserted),
                                    )?;
                                }
                            }
                        }
                        break;
                    }
                }

                if self.dlog.is_some() {
                    pending_dlog = Some(
                        DlogRecord::new(
                            0,
                            self.virtual_tsc.read(),
                            self.exit_count,
                            0,
                            DlogTag::MmioWrite,
                            self.active_vcpu as u8,
                        )
                        .with_mmio_addr(addr)
                        .with_data(data),
                    );
                }
                Ok(false)
            }
            Ok(VcpuExit::Intr) => {
                if num_vcpus == 1 {
                    let mp_state = self.vcpus[executed_vcpu].get_mp_state().map_err(|error| {
                        VmError::ProgressCapability {
                            message: format!(
                                "vCPU {executed_vcpu} halt observation failed: {error}"
                            ),
                        }
                    })?;
                    if mp_state.mp_state == kvm_bindings::KVM_MP_STATE_HALTED {
                        let (halted, record, wake_vcpu) = self.handle_hlt_exit()?;
                        pending_dlog = Some(record);
                        pending_explicit_wake = wake_vcpu;
                        Ok(halted)
                    } else {
                        // A runnable guest may compute without any KVM exit,
                        // and host scheduling can stretch that window. Retry
                        // the run; only an uninterrupted series of host timer
                        // interrupts with no guest exit is an operational
                        // timeout. Retries change no guest-visible state, exit
                        // count, or virtual time.
                        self.host_watchdog_retries = self.host_watchdog_retries.saturating_add(1);
                        if self.host_watchdog_retries > MAX_RUNNABLE_WATCHDOG_RETRIES {
                            Err(VmError::HostWatchdogTimeout {
                                vcpu: executed_vcpu,
                            })
                        } else {
                            Ok(false)
                        }
                    }
                } else {
                    // PMU overflow can produce an admitted deterministic
                    // progress record. Any other interrupt is a host-owned
                    // retry and changes no schedule, virtual time, or verdict.
                    Ok(false)
                }
            }
            Ok(VcpuExit::Hypercall(exit)) => {
                // VMCALL from guest — the primary SDK transport.
                // Guest wrote the HypercallPage and then executed `vmcall`
                // with RAX = VMCALL_NR. This is faster than port I/O and
                // is the canonical x86 guest→hypervisor instruction.
                //
                // Two-phase handling: HypercallExit holds a &mut ref into
                // the vcpu's kvm_run struct (for ret), preventing &mut self
                // calls. We set ret here and defer handle_sdk_hypercall()
                // to the post-match phase via vmcall_sdk_pending.
                self.exit_count += 1;
                self.io_exit_count += 1;
                self.exits_since_last_sdk = 0;
                self.virtual_tsc.tick();

                vmcall_sdk_pending = exit.nr == VMCALL_NR;
                *exit.ret = if vmcall_sdk_pending {
                    0
                } else {
                    (-libc::ENOSYS) as u64
                };
                {
                    let nr = exit.nr;
                    pending_dlog = Some(
                        DlogRecord::new(
                            0,
                            self.virtual_tsc.read(),
                            self.exit_count,
                            0,
                            DlogTag::Hypercall,
                            self.active_vcpu as u8,
                        )
                        .with_data_u64(nr),
                    );
                }
                Ok(false)
            }
            Ok(VcpuExit::Debug(_debug)) => {
                self.exit_count += 1;
                self.exits_since_last_sdk += 1;
                self.virtual_tsc.tick();
                pending_dlog = Some(DlogRecord::new(
                    0,
                    self.virtual_tsc.read(),
                    self.exit_count,
                    0,
                    DlogTag::Debug,
                    self.active_vcpu as u8,
                ));
                Ok(false)
            }
            Ok(VcpuExit::IrqWindowOpen) => {
                pending_dlog = Some(DlogRecord::new(
                    0,
                    self.virtual_tsc.read(),
                    self.exit_count,
                    0,
                    DlogTag::IrqWindowOpen,
                    self.active_vcpu as u8,
                ));
                Ok(false)
            }
            Ok(VcpuExit::InternalError) => Err(VmError::KvmInternalError {
                vcpu: executed_vcpu,
            }),
            Ok(exit) => {
                self.exit_count += 1;
                info!("Unhandled VM exit: {:?} — stopping", exit);
                Ok(true)
            }
            Err(error) => Err(VmError::VcpuRun { source: error }),
        };
        let result = result.and_then(|halted| {
            let mut halted = halted;
            if let Some(boundary) = coarse_boundary {
                if let KvmBoundaryKind::Pio { size, .. } = boundary {
                    let run = self.vcpus[executed_vcpu].get_kvm_run();
                    // SAFETY: the normalized exit was KVM_EXIT_IO, selecting
                    // the io union arm. Read before completion overwrites it.
                    let io = unsafe { run.__bindgen_anon_1.io };
                    if io.count != 1 || io.size != size {
                        return Err(VmError::ProgressCapability {
                            message: "repeated/ambiguous PIO is outside the v1 single-operation settlement table".into(),
                        });
                    }
                }
                if matches!(boundary, KvmBoundaryKind::Pio { .. } | KvmBoundaryKind::Mmio { .. }) {
                    self.settle_coarse_io(executed_vcpu)?;
                }
                self.validate_coarse_clock_devices()?;
                let planned = plan_execution_observation(self.scheduler.state(),
                    ExecutionProgressObservation::KvmBoundary {
                        vcpu: executed_vcpu,
                        profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
                        boundary,
                    })?.ok_or_else(|| VmError::ProgressCapability {
                        message: "settled boundary produced no canonical transition".into(),
                    })?;
                let token = reservation.take().ok_or_else(|| VmError::ProgressCapability {
                    message: "settled boundary lost its preflight journal reservation".into(),
                })?;
                pending_schedule_record = Some(self.scheduler.commit(token, planned)?);
                if matches!(boundary, KvmBoundaryKind::Halt) {
                    self.hlt_latched_vcpus[executed_vcpu] = true;
                    let (observed_halted, record, wake_vcpu) = self.handle_hlt_exit()?;
                    halted = observed_halted;
                    pending_dlog = Some(record);
                    pending_explicit_wake = wake_vcpu;
                }
            }
            if let Some((device_index, queue_index)) = pending_virtio_irq {
                self.deliver_coarse_virtio_irq(device_index, queue_index)?;
            }
            if self.serial.interrupt_evt().0.replace(false) {
                self.inject_irq_from(SERIAL_IRQ, crate::snapshot::InjectionSource::Serial)?;
            }
            Ok(halted)
        });
        let committed_smp_progress = num_vcpus > 1 && pending_schedule_record.is_some();
        let exit_halted = if committed_smp_progress {
            self.finish_after_committed_progress(SchedulePoisonStage::ExitHandling, result)?
        } else {
            result?
        };
        let pending_schedule_action = pending_schedule_record
            .as_ref()
            .map(|record| record.action.clone());

        // ── Post-match phase ──────────────────────────────────
        // Emit deferred dlog record now that the KVM borrow is released.
        // Enrich exit-type records with RSP[31:0] + RFLAGS[31:0] in
        // the extra field. This costs one get_regs() ioctl per exit
        // but only when dlog is enabled.
        if let Some(mut rec) = pending_dlog {
            if self.dlog.is_some() {
                let enrich = matches!(
                    rec.tag(),
                    Some(DlogTag::IoIn)
                        | Some(DlogTag::IoOut)
                        | Some(DlogTag::MmioRead)
                        | Some(DlogTag::MmioWrite)
                        | Some(DlogTag::Hlt)
                        | Some(DlogTag::Hypercall)
                );
                if enrich {
                    if let Ok(regs) = self.vcpus[self.active_vcpu].get_regs() {
                        rec.rip = regs.rip;
                        let rsp_lo = (regs.rsp as u32).to_le_bytes();
                        let rfl_lo = (regs.rflags as u32).to_le_bytes();
                        rec.extra = [
                            rsp_lo[0], rsp_lo[1], rsp_lo[2], rsp_lo[3], rfl_lo[0], rfl_lo[1],
                            rfl_lo[2], rfl_lo[3],
                        ];
                    }
                }
            }
            self.dlog_emit(rec);

            // Periodic full register dump (if configured).
            if self.dlog_register_interval > 0
                && self.exit_count.is_multiple_of(self.dlog_register_interval)
            {
                if let Ok(regs) = self.vcpus[self.active_vcpu].get_regs() {
                    let dump = DlogRecord::new(
                        0,
                        self.virtual_tsc.read(),
                        self.exit_count,
                        regs.rip,
                        DlogTag::RegisterDump,
                        self.active_vcpu as u8,
                    )
                    .with_data_u64(regs.rax)
                    .with_extra(&{
                        let rsp_lo = (regs.rsp as u32).to_le_bytes();
                        let rfl_lo = (regs.rflags as u32).to_le_bytes();
                        [
                            rsp_lo[0], rsp_lo[1], rsp_lo[2], rsp_lo[3], rfl_lo[0], rfl_lo[1],
                            rfl_lo[2], rfl_lo[3],
                        ]
                    });
                    self.dlog_emit(dump);
                }
            }
        }

        // Deferred from the Hypercall arm because HypercallExit held
        // a &mut ref into the vcpu's kvm_run struct, preventing &mut
        // self calls. Now that the match is closed, the borrow is
        // released and we can safely call handle_sdk_hypercall().
        if vmcall_sdk_pending {
            self.handle_sdk_hypercall();
            self.dlog_emit(self.dlog_record(DlogTag::SdkHypercall));
        }

        if let Some(record) = &pending_schedule_record {
            self.emit_schedule_record(record);
        }
        let mut schedule_halted = if let Some(action) = &pending_schedule_action {
            let action_result = self.apply_schedule_action(action);
            self.finish_after_committed_progress(
                SchedulePoisonStage::ScheduleAction,
                action_result,
            )?
        } else {
            false
        };
        if let Some(vcpu) = pending_explicit_wake {
            let wake_result = self.record_explicit_vcpu_wake(vcpu);
            self.finish_after_committed_progress(SchedulePoisonStage::ExitHandling, wake_result)?;
            schedule_halted &= self.scheduler.state().halted;
        }
        if num_vcpus > 1 && self.pending_schedule_reservation.is_none() {
            let wake_result = self.refresh_hlt_wakes();
            self.finish_after_committed_progress(SchedulePoisonStage::ExitHandling, wake_result)?;
            schedule_halted &= self.scheduler.state().halted;
        }

        // Serial panic detection: if the sliding window matched
        // "Kernel p" during this exit's serial output, treat the
        // VM as crashed immediately.
        if self.panic_detected {
            log::warn!(
                "Kernel panic detected via serial output (exit_count={}, vtsc={})",
                self.exit_count,
                self.virtual_tsc.read(),
            );
            return Ok(true);
        }

        Ok(exit_halted || schedule_halted)
    }

    // ─── Guest memory / register access ────────────────────────

    /// Read bytes from guest physical memory.
    ///
    /// Returns up to `size` bytes starting at guest physical address `addr`.
    /// Fails if the address range is outside guest memory.
    pub fn read_guest_memory(&self, addr: u64, size: usize) -> Result<Vec<u8>, VmError> {
        use vm_memory::{Bytes, GuestAddress};
        if checked_guest_range_end(addr, size, self.memory.size()).is_none() {
            return Err(VmError::DiskImage {
                message: format!(
                    "read_guest_memory: addr=0x{:x} size={} out of bounds",
                    addr, size
                ),
            });
        }
        let mut buf = vec![0u8; size];
        self.memory
            .inner()
            .read_slice(&mut buf, GuestAddress(addr))
            .map_err(|_| VmError::DiskImage {
                message: format!(
                    "read_guest_memory: addr=0x{:x} size={} out of bounds",
                    addr, size
                ),
            })?;
        Ok(buf)
    }

    /// Write bytes to guest physical memory.
    ///
    /// Writes `data` starting at guest physical address `addr`.
    /// Fails if the address range is outside guest memory.
    pub fn write_guest_memory(&self, addr: u64, data: &[u8]) -> Result<(), VmError> {
        use vm_memory::{Bytes, GuestAddress};
        if checked_guest_range_end(addr, data.len(), self.memory.size()).is_none() {
            return Err(VmError::DiskImage {
                message: format!(
                    "write_guest_memory: addr=0x{:x} len={} out of bounds",
                    addr,
                    data.len()
                ),
            });
        }
        self.memory
            .inner()
            .write_slice(data, GuestAddress(addr))
            .map_err(|_| VmError::DiskImage {
                message: format!(
                    "write_guest_memory: addr=0x{:x} size={} out of bounds",
                    addr,
                    data.len()
                ),
            })?;
        Ok(())
    }

    /// Read a vCPU's register state.
    ///
    /// Only valid when the vCPU is not inside `KVM_RUN` (i.e. after
    /// `run_bounded` returns or from a restored snapshot).
    pub fn read_vcpu_registers(
        &self,
        vcpu: usize,
    ) -> Result<crate::registers::RegisterState, VmError> {
        if vcpu >= self.vcpus.len() {
            return Err(VmError::DiskImage {
                message: format!(
                    "read_vcpu_registers: vcpu {} out of range (have {})",
                    vcpu,
                    self.vcpus.len()
                ),
            });
        }
        let regs = self.vcpus[vcpu].get_regs().context(GetRegistersSnafu)?;
        let sregs = self.vcpus[vcpu].get_sregs().context(GetRegistersSnafu)?;
        Ok(crate::registers::RegisterState::from_kvm(&regs, &sregs))
    }

    /// Set a vCPU's general-purpose registers.
    ///
    /// Applies the GP registers and RFLAGS from `state`. Segment and
    /// control registers are read-only through this API (use
    /// `set_sregs` directly for those).
    pub fn set_vcpu_registers(
        &mut self,
        vcpu: usize,
        state: &crate::registers::RegisterState,
    ) -> Result<(), VmError> {
        if vcpu >= self.vcpus.len() {
            return Err(VmError::DiskImage {
                message: format!(
                    "set_vcpu_registers: vcpu {} out of range (have {})",
                    vcpu,
                    self.vcpus.len()
                ),
            });
        }
        let mut regs = self.vcpus[vcpu].get_regs().context(GetRegistersSnafu)?;
        state.apply_to_kvm_regs(&mut regs);
        self.vcpus[vcpu]
            .set_regs(&regs)
            .context(SetRegistersSnafu)?;
        Ok(())
    }

    // ─── Determinism log helpers ────────────────────────────────

    /// Emit a dlog record if logging is enabled. No-op otherwise.
    ///
    /// The sequence number is assigned automatically by the writer.
    /// RIP is set to 0 on the hot path (getting it requires an extra
    /// ioctl); callers that have it can fill it in via the builder.
    #[inline]
    fn dlog_emit(&mut self, record: DlogRecord) {
        if let Some(dlog) = &mut self.dlog {
            let _ = dlog.emit(record);
        }
    }

    /// Build a dlog record pre-filled with current VM state.
    #[inline]
    fn dlog_record(&self, tag: DlogTag) -> DlogRecord {
        DlogRecord::new(
            0, // seq assigned by writer
            self.virtual_tsc.read(),
            self.exit_count,
            0, // rip: not fetched on hot path
            tag,
            self.active_vcpu as u8,
        )
    }

    /// Flush dlog to disk without closing it.
    pub fn flush_dlog(&mut self) {
        if let Some(dlog) = &mut self.dlog {
            let _ = dlog.flush();
        }
    }

    /// Replace the dlog file. Flushes and closes the current dlog (if any),
    /// then opens a new file at `path`. Sequence numbers reset to 0.
    pub fn set_dlog_path(&mut self, path: &std::path::Path) -> Result<(), VmError> {
        // Drop the old writer (flushes on Drop).
        self.dlog = None;
        let w = DlogWriter::create(path).map_err(|e| VmError::DiskImage {
            message: format!("dlog create {}: {e}", path.display()),
        })?;
        self.dlog = Some(w);
        Ok(())
    }

    /// Emit a snapshot-taken marker. Call after `snapshot()` succeeds.
    pub fn dlog_snapshot_taken(&mut self) {
        self.dlog_emit(
            self.dlog_record(DlogTag::SnapshotTaken)
                .with_data_u64(self.exit_count),
        );
        self.dlog_emit_memory_hashes();
        self.dlog_flush();
    }

    /// Emit a fault-applied record.
    pub fn dlog_fault_applied(&mut self, fault_type_id: u64) {
        self.dlog_emit(
            self.dlog_record(DlogTag::FaultApplied)
                .with_data_u64(fault_type_id),
        );
    }

    /// Emit an interrupt-injected record.
    pub fn dlog_interrupt_injected(&mut self, irq: u64) {
        self.dlog_emit(
            self.dlog_record(DlogTag::InterruptInjected)
                .with_data_u64(irq),
        );
    }

    /// Emit an NMI-injected record.
    pub fn dlog_nmi_injected(&mut self, target_vcpu: u64) {
        self.dlog_emit(
            self.dlog_record(DlogTag::NmiInjected)
                .with_data_u64(target_vcpu),
        );
    }

    /// Emit a tick marker into the dlog (called by the controller).
    pub fn dlog_tick_marker(&mut self, tick: u64) {
        self.bind_diagnostic_tick(tick);
        self.dlog_emit(self.dlog_record(DlogTag::TickMarker).with_data_u64(tick));
    }

    /// Bind controller time before dispatching restored guest diagnostics.
    pub fn bind_diagnostic_tick(&mut self, tick: u64) {
        self.diagnostic_controller_tick = Some(tick);
    }

    /// Hash selected guest memory pages and emit MemoryHash dlog records.
    ///
    /// Called at snapshot boundaries when `dlog_memory_hash` is enabled.
    /// Hashes: coverage bitmap (0xE0000), hypercall page (0xFE000),
    /// stack area (0x8000–0x9000), and first 1 MB in 4 KB pages.
    pub fn dlog_emit_memory_hashes(&mut self) {
        if self.dlog.is_none() || !self.dlog_memory_hash {
            return;
        }

        use vm_memory::{Bytes, GuestAddress};

        // Well-known pages to hash.
        let mut pages: Vec<u64> = Vec::new();

        // First 1 MB in 4 KB pages (256 pages).
        for pfn in 0..256u64 {
            pages.push(pfn);
        }
        // Coverage bitmap: 0xE0000 (16 pages for 64 KB).
        // Already covered by first 1 MB range above.

        // Hypercall page: 0xFE000.
        // Already covered by first 1 MB range above.

        pages.sort_unstable();
        pages.dedup();

        let mut buf = [0u8; 4096];
        for pfn in pages {
            let gpa = pfn * 4096;
            if self
                .memory
                .inner()
                .read_slice(&mut buf, GuestAddress(gpa))
                .is_ok()
            {
                let crc = crc32fast::hash(&buf);
                let rec = self
                    .dlog_record(DlogTag::MemoryHash)
                    .with_mmio_addr(pfn)
                    .with_data(&crc.to_le_bytes());
                self.dlog_emit(rec);
            }
        }
    }

    /// Flush the dlog to disk (for snapshot boundaries, etc.).
    fn dlog_flush(&mut self) {
        if let Some(dlog) = &mut self.dlog {
            let _ = dlog.sync_all();
        }
    }

    // ─── Public API: fault injection engine ─────────────────────

    /// Get a reference to the fault injection engine.
    pub fn fault_engine(&self) -> &FaultEngine {
        &self.fault_engine
    }

    /// Get a mutable reference to the fault injection engine.
    pub fn fault_engine_mut(&mut self) -> &mut FaultEngine {
        &mut self.fault_engine
    }

    /// Drain accepted assertion hits in guest order with their VM boundary.
    pub fn take_assertion_hits(&mut self) -> Vec<AssertionHitRecord> {
        std::mem::take(&mut self.assertion_hits)
    }

    /// Drain retained protocol observations in arrival order.
    ///
    /// Each record carries the scheduler position the VMM bound at arrival.
    /// Protocol projection meaning and cohort completeness remain consumer-owned.
    pub fn take_protocol_observations(
        &mut self,
    ) -> Vec<chaoscontrol_smr::observation::ProtocolObservation> {
        self.protocol_observations.take_records()
    }

    /// Retained protocol-observation bytes and rejection counts.
    pub fn protocol_observation_state(
        &self,
    ) -> crate::protocol_observation::ProtocolObservationState {
        crate::protocol_observation::ProtocolObservationState {
            retained_bytes: self.protocol_observations.retained_bytes(),
            rejections: self.protocol_observations.rejections(),
        }
    }

    /// Number of accepted hits dropped because the VM-local queue was full.
    pub fn assertion_hit_overflow(&self) -> u64 {
        self.assertion_hit_overflow
    }

    /// Configure branch-local checkpoint stopping and reset its counters.
    pub fn configure_assertion_checkpoint_stop(&mut self, enabled: bool, max_stops: u32) {
        self.assertion_checkpoint_stop = enabled;
        self.assertion_checkpoint_stop_limit = max_stops;
        self.assertion_checkpoint_stops = 0;
        self.assertion_hit_stop_pending = false;
        self.assertion_hits.clear();
        self.assertion_hit_overflow = 0;
    }

    /// Consume a pending checkpoint stop request for the current bounded run.
    pub fn take_assertion_hit_stop(&mut self) -> bool {
        std::mem::take(&mut self.assertion_hit_stop_pending)
    }

    // r[impl chaoscontrol.assertion_checkpoints.observation]
    fn capture_assertion_hits(&mut self) {
        let hits = self.fault_engine.take_assertion_hits();
        for event in hits {
            if self.assertion_hits.len() >= MAX_ASSERTION_HIT_EVENTS {
                self.assertion_hit_overflow = self.assertion_hit_overflow.saturating_add(1);
                continue;
            }
            let eligible = matches!(
                event.kind,
                chaoscontrol_protocol::identity::AssertionKind::Reachable
            ) || (event.kind
                == chaoscontrol_protocol::identity::AssertionKind::Sometimes
                && event.condition);
            if eligible
                && self.assertion_checkpoint_stop
                && self.assertion_checkpoint_stops < self.assertion_checkpoint_stop_limit
            {
                self.assertion_checkpoint_stops = self.assertion_checkpoint_stops.saturating_add(1);
                self.assertion_hit_stop_pending = true;
            }
            self.assertion_hits.push(AssertionHitRecord {
                event,
                exit_count: self.exit_count,
                virtual_tsc: self.virtual_tsc.read(),
                vcpu: self.active_vcpu,
            });
        }
    }

    /// Get a reference to the vCPU scheduler.
    pub fn scheduler(&self) -> &VcpuScheduler {
        &self.scheduler
    }

    /// Get a mutable reference to the vCPU scheduler.
    pub fn scheduler_mut(&mut self) -> &mut VcpuScheduler {
        &mut self.scheduler
    }

    // ─── Internal: SDK hypercall handler ─────────────────────────

    /// Collect one opaque protocol-observation envelope from a lifecycle event.
    ///
    /// The VMM binds only the deterministic scheduler position. Event payloads
    /// that are not protocol observations are left to the fault engine.
    fn collect_protocol_observation(&mut self, page: &HypercallPage) {
        let scheduler_ref =
            crate::protocol_observation::scheduler_position_ref(&self.scheduler.state_id());
        crate::protocol_observation::collect_hypercall(
            &mut self.protocol_observations,
            page,
            &scheduler_ref,
        );
    }

    /// Handle an SDK hypercall triggered by `outb(0x510, 0)`.
    ///
    /// Reads the [`HypercallPage`] from guest memory at
    /// [`HYPERCALL_PAGE_ADDR`], dispatches to the fault engine,
    /// and writes the result back.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    fn handle_sdk_hypercall(&mut self) {
        use vm_memory::Bytes;

        // Read the hypercall page from guest memory
        let mut page = HypercallPage::zeroed();
        let page_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                &mut page as *mut HypercallPage as *mut u8,
                HYPERCALL_PAGE_SIZE,
            )
        };

        if self
            .memory
            .inner()
            .read_slice(page_bytes, vm_memory::GuestAddress(HYPERCALL_PAGE_ADDR))
            .is_err()
        {
            return; // Guest memory read failed — silently ignore
        }

        // Resource observations are VM-owned because they describe this
        // exact guest's admitted execution surface.
        let assertion_command = matches!(
            page.command,
            CMD_ASSERT_ALWAYS
                | CMD_ASSERT_SOMETIMES
                | CMD_ASSERT_REACHABLE
                | CMD_ASSERT_UNREACHABLE
        );
        let mut response_payload = false;
        let (result, status) = if page.command == chaoscontrol_protocol::CMD_RESOURCE_MEMORY_CEILING
        {
            (self.memory_ceiling_bytes, chaoscontrol_protocol::STATUS_OK)
        } else if page.command == chaoscontrol_protocol::CMD_PROCESS_FAULT_POLL {
            response_payload = true;
            match self.fault_engine.write_process_fault_response(&mut page) {
                Ok(present) => (u64::from(present), chaoscontrol_protocol::STATUS_OK),
                Err(_) => (0, chaoscontrol_protocol::STATUS_ERROR),
            }
        } else if page.command == chaoscontrol_protocol::CMD_DIAGNOSTIC_REQUEST {
            response_payload = true;
            match self.fault_engine.write_diagnostic_request(&mut page) {
                Ok(present) => (u64::from(present), chaoscontrol_protocol::STATUS_OK),
                Err(_) => (0, chaoscontrol_protocol::STATUS_ERROR),
            }
        } else if page.command == chaoscontrol_protocol::CMD_DIAGNOSTIC_TICK {
            match self.diagnostic_controller_tick {
                Some(tick) => (tick, chaoscontrol_protocol::STATUS_OK),
                None => (0, chaoscontrol_protocol::STATUS_ERROR),
            }
        } else {
            self.fault_engine.handle_hypercall(&page)
        };

        // Retain only observations the oracle admitted for an active run.
        // Scheduler position binding never widens the oracle's acceptance.
        if page.command == CMD_LIFECYCLE_SEND_EVENT && status == chaoscontrol_protocol::STATUS_OK {
            self.collect_protocol_observation(&page);
        }

        if assertion_command {
            // The engine queues only admitted events. Failure statuses still
            // carry an exact boundary for `always`/`unreachable` findings.
            self.capture_assertion_hits();
        }

        // Write result and status back to the guest page.
        const RESULT_OFFSET: usize = 0x10;
        const STATUS_OFFSET: usize = 0x18;
        if response_payload {
            let result_bytes = result.to_le_bytes();
            page_bytes[RESULT_OFFSET..STATUS_OFFSET].copy_from_slice(&result_bytes);
            page_bytes[STATUS_OFFSET] = status;
            let _ = self
                .memory
                .inner()
                .write_slice(page_bytes, vm_memory::GuestAddress(HYPERCALL_PAGE_ADDR));
        } else {
            let result_bytes = result.to_le_bytes();
            let _ = self.memory.inner().write_slice(
                &result_bytes,
                vm_memory::GuestAddress(HYPERCALL_PAGE_ADDR + RESULT_OFFSET as u64),
            );
            let _ = self.memory.inner().write_slice(
                &[status],
                vm_memory::GuestAddress(HYPERCALL_PAGE_ADDR + STATUS_OFFSET as u64),
            );
        }
    }
}

impl Drop for DeterministicVm {
    fn drop(&mut self) {
        let plan = plan_teardown(self.thread_timer.is_some());
        if plan.disarm_watchdog {
            self.disarm_preemption_timer();
        }
        if plan.release_thread_timer {
            // Taking the unique owner invokes exactly one timer_delete at scope exit.
            let released_timer = self.thread_timer.take();
            debug_assert!(released_timer.is_some());
        }
    }
}

#[path = "vm/coarse_configuration.rs"]
mod coarse_configuration;

#[path = "vm/coarse_boundary.rs"]
mod coarse_boundary;

#[cfg(test)]
#[path = "vm/incremental_lineage.rs"]
mod incremental_lineage;

#[cfg(test)]
#[path = "vm/configuration.rs"]
mod configuration;

#[cfg(test)]
#[path = "vm/diagnostic_fixtures.rs"]
pub(crate) mod diagnostic_fixtures;

#[cfg(test)]
#[path = "vm/runtime_readiness.rs"]
mod runtime_readiness;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_file_reads_reject_symlinks_fifos_and_oversize() {
        use std::os::unix::ffi::OsStrExt;

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        std::fs::write(&target, b"boot").unwrap();
        let link = root.path().join("initrd");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_boot_file(&link, 16).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"boot");

        let fifo = root.path().join("initrd.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_boot_file(&fifo, 16).is_err());
        assert!(open_boot_file(&fifo, 16).is_err());

        assert!(read_boot_file(&target, 3).is_err());
        assert!(open_boot_file(&target, 3).is_err());
        assert!(open_boot_file(&target, 4).is_ok());
        assert!(open_boot_file(root.path(), 4).is_err());
        assert_eq!(read_boot_file(&target, 4).unwrap(), b"boot");
        assert_eq!(read_boot_file(&target, u64::MAX).unwrap(), b"boot");
        assert!(read_boot_file(std::path::Path::new("/proc/self/cmdline"), u64::MAX).is_err());
    }
    use crate::devices::virtio_types::VirtioFailure;
    use crate::vm_core::MAX_SERIAL_CAPTURE_BYTES;

    #[test]
    fn guest_memory_range_is_checked_before_allocation() {
        assert_eq!(checked_guest_range_end(0, 4, 10), Some(4));
        assert_eq!(checked_guest_range_end(6, 4, 10), Some(10));
        assert_eq!(checked_guest_range_end(10, 0, 10), Some(10));
        assert_eq!(checked_guest_range_end(9, 2, 10), None);
        assert_eq!(checked_guest_range_end(u64::MAX, 1, 10), None);
        assert_eq!(checked_guest_range_end(0, 11, 10), None);
    }

    // r[verify chaoscontrol.replay_debugger.navigation]
    #[test]
    fn exact_observation_settles_out_without_executing_the_next_instruction() {
        const MEMORY_BYTES: usize = 4 * 1024 * 1024;
        const CODE_ADDRESS: u64 = 0x200;
        const MARKER: u64 = 0x5a;
        // OUT 0x10,AL; INC AX; HLT. Settlement must not execute INC AX.
        const CODE: [u8; 4] = [0xe6, 0x10, 0x40, 0xf4];
        let mut vm = DeterministicVm::new(VmConfig {
            memory_size: MEMORY_BYTES,
            ..VmConfig::default()
        })
        .unwrap();
        vm.write_guest_memory(CODE_ADDRESS, &CODE).unwrap();
        let mut sregs = vm.vcpus[0].get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        vm.vcpus[0].set_sregs(&sregs).unwrap();
        vm.vcpus[0]
            .set_regs(&kvm_regs {
                rip: CODE_ADDRESS,
                rax: MARKER,
                rflags: 2,
                ..Default::default()
            })
            .unwrap();
        assert!(
            matches!(vm.vcpus[0].run().unwrap(), VcpuExit::IoOut(0x10, data) if data == [MARKER as u8])
        );
        let scheduler = vm.scheduler.snapshot();
        let exit_count = vm.exit_count;
        let io_exit_count = vm.io_exit_count;
        let tsc = vm.virtual_tsc.read();
        vm.settle_exact_observation().unwrap();
        let settled = vm.vcpus[0].get_regs().unwrap();
        assert_eq!(settled.rip, CODE_ADDRESS + 2);
        assert_eq!(settled.rax, MARKER);
        vm.settle_exact_observation().unwrap();
        assert_eq!(vm.vcpus[0].get_regs().unwrap(), settled);
        assert_eq!(vm.scheduler.snapshot(), scheduler);
        assert_eq!(
            (vm.exit_count, vm.io_exit_count, vm.virtual_tsc.read()),
            (exit_count, io_exit_count, tsc)
        );
        assert_eq!(
            vm.read_guest_memory(CODE_ADDRESS, CODE.len()).unwrap(),
            CODE
        );
        // Full checkpoint capture uses the identical immediate-exit boundary.
        vm.snapshot().unwrap();
        assert_eq!(vm.vcpus[0].get_regs().unwrap(), settled);
    }

    #[test]
    fn bootstrap_hlt_wakes_at_virtual_pit_deadline_but_running_guest_times_out() {
        const TEST_MEMORY_BYTES: usize = 4 * 1024 * 1024;
        const CODE_ADDRESS: u64 = 0x200;
        const IRQ_HANDLER_ADDRESS: u64 = 0x100;
        const IRQ_VECTOR: u8 = 0x20;
        const STACK_ADDRESS: u64 = 0x1000;
        const OUTPUT_MARKER: u8 = 0x5a;
        // STI; HLT; MOV AL,marker; OUT 0x10,AL; CLI; JMP $.
        const CODE: [u8; 9] = [
            0xfb,
            0xf4,
            0xb0,
            OUTPUT_MARKER,
            0xe6,
            0x10,
            0xfa,
            0xeb,
            0xfe,
        ];
        // Acknowledge IRQ0 in the PIC, then return from the real-mode interrupt.
        const IRQ_HANDLER: [u8; 5] = [0xb0, 0x20, 0xe6, 0x20, 0xcf];
        const PERIODIC_RELOAD: u32 = 1193;

        let mut vm = DeterministicVm::new(VmConfig {
            memory_size: TEST_MEMORY_BYTES,
            ..VmConfig::default()
        })
        .unwrap();
        vm.init_thread_timer().unwrap();
        assert!(
            vm.thread_timer.is_some(),
            "test requires a thread-local timer"
        );
        assert!(!vm.fault_engine.is_setup_complete());

        vm.memory
            .inner()
            .write_slice(&CODE, GuestAddress(CODE_ADDRESS))
            .unwrap();
        vm.memory
            .inner()
            .write_slice(&IRQ_HANDLER, GuestAddress(IRQ_HANDLER_ADDRESS))
            .unwrap();
        let vector = [
            IRQ_HANDLER_ADDRESS as u8,
            (IRQ_HANDLER_ADDRESS >> 8) as u8,
            0,
            0,
        ];
        vm.memory
            .inner()
            .write_slice(&vector, GuestAddress(u64::from(IRQ_VECTOR) * 4))
            .unwrap();
        let mut sregs = vm.vcpus[0].get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        sregs.ss.base = 0;
        sregs.ss.selector = 0;
        vm.vcpus[0].set_sregs(&sregs).unwrap();
        let regs = kvm_regs {
            rip: CODE_ADDRESS,
            rsp: STACK_ADDRESS,
            rflags: 2,
            ..Default::default()
        };
        vm.vcpus[0].set_regs(&regs).unwrap();

        let mut chip = kvm_bindings::kvm_irqchip {
            chip_id: kvm_bindings::KVM_IRQCHIP_PIC_MASTER,
            ..Default::default()
        };
        vm.vm.get_irqchip(&mut chip).unwrap();
        // SAFETY: chip_id selects the PIC member of the returned union.
        let mut pic = unsafe { chip.chip.pic };
        pic.irq_base = IRQ_VECTOR;
        pic.imr = 0xfe;
        chip.chip.pic = pic;
        vm.vm.set_irqchip(&chip).unwrap();
        let mut pit = vm.vm.get_pit2().unwrap();
        pit.channels[0].count = PERIODIC_RELOAD;
        pit.channels[0].mode = 2;
        pit.channels[0].gate = 1;
        vm.vm.set_pit2(&pit).unwrap();
        vm.sync_and_suppress_pit().unwrap();
        let deadline = vm.pit.next_irq_tsc().unwrap();

        // Actual guest HLT is consumed by KVM. Its architectural MP state,
        // not the observation timer's elapsed time, authorizes this deadline.
        assert!(!vm.step().unwrap());
        assert_eq!(vm.virtual_tsc.read(), deadline);
        assert_eq!(vm.pit.irqs_delivered(), 1);
        assert_eq!(vm.exit_count, 1);
        assert!(!vm.step().unwrap());
        let regs = vm.vcpus[0].get_regs().unwrap();
        assert_eq!(regs.rax as u8, OUTPUT_MARKER);
        assert_eq!(vm.exit_count, 2);

        // The pending OUT completes on re-entry, then CLI/JMP cannot halt.
        // Even before setup, the timeout is operational, with no virtual progress.
        // Settle IRQs already due after the normal I/O-exit time advance.
        vm.sync_and_suppress_pit().unwrap();
        let irqs_before_timeout = vm.pit.irqs_delivered();
        let tsc_before_timeout = vm.virtual_tsc.read();
        assert!(matches!(
            vm.run_bounded(1),
            Err(VmError::HostWatchdogTimeout { vcpu: 0 })
        ));
        assert_eq!(vm.virtual_tsc.read(), tsc_before_timeout);
        assert_eq!(vm.exit_count, 2);
        assert_eq!(vm.pit.irqs_delivered(), irqs_before_timeout);
    }

    #[test]
    fn pit_irq0_uses_virtual_time_across_fresh_and_retained_restore() {
        const TEST_MEMORY_BYTES: usize = 4 * 1024 * 1024;
        const PERIODIC_RELOAD: u32 = 1193; // Approximately 1 ms at PIT frequency.
                                           // Exceeds even the constructor's maximum-count one-shot (~55 ms).
        const HOST_TIMER_OBSERVATION: std::time::Duration = std::time::Duration::from_millis(100);
        const IRQ0_MASK: u8 = 1;

        fn irq0_pending(vm: &DeterministicVm) -> bool {
            let mut chip = kvm_bindings::kvm_irqchip {
                chip_id: kvm_bindings::KVM_IRQCHIP_PIC_MASTER,
                ..Default::default()
            };
            vm.vm.get_irqchip(&mut chip).unwrap();
            // SAFETY: chip_id selects the PIC member of the returned union.
            unsafe { chip.chip.pic.irr & IRQ0_MASK != 0 }
        }

        fn assert_virtual_irq_only(vm: &mut DeterministicVm) {
            // No vCPU executes during this observation. A pending IRQ can only
            // come from the host timer, not virtual-time progress.
            std::thread::sleep(HOST_TIMER_OBSERVATION);
            assert!(!irq0_pending(vm), "host time generated IRQ 0");

            let deadline = vm.pit.next_irq_tsc().expect("armed software PIT");
            vm.virtual_tsc.set(deadline - 1);
            vm.sync_and_suppress_pit().unwrap();
            assert!(
                !irq0_pending(vm),
                "software IRQ 0 arrived before its deadline"
            );
            vm.virtual_tsc.set(deadline);
            vm.sync_and_suppress_pit().unwrap();
            assert!(irq0_pending(vm), "software IRQ 0 did not reach the PIC");
        }

        let config = VmConfig {
            memory_size: TEST_MEMORY_BYTES,
            ..VmConfig::default()
        };
        let mut retained = DeterministicVm::new(config.clone()).unwrap();
        std::thread::sleep(HOST_TIMER_OBSERVATION);
        assert!(!irq0_pending(&retained), "constructor armed a host timer");

        let mut pit = retained.vm.get_pit2().unwrap();
        pit.channels[0].count = PERIODIC_RELOAD;
        pit.channels[0].mode = 2;
        pit.channels[0].gate = 1;
        retained.vm.set_pit2(&pit).unwrap();
        retained.sync_and_suppress_pit().unwrap();
        let snapshot = retained.snapshot().unwrap();
        assert_virtual_irq_only(&mut retained);

        // An unflagged kernel PIT can rearm a physical timer. Reject it before
        // mutation rather than silently changing the captured device identity.
        let mut unsafe_snapshot = snapshot.clone();
        let crate::snapshot::SnapshotKernelPitState::Present(pit) = &mut unsafe_snapshot.pit else {
            panic!("single-vCPU profile must retain its kernel PIT");
        };
        pit.flags &= !KVM_PIT_FLAGS_HPET_LEGACY;
        let pit_before = retained.pit.snapshot();
        let tsc_before = retained.virtual_tsc.read();
        assert!(retained.restore(&unsafe_snapshot).is_err());
        assert!(retained
            .restore_incremental(&unsafe_snapshot, &vec![0; TEST_MEMORY_BYTES])
            .is_err());
        assert_eq!(retained.pit.snapshot(), pit_before);
        assert_eq!(retained.virtual_tsc.read(), tsc_before);
        assert!(retained.schedule_execution_poison().is_none());

        // Valid capture/restore never reloads the software timer from hardware.
        retained.restore(&snapshot).unwrap();
        assert_virtual_irq_only(&mut retained);

        let mut fresh = DeterministicVm::new(config).unwrap();
        fresh.restore(&snapshot).unwrap();
        assert_virtual_irq_only(&mut fresh);

        let base = vec![0; TEST_MEMORY_BYTES];
        retained.restore_incremental(&snapshot, &base).unwrap();
        assert_virtual_irq_only(&mut retained);
    }

    #[test]
    fn coarse_pit_pio_and_deadlines_survive_host_delay_and_both_restore_paths() {
        use crate::snapshot::SnapshotKernelPitState;
        use diagnostic_fixtures::{
            coarse_smp_vm, stage_coarse_irq, COARSE_HANDLER_ADDRESS, COARSE_MEMORY_BYTES,
            COARSE_WATCHDOG_US,
        };
        const HOST_DELAY: std::time::Duration = std::time::Duration::from_millis(100);
        const BOUNDARY_QUANTUM: u64 = 128;
        const MAX_STEPS: usize = 64;
        // CLI; program channel 0 periodic, channel 2 one-shot (both 65535);
        // enable channel 2's gate inside a TF-suppressed interrupt handler.
        // Each OUT must nevertheless be a userspace PIO boundary.
        const PROGRAM: &[u8] = &[
            0xfa, 0xb0, 0x34, 0xe6, 0x43, 0xb0, 0xff, 0xe6, 0x40, 0xe6, 0x40, 0xb0, 0xb0, 0xe6,
            0x43, 0xb0, 0xff, 0xe6, 0x42, 0xe6, 0x42, 0xb0, 0x01, 0xe6, 0x61,
        ];
        // Latch/read channel 2 into BX, then sample speaker state into CX.
        // The final OUT establishes a settled observation before IRET.
        const READS: &[u8] = &[
            0xb0, 0x80, 0xe6, 0x43, 0xe4, 0x42, 0x88, 0xc3, 0xe4, 0x42, 0x88, 0xc7, 0xe4, 0x61,
            0x88, 0xc1, 0xe4, 0x61, 0x88, 0xc5, 0xe6, 0x10,
        ];
        fn reach(vm: &mut DeterministicVm, target: u64) {
            for _ in 0..MAX_STEPS {
                if vm.vcpus[0].get_regs().unwrap().rip == target {
                    return;
                }
                assert!(!vm.step().unwrap());
            }
            panic!("PIT microguest did not reach its declared settled boundary");
        }
        fn irq0_pending(vm: &DeterministicVm) -> bool {
            let mut chip = kvm_bindings::kvm_irqchip {
                chip_id: kvm_bindings::KVM_IRQCHIP_PIC_MASTER,
                ..Default::default()
            };
            vm.vm.get_irqchip(&mut chip).unwrap();
            // SAFETY: chip_id selects the returned PIC union member.
            unsafe { chip.chip.pic.irr & 1 != 0 }
        }
        const MAIN: &[u8] = &[0x90, 0xeb, 0xfd];
        let handler = [PROGRAM, READS, &[0xcf]].concat();
        let mut vm = coarse_smp_vm(MAIN, &handler, BOUNDARY_QUANTUM, COARSE_WATCHDOG_US);
        std::thread::sleep(HOST_DELAY);
        assert!(
            !irq0_pending(&vm),
            "constructor created a coarse host timer"
        );
        stage_coarse_irq(&mut vm);
        assert_eq!(vm.vm.get_pit2().unwrap_err().errno(), libc::ENXIO);
        reach(&mut vm, COARSE_HANDLER_ADDRESS + PROGRAM.len() as u64);
        assert_eq!(vm.pit.channel_reload(0), u16::MAX);
        assert_eq!(vm.pit.channel_reload(2), u16::MAX);
        assert_eq!(
            vm.io_exit_count, 7,
            "kernel must not consume PIT programming"
        );
        vm.take_schedule_trace().unwrap();
        let checkpoint = vm.snapshot().unwrap();
        assert!(matches!(checkpoint.pit, SnapshotKernelPitState::Absent));
        let deadline = vm.pit.next_irq_tsc().unwrap();
        let end = COARSE_HANDLER_ADDRESS + (PROGRAM.len() + READS.len()) as u64;
        reach(&mut vm, end);
        let control_regs = vm.vcpus[0].get_regs().unwrap();
        let control_pit = vm.pit.snapshot();
        let control_tsc = vm.virtual_tsc.read();
        let control_trace = serde_json::to_vec(&vm.take_schedule_trace().unwrap()).unwrap();

        for restore_kind in 0..3 {
            std::thread::sleep(HOST_DELAY);
            let mut fresh = (restore_kind == 1)
                .then(|| coarse_smp_vm(MAIN, &handler, BOUNDARY_QUANTUM, COARSE_WATCHDOG_US));
            let vm = fresh.as_mut().unwrap_or(&mut vm);
            if restore_kind == 2 {
                vm.restore_incremental(&checkpoint, &vec![0; COARSE_MEMORY_BYTES])
                    .unwrap();
            } else {
                vm.restore(&checkpoint).unwrap();
            }
            assert_eq!(
                &vm.execution_descriptor,
                checkpoint.execution_descriptor().unwrap()
            );
            assert_eq!(vm.pit.next_irq_tsc(), Some(deadline));
            assert_eq!(vm.vm.get_pit2().unwrap_err().errno(), libc::ENXIO);
            std::thread::sleep(HOST_DELAY);
            assert_eq!(vm.pit.snapshot(), checkpoint.pit_snapshot);
            assert!(!irq0_pending(vm), "host time generated coarse IRQ 0");
            reach(vm, end);
            assert_eq!(vm.vcpus[0].get_regs().unwrap(), control_regs);
            assert_eq!(vm.pit.snapshot(), control_pit);
            assert_eq!(vm.virtual_tsc.read(), control_tsc);
            assert_eq!(
                serde_json::to_vec(&vm.take_schedule_trace().unwrap()).unwrap(),
                control_trace
            );
            assert!(!irq0_pending(vm));
            vm.virtual_tsc.set(deadline - 1);
            assert!(!vm.sync_and_suppress_pit().unwrap());
            assert!(!irq0_pending(vm));
            vm.virtual_tsc.set(deadline);
            assert!(vm.sync_and_suppress_pit().unwrap());
            assert!(irq0_pending(vm), "virtual deadline must reach the PIC");
        }

        // Creating even an inhibited kernel PIT would steal PIO again.
        vm.vm.create_pit2(kvm_pit_config::default()).unwrap();
        let mut kernel_pit = vm.vm.get_pit2().unwrap();
        kernel_pit.flags |= KVM_PIT_FLAGS_HPET_LEGACY;
        vm.vm.set_pit2(&kernel_pit).unwrap();
        assert!(vm.validate_coarse_clock_devices().is_err());
    }

    #[test]
    fn serial_capture_small_writes_accumulate() {
        let mut buf = Vec::new();
        assert_eq!(capture_serial_bounded(&mut buf, b"hello "), 0);
        assert_eq!(capture_serial_bounded(&mut buf, b"world"), 0);
        assert_eq!(buf, b"hello world");
    }

    #[test]
    fn serial_capture_exact_capacity_is_retained() {
        let mut buf = Vec::new();
        let incoming = vec![0xAB; MAX_SERIAL_CAPTURE_BYTES];
        assert_eq!(capture_serial_bounded(&mut buf, &incoming), 0);
        assert_eq!(buf.len(), MAX_SERIAL_CAPTURE_BYTES);
        assert!(buf.iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn serial_capture_overflow_keeps_recent_tail() {
        let mut buf = Vec::new();
        // Fill to capacity with 0x00, then overflow with a marker tail.
        assert_eq!(
            capture_serial_bounded(&mut buf, &vec![0x00; MAX_SERIAL_CAPTURE_BYTES]),
            0
        );
        let marker = b"OVERFLOW-MARKER";
        let dropped = capture_serial_bounded(&mut buf, marker);
        assert!(dropped > 0, "overflow must drop oldest bytes");
        assert!(buf.len() <= MAX_SERIAL_CAPTURE_BYTES);
        assert!(
            buf.ends_with(marker),
            "most recent output must survive overflow"
        );
    }

    #[test]
    fn serial_capture_oversized_single_write_keeps_its_tail() {
        const EXTRA: usize = 128;
        let mut buf = b"prior".to_vec();
        let mut incoming = vec![0x00; MAX_SERIAL_CAPTURE_BYTES + EXTRA];
        let tail_start = incoming.len() - MAX_SERIAL_CAPTURE_BYTES;
        incoming[tail_start..].fill(0xFF);
        let dropped = capture_serial_bounded(&mut buf, &incoming);
        assert_eq!(dropped, b"prior".len() + EXTRA);
        assert_eq!(buf.len(), MAX_SERIAL_CAPTURE_BYTES);
        assert!(buf.iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn serial_capture_stays_bounded_across_many_writes() {
        let mut buf = Vec::new();
        let chunk = [0x55; 4096];
        let writes = (MAX_SERIAL_CAPTURE_BYTES / chunk.len()) * 2;
        for _ in 0..writes {
            capture_serial_bounded(&mut buf, &chunk);
            assert!(buf.len() <= MAX_SERIAL_CAPTURE_BYTES);
        }
    }

    #[test]
    fn sanitize_serial_passes_printable_and_whitespace() {
        let input = b"boot ok\nprogress\r\ttab\xF0\x9F\x9A\x80";
        assert_eq!(sanitize_serial_for_terminal(input), input);
    }

    #[test]
    fn sanitize_serial_neutralizes_escape_sequences() {
        // ESC[2J (clear screen) and ESC]0;title BEL (window title) must
        // not reach the operator's terminal intact.
        let input = b"\x1B[2J\x1B]0;evil\x07\x00\x7Ftext";
        let out = sanitize_serial_for_terminal(input);
        assert!(!out.contains(&0x1B), "ESC must be replaced");
        assert!(!out.contains(&0x07), "BEL must be replaced");
        assert!(!out.contains(&0x00), "NUL must be replaced");
        assert!(!out.contains(&0x7F), "DEL must be replaced");
        assert!(out.ends_with(b"text"));
    }

    #[test]
    fn capturing_writer_counts_dropped_bytes() {
        use std::io::Write;
        let mut writer = CapturingWriter::new();
        assert_eq!(writer.dropped_byte_count(), 0);
        writer
            .write_all(&vec![0x00; MAX_SERIAL_CAPTURE_BYTES + 1])
            .expect("write");
        assert_eq!(writer.dropped_byte_count(), 1);
        assert_eq!(writer.take().len(), MAX_SERIAL_CAPTURE_BYTES);
        // The counter is cumulative: take() does not reset it.
        assert_eq!(writer.dropped_byte_count(), 1);
    }

    #[test]
    fn test_vm_config_default() {
        let config = VmConfig::default();
        assert_eq!(config.memory_size, 256 * 1024 * 1024);
        assert_eq!(config.num_vcpus, 1);
        assert_eq!(config.cpu.tsc_khz, 3_000_000);
    }

    #[test]
    fn virtio_interrupt_delivery_asserts_then_deasserts() {
        let entropy = crate::devices::entropy::DeterministicEntropy::new(0);
        let backend = crate::devices::virtio_entropy::VirtioEntropy::new(entropy);
        let mut device =
            VirtioMmioDevice::new(VIRTIO_MMIO_BASE_2, VIRTIO_MMIO_IRQ_2, Box::new(backend));
        let mut levels = [false; VIRTIO_IRQ_LEVELS.len()];
        let mut calls = 0usize;
        deliver_virtio_interrupt_with(&mut device, 0, |_irq, asserted| {
            levels[calls] = asserted;
            calls += 1;
            Ok(())
        })
        .expect("interrupt delivery");
        assert_eq!(levels, VIRTIO_IRQ_LEVELS);
        assert!(device.live_state().failure.is_none());
    }

    #[test]
    fn virtio_interrupt_failures_poison_state_and_return_error() {
        for failed_level in VIRTIO_IRQ_LEVELS {
            let entropy = crate::devices::entropy::DeterministicEntropy::new(0);
            let backend = crate::devices::virtio_entropy::VirtioEntropy::new(entropy);
            let mut device =
                VirtioMmioDevice::new(VIRTIO_MMIO_BASE_2, VIRTIO_MMIO_IRQ_2, Box::new(backend));
            let error = deliver_virtio_interrupt_with(&mut device, 0, |_irq, asserted| {
                if asserted == failed_level {
                    Err(kvm_ioctls::Error::new(libc::EIO))
                } else {
                    Ok(())
                }
            })
            .expect_err("interrupt failure");
            assert!(matches!(
                error,
                VmError::VirtioInterrupt {
                    irq: VIRTIO_MMIO_IRQ_2,
                    asserted,
                    ..
                } if asserted == failed_level
            ));
            assert_eq!(
                device.live_state().failure,
                Some(VirtioFailure::InterruptDelivery {
                    irq: VIRTIO_MMIO_IRQ_2,
                    asserted: failed_level,
                })
            );
            assert!(device.live_state().queues[0].failure.is_some());
        }
    }

    #[test]
    fn test_virtio_devices_created() {
        let config = VmConfig::default();
        let vm = DeterministicVm::new(config).unwrap();
        let devices = vm.virtio_devices();
        assert_eq!(devices.len(), 3);

        // Device 0: block @ 0xD000_0000 IRQ 5
        assert_eq!(devices[0].base_addr(), VIRTIO_MMIO_BASE_0);
        assert_eq!(devices[0].irq(), VIRTIO_MMIO_IRQ_0);

        // Device 1: net @ 0xD000_1000 IRQ 6
        assert_eq!(devices[1].base_addr(), VIRTIO_MMIO_BASE_1);
        assert_eq!(devices[1].irq(), VIRTIO_MMIO_IRQ_1);

        // Device 2: entropy @ 0xD000_2000 IRQ 7
        assert_eq!(devices[2].base_addr(), VIRTIO_MMIO_BASE_2);
        assert_eq!(devices[2].irq(), VIRTIO_MMIO_IRQ_2);

        let observations = vm
            .runtime_capacity_observations()
            .expect("runtime capacity observations");
        chaoscontrol_sim_core::validate_runtime_capacity_observations(&observations)
            .expect("runtime capacity observations validate");
        assert_eq!(observations.leaked_scratch_slots, 0);
        assert_eq!(observations.leaked_packet_slots, 0);
    }

    #[test]
    fn test_virtio_mmio_magic_read() {
        let config = VmConfig::default();
        let vm = DeterministicVm::new(config).unwrap();
        let devices = vm.virtio_devices();

        // Read magic value from device 0
        let mut buf = [0u8; 4];
        devices[0].read(0x000, &mut buf); // VIRTIO_MMIO_MAGIC_VALUE offset
        let magic = u32::from_le_bytes(buf);
        assert_eq!(magic, 0x74726976); // "virt"
    }

    #[test]
    fn test_virtio_device_types() {
        let config = VmConfig::default();
        let vm = DeterministicVm::new(config).unwrap();
        let devices = vm.virtio_devices();

        // Device 0: block (device ID = 2)
        let mut buf = [0u8; 4];
        devices[0].read(0x008, &mut buf); // VIRTIO_MMIO_DEVICE_ID offset
        assert_eq!(u32::from_le_bytes(buf), 2);

        // Device 1: net (device ID = 1)
        devices[1].read(0x008, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), 1);

        // Device 2: entropy/rng (device ID = 4)
        devices[2].read(0x008, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), 4);
    }

    // ─── Quick Win #6: Time source tests ────────────────────────

    #[test]
    fn test_nohpet_in_cmdline() {
        let config = VmConfig::default();
        let vm = DeterministicVm::new(config).unwrap();
        let bytes = vm.build_cmdline(0);
        let cmdline = String::from_utf8_lossy(&bytes);
        assert!(cmdline.contains("nohpet"), "cmdline must disable HPET");
    }

    #[test]
    fn test_nohpet_in_smp_cmdline() {
        let config = VmConfig {
            num_vcpus: 2,
            ..VmConfig::default()
        };
        let vm = diagnostic_fixtures::partial_smp_vm(config);
        let bytes = vm.build_cmdline(0);
        let cmdline = String::from_utf8_lossy(&bytes);
        assert!(cmdline.contains("nohpet"), "SMP cmdline must disable HPET");
    }

    #[test]
    fn test_acpi_pm_timer_constants() {
        // ACPI PM timer at port 0x408, frequency 3.579545 MHz
        assert_eq!(ACPI_PM_TIMER_PORT, 0x408);
        assert_eq!(ACPI_PM_TIMER_FREQ_HZ, 3_579_545);
    }

    #[test]
    fn test_hpet_mmio_constants() {
        // HPET at 0xFED0_0000, 1 KiB region
        assert_eq!(HPET_MMIO_BASE, 0xFED0_0000);
        assert_eq!(HPET_MMIO_SIZE, 0x400);
    }

    // ─── Quick Win #3: Core pinning tests ───────────────────────

    #[test]
    fn post_kvm_poison_retains_reservation_and_blocks_all_evidence_paths() {
        const TEST_MEMORY_BYTES: usize = 4 * 1024 * 1024;
        const POISONED_TSC_ADVANCE: u64 = 123;
        let config = VmConfig {
            memory_size: TEST_MEMORY_BYTES,
            num_vcpus: 2,
            ..VmConfig::default()
        };
        let mut vm = diagnostic_fixtures::partial_smp_vm(config);
        let snapshot = vm.snapshot().unwrap();
        let tsc_target = vm.virtual_tsc.read().saturating_add(POISONED_TSC_ADVANCE);
        vm.virtual_tsc.advance_to(tsc_target);
        let poisoned_tsc = vm.virtual_tsc.read();
        let state_before = vm.scheduler.state_id();
        let reservation = vm.reserve_guest_progress().unwrap();
        let error = vm.poison_after_possible_guest_progress(
            reservation,
            SchedulePoisonStage::PmuRead,
            "injected post-KVM read failure",
        );
        assert!(matches!(
            error,
            VmError::ScheduleExecutionPoisoned {
                stage: SchedulePoisonStage::PmuRead,
                ..
            }
        ));
        assert!(vm.scheduler.reservation_outstanding());
        assert_eq!(vm.scheduler.state_id(), state_before);
        assert!(vm.pending_schedule_reservation.is_none());
        assert!(matches!(
            vm.step(),
            Err(VmError::ScheduleExecutionPoisoned { .. })
        ));
        assert!(matches!(
            vm.snapshot(),
            Err(VmError::ScheduleExecutionPoisoned { .. })
        ));
        assert!(matches!(
            vm.restore(&snapshot),
            Err(VmError::ScheduleExecutionPoisoned { .. })
        ));
        assert!(matches!(
            vm.take_schedule_trace(),
            Err(VmError::ScheduleExecutionPoisoned { .. })
        ));
        assert!(matches!(
            vm.run_bounded(1),
            Err(VmError::ScheduleExecutionPoisoned { .. })
        ));
        assert!(matches!(
            vm.run_until("unreachable"),
            Err(VmError::ScheduleExecutionPoisoned { .. })
        ));
        assert!(matches!(
            vm.run(),
            Err(VmError::ScheduleExecutionPoisoned { .. })
        ));
        assert_eq!(vm.virtual_tsc.read(), poisoned_tsc);
    }

    #[test]
    fn committed_exit_and_schedule_action_failures_permanently_poison_execution() {
        const TEST_MEMORY_BYTES: usize = 4 * 1024 * 1024;
        for stage in [
            SchedulePoisonStage::ExitHandling,
            SchedulePoisonStage::ScheduleAction,
        ] {
            let config = VmConfig {
                memory_size: TEST_MEMORY_BYTES,
                num_vcpus: 2,
                ..VmConfig::default()
            };
            let mut vm = diagnostic_fixtures::partial_smp_vm(config);
            vm.inject_controller_test_progress().unwrap();
            let committed_state = vm.scheduler.state_id();
            let injected_result: Result<bool, VmError> = Err(VmError::Snapshot {
                message: "injected post-commit failure".to_string(),
            });

            let error = vm.finish_after_committed_progress(stage, injected_result);

            assert!(matches!(
                error,
                Err(VmError::ScheduleExecutionPoisoned {
                    stage: observed_stage,
                    ..
                }) if observed_stage == stage
            ));
            assert_eq!(vm.scheduler.state_id(), committed_state);
            assert!(!vm.scheduler.reservation_outstanding());
            assert!(matches!(
                vm.run_bounded(1),
                Err(VmError::ScheduleExecutionPoisoned {
                    stage: observed_stage,
                    ..
                }) if observed_stage == stage
            ));
            assert!(matches!(
                vm.snapshot(),
                Err(VmError::ScheduleExecutionPoisoned { .. })
            ));
            assert!(matches!(
                vm.take_schedule_trace(),
                Err(VmError::ScheduleExecutionPoisoned { .. })
            ));
        }
    }

    #[test]
    fn hlt_latch_masks_kvm_runnable_state_until_explicit_wake() {
        const TEST_MEMORY_BYTES: usize = 4 * 1024 * 1024;
        const TEST_VCPU_COUNT: usize = 2;
        let config = VmConfig {
            memory_size: TEST_MEMORY_BYTES,
            num_vcpus: TEST_VCPU_COUNT,
            ..VmConfig::default()
        };
        let mut vm = diagnostic_fixtures::partial_smp_vm(config);
        let scheduler_config = SchedulerConfig {
            num_vcpus: TEST_VCPU_COUNT,
            progress_budget: ProgressBudget::Instructions(DEFAULT_SMP_INSTRUCTION_QUANTUM),
            strategy: SchedulingStrategy::RoundRobin,
            seed: 0,
        };
        vm.scheduler = VcpuScheduler::try_new(
            &scheduler_config,
            ProgressMode::ExactSingleStep,
            vec![true; TEST_VCPU_COUNT],
        )
        .unwrap();
        vm.active_vcpu = vm.scheduler.active();
        let reservation = vm.scheduler.reserve_transition().unwrap();
        let planned = plan_execution_observation(
            vm.scheduler.state(),
            ExecutionProgressObservation::ExactInstructionHalt { vcpu: 0 },
        )
        .unwrap()
        .unwrap();
        vm.scheduler.commit(reservation, planned).unwrap();
        vm.hlt_latched_vcpus[0] = true;

        let observed_before_wake = vm.observe_runnable_vcpus().unwrap();
        assert!(!observed_before_wake[0]);
        assert!(!vm.scheduler.state().runnable_vcpus[0]);
        let snapshot = vm.snapshot().unwrap();
        assert!(DeterministicVm::hlt_latches_from_snapshot(&snapshot)[0]);
        vm.validate_fault_engine_snapshot(&snapshot).unwrap();
        let mut malformed_snapshot = snapshot.clone();
        malformed_snapshot.hlt_latched_vcpus.clear();
        assert!(matches!(
            vm.validate_fault_engine_snapshot(&malformed_snapshot),
            Err(VmError::Snapshot { .. })
        ));

        vm.record_explicit_vcpu_wake(0).unwrap();

        assert!(!vm.hlt_latched_vcpus[0]);
        assert!(vm.scheduler.state().runnable_vcpus[0]);
        assert_eq!(vm.scheduler.state().progress_counts()[0], 1);
        assert!(vm.observe_runnable_vcpus().unwrap()[0]);
    }

    #[test]
    fn diagnostic_step_rearms_after_popf_and_iret_without_skipping_compute() {
        const CODE_ADDRESS: u64 = 0x200;
        const RETURN_ADDRESS: u64 = 0x300;
        const STACK_ADDRESS: u64 = 0x1000;
        const FLAGS_WITHOUT_TF: u16 = 2;
        // INC AX; INC BX; OUT 0x10,AL. Neither compute instruction may be
        // swallowed into the next I/O exit after guest code clears TF.
        const COMPUTE: [u8; 4] = [0x40, 0x43, 0xe6, 0x10];
        for use_iret in [false, true] {
            let mut vm = diagnostic_fixtures::partial_smp_vm(VmConfig {
                memory_size: 4 * 1024 * 1024,
                num_vcpus: 2,
                smp_progress_budget: ProgressBudget::Instructions(64),
                ..VmConfig::default()
            });
            let mut sregs = vm.vcpus[0].get_sregs().unwrap();
            sregs.cs.base = 0;
            sregs.cs.selector = 0;
            sregs.ss.base = 0;
            sregs.ss.selector = 0;
            vm.vcpus[0].set_sregs(&sregs).unwrap();
            let (code, expected_rips, compute_address) = if use_iret {
                vm.write_guest_memory(RETURN_ADDRESS, &COMPUTE).unwrap();
                let frame = [
                    RETURN_ADDRESS as u16,
                    0, // return CS
                    FLAGS_WITHOUT_TF,
                ];
                let bytes: Vec<u8> = frame.into_iter().flat_map(u16::to_le_bytes).collect();
                vm.write_guest_memory(STACK_ADDRESS, &bytes).unwrap();
                (
                    vec![0xcf], // IRET clears TF from the stacked RFLAGS.
                    vec![RETURN_ADDRESS, RETURN_ADDRESS + 1, RETURN_ADDRESS + 2],
                    RETURN_ADDRESS,
                )
            } else {
                // PUSH 2; POPF; followed by the compute instructions.
                let mut code = vec![0x6a, FLAGS_WITHOUT_TF as u8, 0x9d];
                code.extend_from_slice(&COMPUTE);
                (
                    code,
                    vec![
                        CODE_ADDRESS + 2,
                        CODE_ADDRESS + 3,
                        CODE_ADDRESS + 4,
                        CODE_ADDRESS + 5,
                    ],
                    CODE_ADDRESS + 3,
                )
            };
            vm.write_guest_memory(CODE_ADDRESS, &code).unwrap();
            vm.vcpus[0]
                .set_regs(&kvm_regs {
                    rip: CODE_ADDRESS,
                    rsp: STACK_ADDRESS,
                    rflags: u64::from(FLAGS_WITHOUT_TF),
                    ..Default::default()
                })
                .unwrap();
            for (index, expected_rip) in expected_rips.into_iter().enumerate() {
                assert!(!vm.step().unwrap());
                let regs = vm.vcpus[0].get_regs().unwrap();
                assert_eq!(regs.rip, expected_rip, "IRET={use_iret}, step={index}");
                assert_eq!(vm.scheduler.state().progress_counts()[0], index as u64 + 1);
                assert_eq!(vm.io_exit_count, 0, "compute was skipped to OUT");
            }
            let regs = vm.vcpus[0].get_regs().unwrap();
            assert_eq!(regs.rip, compute_address + 2);
            assert_eq!((regs.rax, regs.rbx), (1, 1));
        }
    }

    #[test]
    // r[verify chaoscontrol.event_entry_exact.evidence]
    // Known capability-negative diagnostic, not a production execution test.
    // Retain the one-instruction handler boundary: the observed RIP 0x201 with
    // AX/BX=1 after one debug exit is an overshoot, never the new expectation.
    #[ignore = "known portable KVM IRQ-entry overshoot; diagnostic only, strict SMP rejects"]
    fn diagnostic_irq_entry_does_not_meet_exact_instruction_contract() {
        const CODE_ADDRESS: u64 = 0x200;
        const HANDLER_ADDRESS: u64 = 0x300;
        const STACK_ADDRESS: u64 = 0x1000;
        const VECTOR: u8 = 0x51;
        let mut vm = diagnostic_fixtures::partial_smp_vm(VmConfig {
            memory_size: 4 * 1024 * 1024,
            num_vcpus: 2,
            ..VmConfig::default()
        });
        vm.write_guest_memory(CODE_ADDRESS, &[0x40, 0xe6, 0x10])
            .unwrap();
        vm.write_guest_memory(HANDLER_ADDRESS, &[0x43, 0xcf])
            .unwrap();
        let vector = [HANDLER_ADDRESS as u8, (HANDLER_ADDRESS >> 8) as u8, 0, 0];
        vm.write_guest_memory(u64::from(VECTOR) * 4, &vector)
            .unwrap();
        let mut sregs = vm.vcpus[0].get_sregs().unwrap();
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        sregs.ss.base = 0;
        sregs.ss.selector = 0;
        vm.vcpus[0].set_sregs(&sregs).unwrap();
        vm.vcpus[0]
            .set_regs(&kvm_regs {
                rip: CODE_ADDRESS,
                rsp: STACK_ADDRESS,
                rflags: 2 | (1 << 9),
                ..Default::default()
            })
            .unwrap();
        // Exercise the exact restored-event entry boundary. An IRQ merely
        // queued in an IOAPIC can still require KVM's interrupt-window work;
        // that does not prove it was selected for this particular RUN.
        // SET_VCPU_EVENTS is also the production checkpoint restore path.
        let mut events = vm.vcpus[0].get_vcpu_events().unwrap();
        events.interrupt.injected = 1;
        events.interrupt.nr = VECTOR;
        events.interrupt.soft = 0;
        events.interrupt.shadow = 0;
        vm.vcpus[0].set_vcpu_events(&events).unwrap();
        let admitted = vm.vcpus[0].get_vcpu_events().unwrap();
        assert_eq!(admitted.interrupt.injected, 1);
        assert_eq!(admitted.interrupt.nr, VECTOR);
        assert!(vm.vcpu_has_hlt_wake(0).unwrap());
        for (index, expected_rip) in [HANDLER_ADDRESS + 1, CODE_ADDRESS, CODE_ADDRESS + 1]
            .into_iter()
            .enumerate()
        {
            let before_regs = vm.vcpus[0].get_regs().unwrap();
            let before_events = vm.vcpus[0].get_vcpu_events().unwrap();
            assert!(!vm.step().unwrap());
            let after_regs = vm.vcpus[0].get_regs().unwrap();
            let after_events = vm.vcpus[0].get_vcpu_events().unwrap();
            assert_eq!(
                after_regs.rip, expected_rip,
                "step={index}; before_regs={before_regs:?}; before_events={before_events:?}; \
                 after_regs={after_regs:?}; after_events={after_events:?}"
            );
            assert_eq!(vm.scheduler.state().progress_counts()[0], index as u64 + 1);
            assert_eq!(vm.io_exit_count, 0);
        }
        let regs = vm.vcpus[0].get_regs().unwrap();
        assert_eq!((regs.rax, regs.rbx), (1, 1));
    }

    #[test]
    fn smp_guest_cpuid_hides_tsc_and_binds_jiffies_profile() {
        const CODE_ADDRESS: u64 = 0x200;
        // CPUID; OUT 0x10,AL. Inspect EDX after CPUID really runs in KVM.
        const CODE: [u8; 4] = [0x0f, 0xa2, 0xe6, 0x10];
        for num_vcpus in [1, 2] {
            let config = VmConfig {
                memory_size: 4 * 1024 * 1024,
                num_vcpus,
                ..VmConfig::default()
            };
            let mut vm = if num_vcpus == 1 {
                DeterministicVm::new(config).unwrap()
            } else {
                diagnostic_fixtures::partial_smp_vm(config)
            };
            vm.write_guest_memory(CODE_ADDRESS, &CODE).unwrap();
            let mut sregs = vm.vcpus[0].get_sregs().unwrap();
            sregs.cs.base = 0;
            sregs.cs.selector = 0;
            vm.vcpus[0].set_sregs(&sregs).unwrap();
            vm.vcpus[0]
                .set_regs(&kvm_regs {
                    rip: CODE_ADDRESS,
                    rax: 1,
                    rflags: 2,
                    ..Default::default()
                })
                .unwrap();
            assert!(matches!(
                vm.vcpus[0].run().unwrap(),
                VcpuExit::IoOut(0x10, _)
            ));
            let edx = vm.vcpus[0].get_regs().unwrap().rdx;
            assert_eq!(edx & u64::from(cpu::CPUID_1_EDX_TSC) == 0, num_vcpus > 1);
            let expected_clock = if num_vcpus > 1 {
                GuestClockMode::DeterministicJiffies
            } else {
                GuestClockMode::VirtualTsc
            };
            assert_eq!(
                vm.guest_determinism_profile.input.clock_mode,
                expected_clock
            );
            let snapshot = vm.snapshot().unwrap();
            vm.validate_fault_engine_snapshot(&snapshot).unwrap();
        }
    }

    fn halted_smp_test_vm() -> DeterministicVm {
        let mut vm = diagnostic_fixtures::partial_smp_vm(VmConfig {
            memory_size: 4 * 1024 * 1024,
            num_vcpus: 2,
            ..VmConfig::default()
        });
        let config = SchedulerConfig {
            num_vcpus: 2,
            progress_budget: ProgressBudget::Instructions(DEFAULT_SMP_INSTRUCTION_QUANTUM),
            strategy: SchedulingStrategy::RoundRobin,
            seed: 0,
        };
        vm.scheduler =
            VcpuScheduler::try_new(&config, ProgressMode::ExactSingleStep, vec![true; 2]).unwrap();
        for vcpu in 0..2 {
            vm.vcpus[vcpu]
                .set_mp_state(kvm_bindings::kvm_mp_state {
                    mp_state: KVM_MP_STATE_RUNNABLE,
                })
                .unwrap();
            let mut regs = vm.vcpus[vcpu].get_regs().unwrap();
            regs.rflags = 2 | (1 << 9);
            vm.vcpus[vcpu].set_regs(&regs).unwrap();
            let mut lapic = vm.vcpus[vcpu].get_lapic().unwrap();
            lapic.regs[0xf0..0xf4].copy_from_slice(&0x1ffu32.to_le_bytes().map(|b| b as i8));
            // Isolate LAPIC wake tests from the legacy PIC virtual wire.
            lapic.regs[0x350..0x354].copy_from_slice(&(1u32 << 16).to_le_bytes().map(|b| b as i8));
            vm.vcpus[vcpu].set_lapic(&lapic).unwrap();
            let reservation = vm.scheduler.reserve_transition().unwrap();
            let planned = plan_execution_observation(
                vm.scheduler.state(),
                ExecutionProgressObservation::ExactInstructionHalt { vcpu },
            )
            .unwrap()
            .unwrap();
            let record = vm.scheduler.commit(reservation, planned).unwrap();
            vm.hlt_latched_vcpus[vcpu] = true;
            vm.apply_schedule_action(&record.action).unwrap();
        }
        vm
    }

    #[test]
    fn pending_nmi_hlt_wake_respects_mask_and_both_interrupt_shadows() {
        let mut vm = halted_smp_test_vm();
        let vcpu = 1;
        let mut regs = vm.vcpus[vcpu].get_regs().unwrap();
        regs.rflags = 2; // NMI delivery is independent of IF.
        vm.vcpus[vcpu].set_regs(&regs).unwrap();
        let mut events = vm.vcpus[vcpu].get_vcpu_events().unwrap();
        events.flags |= kvm_bindings::KVM_VCPUEVENT_VALID_NMI_PENDING;
        events.nmi.pending = 1;
        for (masked, shadow) in [
            (1, 0),
            (0, kvm_bindings::KVM_X86_SHADOW_INT_MOV_SS as u8),
            (0, kvm_bindings::KVM_X86_SHADOW_INT_STI as u8),
        ] {
            events.nmi.masked = masked;
            events.interrupt.shadow = shadow;
            vm.vcpus[vcpu].set_vcpu_events(&events).unwrap();
            vm.refresh_hlt_wakes().unwrap();
            assert!(vm.hlt_latched_vcpus[vcpu]);
        }
        events.nmi.masked = 0;
        events.interrupt.shadow = 0;
        vm.vcpus[vcpu].set_vcpu_events(&events).unwrap();
        vm.refresh_hlt_wakes().unwrap();
        assert!(!vm.hlt_latched_vcpus[vcpu]);
        assert!(vm.scheduler.state().runnable_vcpus[vcpu]);
    }

    #[test]
    fn smp_hlt_wakes_on_targeted_ipi_but_not_if_or_priority_masked() {
        const VECTOR: u32 = 0x51;
        const X2APIC_ICR: u32 = 0x830;
        let mut vm = halted_smp_test_vm();
        let mut sregs = vm.vcpus[0].get_sregs().unwrap();
        sregs.apic_base |= (1 << 11) | (1 << 10);
        vm.vcpus[0].set_sregs(&sregs).unwrap();
        let msrs = kvm_bindings::Msrs::from_entries(&[kvm_bindings::kvm_msr_entry {
            index: X2APIC_ICR,
            data: (1u64 << 32) | u64::from(VECTOR),
            ..Default::default()
        }])
        .unwrap();
        assert_eq!(vm.vcpus[0].set_msrs(&msrs).unwrap(), 1);
        let mut regs = vm.vcpus[1].get_regs().unwrap();
        regs.rflags &= !(1 << 9);
        vm.vcpus[1].set_regs(&regs).unwrap();
        vm.refresh_hlt_wakes().unwrap();
        assert_eq!(vm.hlt_latched_vcpus, [true, true]);
        regs.rflags |= 1 << 9;
        vm.vcpus[1].set_regs(&regs).unwrap();
        let mut events = vm.vcpus[1].get_vcpu_events().unwrap();
        events.interrupt.shadow = 1;
        vm.vcpus[1].set_vcpu_events(&events).unwrap();
        vm.refresh_hlt_wakes().unwrap();
        assert_eq!(vm.hlt_latched_vcpus, [true, true]);
        events.interrupt.shadow = 0;
        vm.vcpus[1].set_vcpu_events(&events).unwrap();
        let mut lapic = vm.vcpus[1].get_lapic().unwrap();
        lapic.regs[0x80..0x84].copy_from_slice(&0x50u32.to_le_bytes().map(|b| b as i8));
        vm.vcpus[1].set_lapic(&lapic).unwrap();
        vm.refresh_hlt_wakes().unwrap();
        assert_eq!(vm.hlt_latched_vcpus, [true, true]);
        lapic.regs[0x80..0x84].fill(0);
        vm.vcpus[1].set_lapic(&lapic).unwrap();
        vm.vcpus[1]
            .set_mp_state(kvm_bindings::kvm_mp_state {
                mp_state: kvm_bindings::KVM_MP_STATE_HALTED,
            })
            .unwrap();
        let progress = vm.scheduler.state().progress_counts().to_vec();
        let sequence = vm.scheduler.state().sequence;
        vm.refresh_hlt_wakes().unwrap();
        assert_eq!(vm.hlt_latched_vcpus, [true, false]);
        assert_eq!(vm.scheduler.state().runnable_vcpus, [false, true]);
        assert_eq!(vm.observe_runnable_vcpus().unwrap(), [false, true]);
        assert_eq!(vm.scheduler.state().progress_counts(), progress);
        assert_eq!(vm.scheduler.state().sequence, sequence + 1);
        vm.refresh_hlt_wakes().unwrap();
        assert_eq!(vm.scheduler.state().sequence, sequence + 1);
        let snapshot = vm.snapshot().unwrap();
        vm.validate_fault_engine_snapshot(&snapshot).unwrap();
    }

    #[test]
    fn smp_hlt_pit_uses_one_deadline_and_respects_ioapic_mask_and_destination() {
        const VECTOR: u64 = 0x51;
        let mut vm = halted_smp_test_vm();
        let mut ioapic = kvm_bindings::kvm_irqchip {
            chip_id: kvm_bindings::KVM_IRQCHIP_IOAPIC,
            ..Default::default()
        };
        vm.vm.get_irqchip(&mut ioapic).unwrap();
        // SAFETY: chip identity selects IOAPIC; IRQ0 remains on default pin0.
        let mut state = unsafe { ioapic.chip.ioapic };
        state.redirtbl[0].bits = (1u64 << 56) | (1 << 16) | VECTOR;
        ioapic.chip.ioapic = state;
        vm.vm.set_irqchip(&ioapic).unwrap();
        let mut pit = vm.vm.get_pit2().unwrap();
        pit.channels[0].count = 1193;
        pit.channels[0].mode = 0;
        pit.channels[0].gate = 1;
        vm.vm.set_pit2(&pit).unwrap();
        vm.sync_and_suppress_pit().unwrap();
        let deadline = vm.pit.next_irq_tsc().unwrap();
        vm.virtual_tsc.set(deadline - cpu::DEFAULT_TSC_ADVANCE - 1);
        assert!(!vm.sync_and_suppress_pit().unwrap());
        vm.refresh_hlt_wakes().unwrap();
        assert_eq!(vm.hlt_latched_vcpus, [true, true]);
        vm.handle_hlt_exit().unwrap();
        assert_eq!(vm.virtual_tsc.read(), deadline);
        assert_eq!(vm.pit.irqs_delivered(), 1);
        vm.refresh_hlt_wakes().unwrap();
        assert_eq!(vm.hlt_latched_vcpus, [true, true]);
        state.redirtbl[0].bits = (1u64 << 56) | VECTOR;
        ioapic.chip.ioapic = state;
        vm.vm.set_irqchip(&ioapic).unwrap();
        vm.inject_interrupt(PIT_IRQ).unwrap();
        vm.refresh_hlt_wakes().unwrap();
        assert_eq!(vm.hlt_latched_vcpus, [true, false]);
        let delivered = vm.pit.irqs_delivered();
        let tsc = vm.virtual_tsc.read();
        vm.handle_hlt_exit().unwrap();
        assert_eq!(vm.pit.irqs_delivered(), delivered);
        assert_eq!(vm.virtual_tsc.read(), tsc + cpu::DEFAULT_TSC_ADVANCE);
        assert!(
            vm.pit.next_irq_tsc().is_none(),
            "one-shot PIT must not repeat"
        );
    }

    #[test]
    fn deterministic_smp_defaults_are_exact_and_bounded() {
        let config = VmConfig::default();
        assert_eq!(config.smp_progress_mode, ProgressMode::ExactSingleStep);
        assert_eq!(
            config.smp_schedule_journal_limit,
            DEFAULT_SCHEDULE_JOURNAL_LIMIT
        );
    }

    #[test]
    fn runtime_capacity_plan_is_admitted_before_vm_activation() {
        let config = VmConfig::default();
        let limits = runtime_capacity_limits(&config).expect("default runtime limits");
        let plan = plan_runtime_capacity(&limits).expect("default runtime capacity plan");
        assert_eq!(plan.journal_record_slots, DEFAULT_SCHEDULE_JOURNAL_LIMIT);
        assert_eq!(plan.scratch_slots, VIRTIO_SCRATCH_DEVICE_COUNT);
        assert_eq!(
            plan.packet_slots,
            crate::devices::virtio_types::DEFAULT_MAX_NET_TX_PACKETS
        );

        let mut invalid = config;
        invalid.smp_schedule_journal_limit = 0;
        let invalid_limits =
            runtime_capacity_limits(&invalid).expect("invalid limits are representable");
        assert!(matches!(
            plan_runtime_capacity(&invalid_limits),
            Err(chaoscontrol_sim_core::RuntimeCapacityError::Zero {
                field: chaoscontrol_sim_core::CapacityField::JournalRecordSlots,
            })
        ));
    }

    #[test]
    fn test_core_affinity_default_none() {
        let config = VmConfig::default();
        assert!(
            config.core_affinity.is_none(),
            "default should not pin to any core"
        );
    }

    #[test]
    fn test_core_affinity_config() {
        let config = VmConfig {
            core_affinity: Some(3),
            ..VmConfig::default()
        };
        let vm = DeterministicVm::new(config).unwrap();
        // VM should be created successfully with affinity set.
        // (We can't easily verify sched_setaffinity was called
        // in a unit test, but we verify the config propagates.)
        assert!(vm.exit_count() == 0);
    }

    // ─── Quick Win #1: VMCALL transport tests ───────────────────

    #[test]
    fn test_vmcall_nr_in_range() {
        // VMCALL_NR must fit in the 64-bit bitmask used by
        // KVM_CAP_EXIT_HYPERCALL
        const { assert!(VMCALL_NR < 64) };
    }

    #[test]
    fn test_vmcall_nr_no_conflict_with_kvm_builtins() {
        // KVM's built-in hypercalls are numbered 1-12.
        const { assert!(VMCALL_NR > 12) };
    }

    // ─── Dirty page tracking ────────────────────────────────────

    #[test]
    fn test_dirty_log_enabled_by_default() {
        let config = VmConfig::default();
        let vm = DeterministicVm::new(config).unwrap();
        assert!(vm.dirty_log_enabled());
    }

    // ─── Panic detection ────────────────────────────────────────

    #[test]
    fn test_panic_patterns_constants() {
        assert_eq!(PANIC_PATTERNS[0], u64::from_be_bytes(*b"Kernel p"));
        assert_eq!(PANIC_PATTERNS[1], u64::from_be_bytes(*b"---[ end"));
        assert_eq!(PANIC_PATTERNS[2], u64::from_be_bytes(*b"RIP: 001"));
        assert_eq!(PANIC_PATTERNS[3], u64::from_be_bytes(*b"end Kern"));
    }

    fn sliding_window_detects(input: &[u8]) -> bool {
        let mut state: u64 = 0;
        for &byte in input.iter() {
            state = (state << 8) | (byte as u64);
            if PANIC_PATTERNS.contains(&state) {
                return true;
            }
        }
        false
    }

    #[test]
    fn test_panic_sliding_window_detects_kernel_panic() {
        assert!(sliding_window_detects(b"Kernel panic - not syncing: fatal"));
    }

    #[test]
    fn test_panic_sliding_window_detects_end_trace() {
        assert!(sliding_window_detects(
            b"---[ end trace 0000000000000000 ]---"
        ));
    }

    #[test]
    fn test_panic_sliding_window_detects_rip_dump() {
        assert!(sliding_window_detects(
            b"RIP: 0010:entry_SYSCALL_64+0x0/0xe"
        ));
    }

    #[test]
    fn test_panic_sliding_window_no_false_positive() {
        assert!(!sliding_window_detects(
            b"Linux version 6.19 (root@builder) console=ttyS0 kernel loaded OK"
        ));
    }

    #[test]
    fn test_panic_sliding_window_partial_match_no_trigger() {
        assert!(!sliding_window_detects(b"Kernel loading..."));
    }

    #[test]
    fn test_panic_detection_initialized_false() {
        let config = VmConfig::default();
        let vm = DeterministicVm::new(config).unwrap();
        assert!(!vm.panic_detected, "panic_detected starts false");
        assert_eq!(vm.panic_match_state, 0, "match state starts at 0");
    }

    #[test]
    fn test_cmdline_uses_panic_zero() {
        let config = VmConfig::default();
        let cmdline = String::from_utf8_lossy(&config.cmdline);
        assert!(
            cmdline.contains("panic=0"),
            "default cmdline should use panic=0, got: {}",
            cmdline
        );
        assert!(
            !cmdline.contains("panic=-1"),
            "default cmdline should NOT contain panic=-1"
        );
    }

    #[test]
    fn test_build_cmdline_uses_panic_zero() {
        let config = VmConfig::default();
        let vm = DeterministicVm::new(config).unwrap();
        let bytes = vm.build_cmdline(0);
        let cmdline = String::from_utf8_lossy(&bytes);
        assert!(
            cmdline.contains("panic=0"),
            "build_cmdline should use panic=0, got: {}",
            cmdline
        );
        assert!(
            !cmdline.contains("panic=-1"),
            "build_cmdline should NOT contain panic=-1"
        );
    }
}
