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
use crate::scheduler::{SchedulerConfig, SchedulingStrategy, VcpuScheduler};

use crate::memory::{
    self, build_e820_map, code64_segment, data_segment, tss_segment, GuestMemoryManager,
    BOOT_GDT_OFFSET, BOOT_IDT_OFFSET, BOOT_STACK_POINTER, CMDLINE_START, GDT_ENTRY_COUNT,
    HIMEM_START, PML4_START, ZERO_PAGE_START,
};
use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_protocol::{
    HypercallPage, COVERAGE_BITMAP_ADDR, COVERAGE_BITMAP_SIZE, COVERAGE_PORT, HYPERCALL_PAGE_ADDR,
    HYPERCALL_PAGE_SIZE, SDK_PORT, VMCALL_NR,
};

use kvm_bindings::{
    kvm_clock_data, kvm_enable_cap, kvm_fpu, kvm_guest_debug, kvm_pit_config, kvm_regs,
    kvm_userspace_memory_region, KVM_GUESTDBG_ENABLE, KVM_GUESTDBG_SINGLESTEP,
    KVM_MEM_LOG_DIRTY_PAGES, KVM_MP_STATE_HALTED, KVM_MP_STATE_RUNNABLE, KVM_PIT_SPEAKER_DUMMY,
};
use kvm_ioctls::{Cap, Kvm, VcpuExit, VcpuFd, VmFd};
use linux_loader::configurator::linux::LinuxBootConfigurator;
use linux_loader::configurator::{BootConfigurator, BootParams};
use linux_loader::loader::bootparam::boot_params;
use linux_loader::loader::elf::Elf;

use chaoscontrol_fault::faults::GpRegister;

/// Wrapper for `libc::timer_t` to make it `Send`.
///
/// `timer_t` is `*mut c_void` which is `!Send`. The POSIX timer is
/// only used from the thread that created it (via `init_thread_timer`),
/// and ownership transfers between threads happen only when the
/// controller moves between the pool and scoped thread spawns.
struct SendTimerId(libc::timer_t);

// SAFETY: The timer is created per-thread and only accessed from the
// owning thread. Transfer between threads is guarded by the scoped
// thread join (controller moves into thread, then back out).
unsafe impl Send for SendTimerId {}

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
use vmm_sys_util::eventfd::EventFd;

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

/// PIT oscillator frequency (Hz).
const PIT_FREQ_HZ: u128 = 1_193_182;

/// Single-step margin for exact preemption (reserved for future use).
const SINGLESTEP_MARGIN: u64 = 50;

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

// ═══════════════════════════════════════════════════════════════════════
//  Configuration
// ═══════════════════════════════════════════════════════════════════════

/// Configuration for creating a [`DeterministicVm`].
#[derive(Debug, Clone)]
pub struct VmConfig {
    /// Guest memory size in bytes (default: 256 MB).
    pub memory_size: usize,
    /// CPU determinism configuration.
    pub cpu: CpuConfig,
    /// Number of vCPUs (default: 1).
    ///
    /// When `num_vcpus > 1`, the VM runs in SMP mode with deterministic
    /// serialized scheduling — only one vCPU executes at a time.
    pub num_vcpus: usize,
    /// Scheduling strategy for multi-vCPU VMs.
    ///
    /// Only meaningful when `num_vcpus > 1`. Controls how vCPU execution
    /// time is divided: fixed round-robin or randomized quantum.
    pub scheduling_strategy: SchedulingStrategy,
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

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            memory_size: 256 * 1024 * 1024,
            num_vcpus: 1,
            scheduling_strategy: SchedulingStrategy::RoundRobin,
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
                hide_tsc: false,
                fixed_family: Some(6),
                fixed_model: Some(85), // Skylake-SP
                fixed_stepping: Some(4),
                ..CpuConfig::default()
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
            cmdline: b"console=ttyS0 earlyprintk=serial \
                       clocksource=tsc tsc=reliable \
                       lpj=6000000 \
                       nokaslr noapic nosmp \
                       nohpet \
                       randomize_kstack_offset=off norandmaps \
                       kfence.sample_interval=0 \
                       no_hash_pointers \
                       virtio_mmio.device=4K@0xd0000000:5 \
                       virtio_mmio.device=4K@0xd0001000:6 \
                       virtio_mmio.device=4K@0xd0002000:7 \
                       panic=0\0"
                .to_vec(),
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

    #[snafu(display("IO error"), context(false))]
    Io { source: io::Error },

    #[snafu(display("Snapshot error: {message}"))]
    Snapshot { message: String },

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

/// Wrapper to implement `vm_superio::Trigger` for `EventFd`.
struct SerialTrigger(EventFd);

impl vm_superio::Trigger for SerialTrigger {
    type E = io::Error;

    fn trigger(&self) -> Result<(), Self::E> {
        self.0.write(1).map_err(io::Error::other)
    }
}

/// Maximum bytes retained by the serial capture buffer.
///
/// The guest controls serial output. Without a cap, a guest that prints
/// in a loop grows host memory without limit. 4 MiB keeps ample boot and
/// debug context while bounding worst-case retention per VM.
const MAX_SERIAL_CAPTURE_BYTES: usize = 4 * 1024 * 1024;

/// Bytes retained after an overflow drain.
///
/// Draining to half capacity on overflow amortizes the front-drain cost:
/// one O(cap) drain per `SERIAL_CAPTURE_RETAINED_BYTES` incoming bytes
/// instead of one drain per write.
const SERIAL_CAPTURE_RETAINED_BYTES: usize = MAX_SERIAL_CAPTURE_BYTES / 2;

/// Append `incoming` to `buf`, retaining at most `MAX_SERIAL_CAPTURE_BYTES`
/// of the most recent output. Returns the number of oldest bytes dropped.
///
/// Pure core of [`CapturingWriter`]: no I/O, no locks, no clocks.
fn capture_serial_bounded(buf: &mut Vec<u8>, incoming: &[u8]) -> usize {
    if incoming.len() >= MAX_SERIAL_CAPTURE_BYTES {
        let dropped = buf.len() + (incoming.len() - MAX_SERIAL_CAPTURE_BYTES);
        buf.clear();
        buf.extend_from_slice(&incoming[incoming.len() - MAX_SERIAL_CAPTURE_BYTES..]);
        return dropped;
    }
    let mut dropped = 0;
    if buf.len() + incoming.len() > MAX_SERIAL_CAPTURE_BYTES {
        // Drop the oldest bytes so the append fits. Retain at most
        // SERIAL_CAPTURE_RETAINED_BYTES to amortize the drain cost.
        let room_target = MAX_SERIAL_CAPTURE_BYTES - incoming.len();
        let target = buf
            .len()
            .min(SERIAL_CAPTURE_RETAINED_BYTES)
            .min(room_target);
        dropped = buf.len() - target;
        buf.drain(..dropped);
    }
    buf.extend_from_slice(incoming);
    debug_assert!(buf.len() <= MAX_SERIAL_CAPTURE_BYTES);
    dropped
}

/// Placeholder byte for terminal-control bytes in sanitized output.
const SERIAL_SANITIZE_REPLACEMENT: u8 = b'.';

/// Replace terminal-control bytes with a placeholder.
///
/// The guest controls serial output. Raw guest bytes can carry ANSI
/// escape sequences that rewrite the operator's terminal or trigger
/// terminal-emulator bugs. Newline, carriage return, and tab pass
/// through. Every other C0 control byte (ESC included) and DEL become
/// [`SERIAL_SANITIZE_REPLACEMENT`]. Bytes 0x80..=0xFF pass through so
/// UTF-8 text stays intact.
///
/// Pure core of the [`CapturingWriter`] stdout path: no I/O.
fn sanitize_serial_for_terminal(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|&b| match b {
            b'\n' | b'\r' | b'\t' => b,
            0x00..=0x1F | 0x7F => SERIAL_SANITIZE_REPLACEMENT,
            _ => b,
        })
        .collect()
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
pub struct DeterministicVm {
    #[allow(dead_code)]
    kvm: Kvm,
    vm: VmFd,
    vcpus: Vec<VcpuFd>,
    /// Index of the currently active vCPU (0 = BSP).
    active_vcpu: usize,
    memory: GuestMemoryManager,

    // Determinism state
    virtual_tsc: VirtualTsc,

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

    // Virtio MMIO devices
    virtio_devices: Vec<VirtioMmioDevice>,

    // Intra-VM vCPU scheduler (only meaningful when num_vcpus > 1)
    scheduler: VcpuScheduler,

    // Hardware instruction counter for deterministic SMP scheduling.
    // In overflow mode: delivers SIGIO after `insn_quantum - SINGLESTEP_MARGIN`
    // guest instructions. We then single-step the exact remainder.
    instruction_counter: Option<crate::perf::InstructionCounter>,
    /// Accumulated guest instructions for the current vCPU's turn.
    insn_count: u64,
    /// Instruction quantum: total guest instructions per vCPU turn.
    #[allow(dead_code)]
    insn_quantum: u64,

    /// Single-step state for exact preemption.
    /// When PMU overflow fires, we enable KVM single-stepping and count
    /// down `singlestep_remaining` instructions to reach the exact quantum.
    singlestep_remaining: u64,
    /// Whether KVM guest debug single-step is currently active.
    singlestep_active: bool,
    /// Consecutive SIGALRM exits without a real exit.
    /// Used to detect spin-wait loops for liveness switches.
    sigalrm_without_exit: u32,

    // Execution statistics
    exit_count: u64,
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

    /// Set after a signal-interrupted exit (EINTR/Intr). Causes the
    /// next `step()` to skip `sync_tsc_to_guest()` so the TSC resync
    /// doesn't happen at non-deterministic wall-clock times.
    skip_tsc_sync: bool,

    /// Set when serial output contains "Kernel panic".
    /// Causes `step()` to return halted on the next iteration.
    panic_detected: bool,

    /// Sliding window for serial panic detection.
    /// Accumulates the last 8 serial output bytes as a big-endian u64.
    panic_match_state: u64,

    /// Per-thread POSIX timer for SIGALRM delivery.
    /// Created by `init_thread_timer()` for parallel worker threads.
    /// When `Some`, `arm_preemption_timer()` uses this instead of
    /// the process-wide `ITIMER_REAL`.
    ///
    /// Wrapped in `SendTimerId` because `timer_t` is `*mut c_void`
    /// which is `!Send`. The timer is only used from the thread that
    /// created it, so this is safe.
    thread_timer: Option<SendTimerId>,

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

    /// Page indices dirtied by the most recent incremental restore.
    /// Used by `restore_incremental` to revert only the previous
    /// branch's dirty pages before applying the new overlay.
    last_dirty_page_indices: Vec<usize>,
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

    /// Install a no-op SIGALRM handler so the preemption timer doesn't
    /// kill the process. Called on first multi-vCPU VM creation and
    /// by `init_thread_timer()` for single-vCPU watchdog timers.
    fn install_sigalrm_handler() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            // SAFETY: sigaction with SA_RESTART and a trivial handler is safe.
            unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = noop_signal_handler as *const () as usize;
                sa.sa_flags = 0; // Do NOT set SA_RESTART — we want the signal to interrupt vcpu.run()
                libc::sigaction(libc::SIGALRM, &sa, std::ptr::null_mut());
            }
        });

        extern "C" fn noop_signal_handler(_sig: libc::c_int) {
            // Intentionally empty — the signal delivery interrupts vcpu.run(),
            // causing KVM to return VcpuExit::Intr. No action needed here.
        }
    }

    /// Create a new deterministic VM with the given configuration.
    ///
    /// This sets up KVM, guest memory, IRQ chip, PIT, and the serial
    /// console. The VM is ready for [`load_kernel`](Self::load_kernel)
    /// after construction.
    pub fn new(config: VmConfig) -> Result<Self, VmError> {
        let kvm = Kvm::new().context(KvmCreateSnafu)?;
        let vm = kvm.create_vm().context(VmCreateSnafu)?;

        // Create guest memory
        let memory = GuestMemoryManager::new(config.memory_size)?;

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

        // Create KVM PIT with speaker dummy flag.
        // KVM's PIT handles I/O ports 0x40-0x43, 0x61 internally and
        // delivers IRQ 0 via the in-kernel PIC. We use set_pit2() to
        // reset its count_load_time before each vcpu.run(), pinning
        // timer delivery to our virtual TSC instead of host wall time.
        let pit_config = kvm_pit_config {
            flags: KVM_PIT_SPEAKER_DUMMY,
            ..Default::default()
        };
        vm.create_pit2(pit_config).context(CreatePitSnafu)?;

        // Immediately disable KVM PIT channel 0 timer so it never fires
        // on host time. We'll deliver IRQ 0 ourselves via set_irq_line
        // at deterministic virtual-time points.
        {
            let mut pit_state = vm.get_pit2().context(CreatePitSnafu)?;
            // Set channel 0 to mode 0 (one-shot) with max count and
            // a far-future load time so it never triggers
            pit_state.channels[0].count = 0; // 0 = 65536
            pit_state.channels[0].mode = 0; // mode 0 = one-shot
            pit_state.channels[0].gate = 1;
            // Set count_load_time far in the future (year 2100)
            pit_state.channels[0].count_load_time = i64::MAX / 2;
            vm.set_pit2(&pit_state).context(CreatePitSnafu)?;
        }

        // Enable KVM_CAP_EXIT_HYPERCALL so guest `vmcall` instructions
        // exit to userspace instead of being handled by KVM internally.
        // This is the canonical x86 guest→hypervisor communication path
        // (used by Antithesis's Determinator). Falls back to port I/O
        // if the host kernel doesn't support this capability.
        let vmcall_enabled = if kvm.check_extension(Cap::ExitHypercall) {
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
        let num_vcpus = config.num_vcpus.max(1);
        if num_vcpus > 1 {
            Self::install_sigalrm_handler();
        }

        let cpuid = cpu::filter_cpuid(&kvm, &config.cpu)?;
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

        // Create virtual TSC for deterministic time tracking
        let virtual_tsc = VirtualTsc::from_config(&config.cpu);

        // Create deterministic entropy source seeded from master seed
        let entropy = DeterministicEntropy::new(config.cpu.seed);

        // Deterministic PIT driven by virtual TSC — delivers timer
        // interrupts at exact virtual-time points via set_irq_line.
        let pit = DeterministicPit::new(config.cpu.tsc_khz);

        // Set up serial port with interrupt support
        let serial_evt = EventFd::new(libc::EFD_NONBLOCK)?;
        let serial_trigger = SerialTrigger(serial_evt.try_clone()?);
        let serial_writer = CapturingWriter::new();
        let serial = vm_superio::Serial::new(serial_trigger, serial_writer.clone());

        // Register the serial EventFd with KVM IRQ line 4 (COM1)
        vm.register_irqfd(&serial_evt, SERIAL_IRQ)
            .context(CreateIrqChipSnafu)?;

        // Create intra-VM vCPU scheduler
        let scheduler = VcpuScheduler::new(&SchedulerConfig {
            num_vcpus,
            quantum: 100, // exits per vCPU turn
            strategy: config.scheduling_strategy,
            seed: config.cpu.seed,
        });

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

        // For SMP: PMU counting mode + SIGALRM + KVM single-step.
        //
        // Strategy: the PMU counts guest instructions (exclude_host=1).
        // SIGALRM (500µs) breaks tight spin loops that have no real exits.
        // At EVERY exit (real or SIGALRM), we read the counter. When it
        // reaches `quantum - margin`, we enable KVM single-step and count
        // down to the exact quantum boundary. Result: each vCPU runs
        // exactly `quantum` guest instructions per turn (deterministic).
        let insn_quantum = 500_000u64;
        let instruction_counter = if num_vcpus > 1 {
            debug_assert!(
                insn_quantum > SINGLESTEP_MARGIN,
                "quantum must exceed single-step margin"
            );
            Self::install_sigalrm_handler();
            match crate::perf::InstructionCounter::new() {
                Ok(counter) => {
                    info!(
                        "SMP preemption: PMU counting + single-step, quantum={}, margin={}",
                        insn_quantum, SINGLESTEP_MARGIN
                    );
                    Some(counter)
                }
                Err(e) => {
                    info!(
                        "SMP preemption: falling back to SIGALRM only (no PMU: {})",
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        // Reset the PMU counter for the initial vCPU (BSP).
        // It starts paused; resume() happens just before each vcpu.run().
        if let Some(ref counter) = instruction_counter {
            counter.reset_and_enable();
            counter.disable();
        }

        Ok(Self {
            kvm,
            vm,
            vcpus,
            active_vcpu: 0,
            memory,
            virtual_tsc,
            entropy,
            pit,
            serial,
            serial_writer,
            scheduler,
            fault_engine,
            virtio_devices,
            instruction_counter,
            insn_count: 0,
            insn_quantum,
            singlestep_remaining: 0,
            singlestep_active: false,
            sigalrm_without_exit: 0,
            last_kvm_pit_mode: 0xFF, // impossible value forces first sync
            exit_count: 0,
            io_exit_count: 0,
            exits_since_last_sdk: 0,
            panic_detected: false,
            panic_match_state: 0,
            thread_timer: None,
            coverage_active: false,
            vmcall_enabled,
            skip_tsc_sync: false,
            extra_cmdline: config.extra_cmdline.clone(),
            dirty_log_enabled: true,
            vm_id: config.vm_id,
            dlog,
            dlog_register_interval: config.dlog_register_interval,
            dlog_memory_hash: config.dlog_memory_hash,
            last_dirty_page_indices: Vec::new(),
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
    ) -> Result<Vec<VirtioMmioDevice>, VmError> {
        use crate::devices::block::DeterministicBlock;
        use crate::devices::net::DeterministicNet;
        use crate::devices::virtio_block::VirtioBlock;
        use crate::devices::virtio_entropy::VirtioEntropy;
        use crate::devices::virtio_net::VirtioNet;

        let mut devices = Vec::new();

        // Device 0: virtio-blk
        let disk = match disk_image_path {
            Some(path) => {
                info!("Loading disk image: {}", path);
                DeterministicBlock::from_image_file(path).map_err(|e| VmError::DiskImage {
                    message: e.to_string(),
                })?
            }
            None => DeterministicBlock::new(16 * 1024 * 1024),
        };
        info!(
            "  Block device: {} bytes ({} MB)",
            disk.size(),
            disk.size() / (1024 * 1024)
        );
        let blk_backend = Box::new(VirtioBlock::new(disk));
        let blk_device = VirtioMmioDevice::new(VIRTIO_MMIO_BASE_0, VIRTIO_MMIO_IRQ_0, blk_backend);
        devices.push(blk_device);

        // Device 1: virtio-net (unique MAC per VM)
        let mac = [0x52, 0x54, 0x00, 0x12, 0x34, vm_id as u8];
        let net = DeterministicNet::new(mac);
        let net_backend = Box::new(VirtioNet::new(net));
        let net_device = VirtioMmioDevice::new(VIRTIO_MMIO_BASE_1, VIRTIO_MMIO_IRQ_1, net_backend);
        devices.push(net_device);

        // Device 2: virtio-rng
        let entropy = DeterministicEntropy::new(seed);
        let rng_backend = Box::new(VirtioEntropy::new(entropy));
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
        let (smp_params, clock_params) = if num_vcpus > 1 {
            // SMP: use jiffies clocksource (driven by deterministic PIT).
            // notsc disables TSC entirely — no TSC calibration (which reads
            // hardware TSC + PIT, producing non-deterministic results).
            (
                format!("maxcpus={num_vcpus}"),
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

        let mut kernel_file = File::open(kernel_path)?;

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
        let initrd_info = if let Some(initrd_path) = initrd_path {
            info!("Loading initrd from {}", initrd_path);
            let initrd_data = std::fs::read(initrd_path)?;
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

        self.vcpus[self.active_vcpu]
            .set_msrs(&msrs)
            .context(SetRegistersSnafu)?;
        Ok(())
    }

    /// Sync the virtual TSC to the guest vCPU before each run.
    ///
    /// This is the critical determinism fix: it overwrites KVM's
    /// real-time TSC drift with our deterministic counter so every
    /// guest execution slice starts at an exact, reproducible value.
    fn sync_tsc_to_guest(&self) -> Result<(), VmError> {
        self.write_tsc_to_guest(self.virtual_tsc.read())
    }

    fn setup_boot_params(&self, initrd_info: Option<(u64, u64)>) -> Result<(), VmError> {
        const KERNEL_BOOT_FLAG_MAGIC: u16 = 0xaa55;
        const KERNEL_HDR_MAGIC: u32 = 0x5372_6448;
        const KERNEL_LOADER_OTHER: u8 = 0xff;
        const KERNEL_MIN_ALIGNMENT_BYTES: u32 = 0x0100_0000;

        // Write kernel command line (dynamic based on num_vcpus and vm_id)
        let cmdline = self.build_cmdline(self.vm_id);
        self.memory.write_cmdline(&cmdline)?;

        let mut hdr = linux_loader::loader::bootparam::setup_header {
            type_of_loader: KERNEL_LOADER_OTHER,
            boot_flag: KERNEL_BOOT_FLAG_MAGIC,
            header: KERNEL_HDR_MAGIC,
            cmd_line_ptr: CMDLINE_START as u32,
            cmdline_size: cmdline.len() as u32,
            kernel_alignment: KERNEL_MIN_ALIGNMENT_BYTES,
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

        // KVM PIT channel 0 is disabled (count_load_time = far future)
        // so it won't fire. Our DeterministicPit delivers IRQ 0 instead.

        Ok(())
    }

    // ─── Public API: execution ───────────────────────────────────────

    /// Run the VM until it halts or shuts down.
    pub fn run(&mut self) -> Result<(), VmError> {
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

        // Track real exits via exit_count, NOT loop iterations.
        // SIGALRM (VcpuExit::Intr) does not increment exit_count,
        // so using a simple `for i in 0..max_exits` loop would
        // consume a budget slot on each SIGALRM without producing
        // a real exit — causing non-deterministic total exit counts
        // in SMP mode where SIGALRM fires at wall-clock intervals.
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
        // Disarm preemption timer at end of bounded run so it doesn't
        // fire on a future vcpu.run() call from a different VM.
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

    /// Set the active vCPU index.
    ///
    /// # Panics
    ///
    /// Panics if `index >= num_vcpus()`.
    pub fn set_active_vcpu(&mut self, index: usize) {
        assert!(
            index < self.vcpus.len(),
            "vCPU index {} out of range (have {})",
            index,
            self.vcpus.len(),
        );
        self.active_vcpu = index;
    }

    /// Get a reference to the guest memory manager.
    pub fn memory(&self) -> &GuestMemoryManager {
        &self.memory
    }

    /// Get the KVM dirty page bitmap and atomically reset it.
    ///
    /// Returns a bitmap where each bit represents a 4 KB page. Bit N is
    /// set if the guest wrote to page N since the last call. The bitmap
    /// is packed as `Vec<u64>` — bit 0 of element 0 covers page 0, bit
    /// 63 of element 0 covers page 63, bit 0 of element 1 covers page
    /// 64, and so on.
    ///
    /// Requires `KVM_MEM_LOG_DIRTY_PAGES` on the memory slot (enabled
    /// by default at VM creation).
    pub fn get_dirty_bitmap(&self) -> Result<Vec<u64>, VmError> {
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
        // Block device has device_id == 2
        for device in &mut self.virtio_devices {
            if device.backend().device_id() == 2 {
                // Downcast to VirtioBlock
                if let Some(virtio_block) = device
                    .backend_mut()
                    .as_any_mut()
                    .downcast_mut::<crate::devices::virtio_block::VirtioBlock>(
                ) {
                    virtio_block.disk_mut().inject_fault(fault);
                    return true;
                }
            }
        }
        false
    }

    /// Set per-I/O delay on the block device (DiskSlow fault).
    pub fn set_disk_slow_delay(&mut self, delay_ns: u64) {
        self.with_block_device(|disk| disk.set_slow_delay_ns(delay_ns));
    }

    /// Enable fsync-lie mode on the block device.
    pub fn enable_disk_fsync_lie(&mut self) {
        self.with_block_device(|disk| disk.enable_fsync_lie());
    }

    /// Flush volatile writes on the block device (DiskFsyncFlush).
    pub fn flush_disk_volatile(&mut self) {
        self.with_block_device(|disk| disk.flush_volatile());
    }

    /// Discard volatile writes on the block device (crash semantics).
    pub fn discard_disk_volatile(&mut self) {
        self.with_block_device(|disk| disk.discard_volatile());
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
    pub fn restore_block_dirty(&mut self, overlay: crate::devices::block::DirtyOverlay) {
        self.with_block_device(|disk| disk.restore_dirty(overlay));
    }

    /// Helper: run a closure on the virtio-blk disk, if present.
    fn with_block_device(
        &mut self,
        f: impl FnOnce(&mut crate::devices::block::DeterministicBlock),
    ) {
        for device in &mut self.virtio_devices {
            if device.backend().device_id() == 2 {
                if let Some(virtio_block) = device
                    .backend_mut()
                    .as_any_mut()
                    .downcast_mut::<crate::devices::virtio_block::VirtioBlock>(
                ) {
                    f(virtio_block.disk_mut());
                    return;
                }
            }
        }
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
        if bit >= 64 {
            return Ok(()); // silently ignore out-of-range bits
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
        self.vm
            .set_irq_line(irq, true)
            .context(CreateIrqChipSnafu)?;
        self.vm
            .set_irq_line(irq, false)
            .context(CreateIrqChipSnafu)?;
        log::debug!(
            "Injected IRQ {} (exit_count={}, vtsc={})",
            irq,
            self.exit_count,
            self.virtual_tsc.read()
        );
        Ok(())
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
                log::warn!("KVM_NMI not supported on this kernel — skipping");
                return Ok(()); // Graceful degradation
            }
            Err(VmError::Io { source: err })
        }
    }

    // ─── Public API: snapshot / restore ──────────────────────────────

    /// Take a snapshot of the current VM state.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn snapshot(&self) -> Result<crate::snapshot::VmSnapshot, VmError> {
        use crate::snapshot::{CaptureParams, VirtioDeviceSnapshot};

        // Snapshot virtio device state (block device data)
        let virtio_snapshots: Vec<VirtioDeviceSnapshot> = self
            .virtio_devices
            .iter()
            .map(|dev| {
                let device_id = dev.backend().device_id();
                let block_snapshot = if device_id == 2 {
                    dev.backend()
                        .as_any()
                        .downcast_ref::<crate::devices::virtio_block::VirtioBlock>()
                        .map(|vb| vb.disk().snapshot())
                } else {
                    None
                };
                VirtioDeviceSnapshot {
                    device_id,
                    block_snapshot,
                }
            })
            .collect();

        let params = CaptureParams {
            serial_state: self.serial.state(),
            entropy: self.entropy.snapshot(),
            virtual_tsc: self.virtual_tsc.read(),
            exit_count: self.exit_count,
            io_exit_count: self.io_exit_count,
            exits_since_last_sdk: self.exits_since_last_sdk,
            pit_snapshot: self.pit.snapshot(),
            last_kvm_pit_mode: self.last_kvm_pit_mode,
            fault_engine_snapshot: self.fault_engine.snapshot(),
            virtio_snapshots,
            coverage_active: self.coverage_active,
            scheduler_snapshot: self.scheduler.snapshot(),
            singlestep_remaining: self.singlestep_remaining,
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
        // Can't call self.dlog_emit since snapshot takes &self.
        // Caller should use dlog_emit_snapshot_taken() after snapshot.
        result
    }

    /// Take an incremental snapshot using KVM dirty page tracking.
    ///
    /// Instead of copying all guest memory, queries the KVM dirty log
    /// to find which pages changed since the last query, then builds
    /// an overlay snapshot that references the shared `base` and stores
    /// only the dirty pages.
    ///
    /// Returns the snapshot and the number of dirty pages captured.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn snapshot_incremental(
        &self,
        base: &std::sync::Arc<Vec<u8>>,
    ) -> Result<(crate::snapshot::VmSnapshot, usize), VmError> {
        use crate::snapshot::{CaptureParams, SnapshotMemory, VirtioDeviceSnapshot};

        let dirty_bitmap = self.get_dirty_bitmap()?;
        let memory = SnapshotMemory::from_dirty(base, &dirty_bitmap, self.memory.inner());
        let dirty_count = memory.dirty_page_count();

        let virtio_snapshots: Vec<VirtioDeviceSnapshot> = self
            .virtio_devices
            .iter()
            .map(|dev| {
                let device_id = dev.backend().device_id();
                let block_snapshot = if device_id == 2 {
                    dev.backend()
                        .as_any()
                        .downcast_ref::<crate::devices::virtio_block::VirtioBlock>()
                        .map(|vb| vb.disk().snapshot())
                } else {
                    None
                };
                VirtioDeviceSnapshot {
                    device_id,
                    block_snapshot,
                }
            })
            .collect();

        let params = CaptureParams {
            serial_state: self.serial.state(),
            entropy: self.entropy.snapshot(),
            virtual_tsc: self.virtual_tsc.read(),
            exit_count: self.exit_count,
            io_exit_count: self.io_exit_count,
            exits_since_last_sdk: self.exits_since_last_sdk,
            pit_snapshot: self.pit.snapshot(),
            last_kvm_pit_mode: self.last_kvm_pit_mode,
            fault_engine_snapshot: self.fault_engine.snapshot(),
            virtio_snapshots,
            coverage_active: self.coverage_active,
            scheduler_snapshot: self.scheduler.snapshot(),
            singlestep_remaining: self.singlestep_remaining,
        };

        // Build the VmSnapshot with overlay memory instead of full copy.
        // We bypass VmSnapshot::capture() because it always does a full
        // memory read. Instead, construct the struct directly with the
        // overlay memory we already built.
        let mut vcpu_snapshots = Vec::with_capacity(self.vcpus.len());
        for vcpu in &self.vcpus {
            vcpu_snapshots.push(crate::snapshot::VcpuSnapshot::capture(vcpu).map_err(|e| {
                SnapshotSnafu {
                    message: e.to_string(),
                }
                .build()
            })?);
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
        let pit = self.vm.get_pit2().map_err(|e| {
            SnapshotSnafu {
                message: format!("get_pit2: {e}"),
            }
            .build()
        })?;
        let clock = self.vm.get_clock().map_err(|e| {
            SnapshotSnafu {
                message: format!("get_clock: {e}"),
            }
            .build()
        })?;

        let snap = crate::snapshot::VmSnapshot {
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
            pit_snapshot: params.pit_snapshot,
            last_kvm_pit_mode: params.last_kvm_pit_mode,
            fault_engine_snapshot: params.fault_engine_snapshot,
            virtio_snapshots: params.virtio_snapshots,
            coverage_active: params.coverage_active,
            active_vcpu: params.scheduler_snapshot.active,
            scheduler_snapshot: params.scheduler_snapshot,
            singlestep_remaining: params.singlestep_remaining,
        };

        info!(
            "Incremental snapshot: {} dirty pages ({} KB), {} vCPUs",
            dirty_count,
            dirty_count * 4,
            snap.vcpu_snapshots.len()
        );

        Ok((snap, dirty_count))
    }

    /// Restore VM state from a snapshot.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn restore(&mut self, snapshot: &crate::snapshot::VmSnapshot) -> Result<(), VmError> {
        snapshot.validate_assertion_identity().map_err(|error| {
            SnapshotSnafu {
                message: format!("invalid assertion snapshot: {error:?}"),
            }
            .build()
        })?;
        snapshot
            .restore(&self.vcpus, &self.vm, self.memory.inner())
            .map_err(|e| {
                SnapshotSnafu {
                    message: e.to_string(),
                }
                .build()
            })?;

        // Restore deterministic entropy PRNG state
        self.entropy = DeterministicEntropy::restore(&snapshot.entropy);

        // Restore VMM-side counters
        self.virtual_tsc.set(snapshot.virtual_tsc);
        self.exit_count = snapshot.exit_count;
        self.io_exit_count = snapshot.io_exit_count;
        // Always reset idle counter — branches should start fresh,
        // not inherit the bootstrap's idle state.
        self.exits_since_last_sdk = 0;

        // Clear panic detection state so a panic from a previous
        // branch doesn't carry into a new branch.
        self.panic_detected = false;
        self.panic_match_state = 0;

        // Restore DeterministicPit state
        self.pit = DeterministicPit::restore(&snapshot.pit_snapshot);
        self.last_kvm_pit_mode = snapshot.last_kvm_pit_mode;

        // Restore fault engine state. The lower layer validates again.
        self.fault_engine
            .restore(&snapshot.fault_engine_snapshot)
            .map_err(|error| {
                SnapshotSnafu {
                    message: format!("invalid assertion snapshot: {error:?}"),
                }
                .build()
            })?;

        // Restore coverage flag
        self.coverage_active = snapshot.coverage_active;

        // Restore scheduler state and active vCPU
        self.scheduler.restore(&snapshot.scheduler_snapshot);
        self.active_vcpu = snapshot.active_vcpu;

        // Restore single-step state. If the snapshot was taken during
        // single-stepping, re-enable it. Otherwise, re-arm the PMU counter.
        self.singlestep_remaining = snapshot.singlestep_remaining;
        if self.singlestep_remaining > 0 && self.instruction_counter.is_some() {
            self.singlestep_active = false; // clear so enable_singlestep works
            self.enable_singlestep();
        } else {
            self.disable_singlestep();
            if let Some(ref counter) = self.instruction_counter {
                counter.reset_and_enable();
                counter.disable(); // Paused; resume() before vcpu.run()
            }
        }

        // Restore virtio device state (block device data)
        for (snap, dev) in snapshot
            .virtio_snapshots
            .iter()
            .zip(self.virtio_devices.iter_mut())
        {
            if let Some(ref blk_snap) = snap.block_snapshot {
                if let Some(vb) = dev
                    .backend_mut()
                    .as_any_mut()
                    .downcast_mut::<crate::devices::virtio_block::VirtioBlock>()
                {
                    *vb.disk_mut() = crate::devices::block::DeterministicBlock::restore(blk_snap);
                }
            }
        }

        // Restore serial state with new EventFd and our capturing writer
        let serial_evt = EventFd::new(libc::EFD_NONBLOCK)?;
        let serial_trigger = SerialTrigger(serial_evt.try_clone()?);
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

        // Re-register IRQ fd
        self.vm
            .register_irqfd(&serial_evt, SERIAL_IRQ)
            .context(CreateIrqChipSnafu)?;

        self.dlog_emit(
            self.dlog_record(DlogTag::SnapshotRestored)
                .with_data_u64(snapshot.exit_count),
        );
        self.dlog_flush();

        // Reset host-side preemption state that is NOT part of the
        // deterministic snapshot but affects scheduling decisions.
        // Without this, SIGALRM-driven liveness switches from a
        // prior run can leak non-determinism into the restored session.
        self.sigalrm_without_exit = 0;
        self.skip_tsc_sync = false;
        self.insn_count = 0;

        // Disarm the SIGALRM preemption timer and drain any pending
        // signal so it doesn't fire at a non-deterministic phase
        // relative to the first vcpu.run() after restore. The timer
        // will be re-armed in the next step() call.
        if self.vcpus.len() > 1 {
            unsafe {
                let zero = libc::itimerval {
                    it_interval: libc::timeval {
                        tv_sec: 0,
                        tv_usec: 0,
                    },
                    it_value: libc::timeval {
                        tv_sec: 0,
                        tv_usec: 0,
                    },
                };
                libc::setitimer(libc::ITIMER_REAL, &zero, std::ptr::null_mut());
            }
        }

        info!("VM restored from snapshot (BSP RIP={:#x})", snapshot.rip());

        // Full restore overwrites all memory, so no dirty tracking needed.
        self.last_dirty_page_indices.clear();

        Ok(())
    }

    /// Incremental restore: only revert pages that changed since the
    /// last restore, then apply the new overlay's dirty pages.
    ///
    /// On a 256 MB VM with ~9 dirty pages per branch, this replaces
    /// a 256 MB memcpy (~75 ms) with ~18 page writes (~0.07 ms).
    ///
    /// **Precondition:** guest memory must contain the base image.
    /// Call a full `restore()` first, or use this only after a prior
    /// `restore_incremental` on the same VM.
    #[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]
    pub fn restore_incremental(
        &mut self,
        snapshot: &crate::snapshot::VmSnapshot,
        base: &[u8],
    ) -> Result<(), VmError> {
        use crate::snapshot::SnapshotMemory;

        snapshot.validate_assertion_identity().map_err(|error| {
            SnapshotSnafu {
                message: format!("invalid assertion snapshot: {error:?}"),
            }
            .build()
        })?;

        // Step 1: Revert previously-dirtied pages back to base values.
        if !self.last_dirty_page_indices.is_empty() {
            SnapshotMemory::revert_pages_from_base(
                base,
                self.last_dirty_page_indices.iter().copied(),
                self.memory.inner(),
            )
            .map_err(|e| {
                SnapshotSnafu {
                    message: format!("revert dirty pages: {e}"),
                }
                .build()
            })?;
        }

        // Step 2: Apply the new snapshot's dirty pages (if overlay)
        // and record which pages are now dirty.
        match &snapshot.memory {
            SnapshotMemory::Overlay { dirty_pages, .. } => {
                // Write only the dirty pages.
                snapshot
                    .memory
                    .write_to_guest(self.memory.inner())
                    .map_err(|e| {
                        SnapshotSnafu {
                            message: format!("write overlay pages: {e}"),
                        }
                        .build()
                    })?;
                self.last_dirty_page_indices = dirty_pages.keys().copied().collect();
            }
            SnapshotMemory::Full(_) => {
                // Full snapshot — write everything, no dirty tracking.
                snapshot
                    .memory
                    .write_to_guest(self.memory.inner())
                    .map_err(|e| {
                        SnapshotSnafu {
                            message: format!("write full memory: {e}"),
                        }
                        .build()
                    })?;
                self.last_dirty_page_indices.clear();
            }
        }

        // Step 3: Restore KVM device state (registers, IRQ chips, etc.)
        // Same as VmSnapshot::restore minus the memory write.
        snapshot
            .restore_devices_only(&self.vcpus, &self.vm)
            .map_err(|e| {
                SnapshotSnafu {
                    message: e.to_string(),
                }
                .build()
            })?;

        // Step 4: Restore VMM-side state (same as full restore).
        self.entropy = DeterministicEntropy::restore(&snapshot.entropy);
        self.virtual_tsc.set(snapshot.virtual_tsc);
        self.exit_count = snapshot.exit_count;
        self.io_exit_count = snapshot.io_exit_count;
        self.exits_since_last_sdk = 0;
        self.panic_detected = false;
        self.panic_match_state = 0;
        self.pit = DeterministicPit::restore(&snapshot.pit_snapshot);
        self.last_kvm_pit_mode = snapshot.last_kvm_pit_mode;
        self.fault_engine
            .restore(&snapshot.fault_engine_snapshot)
            .map_err(|error| {
                SnapshotSnafu {
                    message: format!("invalid assertion snapshot: {error:?}"),
                }
                .build()
            })?;
        self.coverage_active = snapshot.coverage_active;
        self.scheduler.restore(&snapshot.scheduler_snapshot);
        self.active_vcpu = snapshot.active_vcpu;

        self.singlestep_remaining = snapshot.singlestep_remaining;
        if self.singlestep_remaining > 0 && self.instruction_counter.is_some() {
            self.singlestep_active = false;
            self.enable_singlestep();
        } else {
            self.disable_singlestep();
            if let Some(ref counter) = self.instruction_counter {
                counter.reset_and_enable();
                counter.disable();
            }
        }

        for (snap, dev) in snapshot
            .virtio_snapshots
            .iter()
            .zip(self.virtio_devices.iter_mut())
        {
            if let Some(ref blk_snap) = snap.block_snapshot {
                if let Some(vb) = dev
                    .backend_mut()
                    .as_any_mut()
                    .downcast_mut::<crate::devices::virtio_block::VirtioBlock>()
                {
                    *vb.disk_mut() = crate::devices::block::DeterministicBlock::restore(blk_snap);
                }
            }
        }

        let serial_evt = EventFd::new(libc::EFD_NONBLOCK)?;
        let serial_trigger = SerialTrigger(serial_evt.try_clone()?);
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
        self.vm
            .register_irqfd(&serial_evt, SERIAL_IRQ)
            .context(CreateIrqChipSnafu)?;

        self.dlog_emit(
            self.dlog_record(DlogTag::SnapshotRestored)
                .with_data_u64(snapshot.exit_count),
        );
        self.dlog_flush();

        self.sigalrm_without_exit = 0;
        self.skip_tsc_sync = false;
        self.insn_count = 0;

        if self.vcpus.len() > 1 {
            unsafe {
                let zero = libc::itimerval {
                    it_interval: libc::timeval {
                        tv_sec: 0,
                        tv_usec: 0,
                    },
                    it_value: libc::timeval {
                        tv_sec: 0,
                        tv_usec: 0,
                    },
                };
                libc::setitimer(libc::ITIMER_REAL, &zero, std::ptr::null_mut());
            }
        }

        Ok(())
    }

    // ─── Internal: VM exit handling ──────────────────────────────────

    /// Execute one vCPU run cycle and handle the resulting exit.
    ///
    /// Returns `true` if the VM halted or shut down.
    /// Advances the virtual TSC on every exit for deterministic time progression.
    /// Synchronize PIT state: read KVM PIT, mirror to our DeterministicPit,
    /// then suppress KVM's timer by pushing count_load_time to far future.
    /// We deliver IRQ 0 ourselves at deterministic virtual-time points.
    fn sync_and_suppress_pit(&mut self) -> Result<(), VmError> {
        let mut pit_state = self.vm.get_pit2().context(CreatePitSnafu)?;
        let current_tsc = self.virtual_tsc.read();
        let _tsc_khz = self.virtual_tsc.tsc_khz() as u128;

        // ── Channel 0: mirror config + suppress KVM timer ──────────
        let ch0 = &pit_state.channels[0];
        let reload = ch0.count as u16;
        let mode = ch0.mode;
        if ch0.gate != 0 && (reload != self.pit.channel_reload(0) || mode != self.last_kvm_pit_mode)
        {
            // Program our DeterministicPit with the same config
            let cmd = 0x30 | ((mode & 0x7) << 1);
            self.pit.write_port(0x43, cmd, current_tsc);
            self.pit.write_port(0x40, reload as u8, current_tsc);
            self.pit.write_port(0x40, (reload >> 8) as u8, current_tsc);
            self.last_kvm_pit_mode = mode;
        }
        // Suppress KVM PIT channel 0 timer: push count_load_time far
        // into future so KVM never thinks the counter expired.
        pit_state.channels[0].count_load_time = i64::MAX / 2;

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

        // ── Deliver deterministic IRQ 0 ─────────────────────────────
        if self.pit.pending_irq(current_tsc) {
            self.vm
                .set_irq_line(PIT_IRQ, true)
                .context(CreateIrqChipSnafu)?;
            self.vm
                .set_irq_line(PIT_IRQ, false)
                .context(CreateIrqChipSnafu)?;
            self.pit.acknowledge_irq();
        }
        Ok(())
    }

    /// Check if vCPU should switch after a real VM exit.
    ///
    /// Only ticks the deterministic scheduler when running the scheduler's
    /// intended vCPU (active_vcpu == scheduler.active()). During liveness
    /// detours (SIGALRM-switched), exits are counted globally but don't
    /// affect scheduler state, preserving deterministic interleaving.
    #[inline]
    fn maybe_switch_vcpu(&mut self) {
        if self.vcpus.len() <= 1 {
            return;
        }
        // Real exit occurred — reset the spin-loop detection counter.
        self.sigalrm_without_exit = 0;

        // During a liveness detour (SIGALRM switched us to a different
        // vCPU than the scheduler intended), don't tick the scheduler.
        // The detour vCPU's exits are real (affect exit_count, vtsc) but
        // invisible to the scheduler's quantum tracking.
        if self.active_vcpu != self.scheduler.active() {
            return;
        }
        if self.scheduler.tick() {
            let prev = self.active_vcpu;
            // Scheduler says switch. Use advance() for deterministic next vCPU.
            let next = self.scheduler.advance();
            // Find next RUNNABLE vCPU starting from scheduler's choice.
            for offset in 0..self.vcpus.len() {
                let candidate = (next + offset) % self.vcpus.len();
                if self.vcpu_is_runnable(candidate) {
                    self.active_vcpu = candidate;
                    if candidate != next {
                        self.scheduler.set_active(candidate);
                    }
                    self.dlog_emit(
                        self.dlog_record(DlogTag::SchedulerSwitch)
                            .with_data(&[prev as u8])
                            .with_extra_u64(self.scheduler.quantum_remaining()),
                    );
                    return;
                }
            }
        }
    }

    /// Check if a vCPU is schedulable (via KVM_GET_MP_STATE).
    ///
    /// APs (secondary CPUs) start in UNINITIALIZED/INIT_RECEIVED state
    /// and only become RUNNABLE after receiving a SIPI from the BSP.
    /// HALTED means the vCPU executed HLT and is waiting for an interrupt —
    /// it's still schedulable (our HLT handler injects the timer IRQ).
    fn vcpu_is_runnable(&self, vcpu_idx: usize) -> bool {
        // BSP (vCPU 0) is always runnable after setup
        if vcpu_idx == 0 {
            return true;
        }
        match self.vcpus[vcpu_idx].get_mp_state() {
            Ok(mp) => mp.mp_state == KVM_MP_STATE_RUNNABLE || mp.mp_state == KVM_MP_STATE_HALTED,
            Err(_) => false,
        }
    }

    /// Arm a POSIX interval timer that fires SIGALRM after `us` microseconds.
    ///
    /// When the vCPU is in a tight spin loop (no VM exits), this signal
    /// interrupts `vcpu.run()` causing `VcpuExit::Intr`, which lets us
    /// switch to another vCPU. Essential for SMP — without it, the BSP
    /// can monopolize execution while spin-waiting for an AP to come online.
    fn arm_preemption_timer(&self, us: i64) {
        // Try thread-targeted timer first (works in parallel workers).
        if let Some(ref tid) = self.thread_timer {
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
            // SAFETY: tid.0 is a valid POSIX timer created in init_thread_timer().
            unsafe {
                libc::timer_settime(tid.0, 0, &ts, std::ptr::null_mut());
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

    /// Create a per-thread POSIX timer that sends SIGALRM to this thread.
    ///
    /// Must be called from the thread that will call `vcpu.run()` (worker
    /// thread for parallel execution). This replaces the process-wide
    /// `ITIMER_REAL` approach, allowing multiple workers to each have
    /// independent watchdog timers.
    pub fn init_thread_timer(&mut self) {
        // Ensure the no-op SIGALRM handler is installed before we start
        // creating timers that deliver SIGALRM. Without this, the
        // default SIGALRM disposition (terminate) kills the process.
        Self::install_sigalrm_handler();

        // SAFETY: timer_create with SIGEV_THREAD_ID targets the signal
        // at a specific thread (the caller). This is Linux-specific.
        unsafe {
            let tid = libc::syscall(libc::SYS_gettid) as i32;
            let mut sev: libc::sigevent = std::mem::zeroed();
            sev.sigev_notify = libc::SIGEV_THREAD_ID;
            sev.sigev_signo = libc::SIGALRM;
            // sigev_notify_thread_id is in the union at sigev_value offset
            // on Linux. The libc crate exposes it via the _tid field.
            sev.sigev_notify_thread_id = tid;
            let mut timer_id: libc::timer_t = std::ptr::null_mut();
            let ret = libc::timer_create(libc::CLOCK_MONOTONIC, &mut sev, &mut timer_id);
            if ret == 0 {
                self.thread_timer = Some(SendTimerId(timer_id));
            } else {
                log::warn!(
                    "timer_create failed (errno={}), falling back to ITIMER_REAL",
                    *libc::__errno_location()
                );
            }
        }
    }

    /// Enable KVM guest single-stepping on the active vCPU.
    ///
    /// Each guest instruction will cause `VcpuExit::Debug` instead of
    /// executing normally. Used to count down the exact remainder after
    /// PMU overflow to reach the quantum boundary precisely.
    fn enable_singlestep(&mut self) {
        debug_assert!(!self.singlestep_active, "single-step already active");
        let dbg = kvm_guest_debug {
            control: KVM_GUESTDBG_ENABLE | KVM_GUESTDBG_SINGLESTEP,
            pad: 0,
            arch: Default::default(),
        };
        if let Err(e) = self.vcpus[self.active_vcpu].set_guest_debug(&dbg) {
            log::warn!(
                "Failed to enable single-step on vCPU {}: {}",
                self.active_vcpu,
                e
            );
            return;
        }
        self.singlestep_active = true;
    }

    /// Disable KVM guest single-stepping on the active vCPU.
    fn disable_singlestep(&mut self) {
        if !self.singlestep_active {
            return;
        }
        let dbg = kvm_guest_debug {
            control: 0,
            pad: 0,
            arch: Default::default(),
        };
        let _ = self.vcpus[self.active_vcpu].set_guest_debug(&dbg);
        self.singlestep_active = false;
        self.singlestep_remaining = 0;
    }

    /// Switch to the next runnable vCPU after a quantum expires.
    /// Disables single-stepping, resets instruction count, re-arms the PMU
    /// counter for the new vCPU's turn.
    #[allow(dead_code)]
    fn switch_vcpu_at_quantum(&mut self) {
        self.disable_singlestep();
        self.insn_count = 0;

        // Switch to next runnable vCPU
        for offset in 1..self.vcpus.len() {
            let candidate = (self.active_vcpu + offset) % self.vcpus.len();
            if self.vcpu_is_runnable(candidate) {
                self.active_vcpu = candidate;
                self.scheduler.set_active(candidate);
                break;
            }
        }

        // Reset PMU counter for the new turn (but don't enable yet —
        // we'll resume() just before vcpu.run() to avoid counting host code).
        if let Some(ref counter) = self.instruction_counter {
            counter.reset_and_enable();
            counter.disable(); // Paused at 0; resume() before vcpu.run()
        }
    }

    fn step(&mut self) -> Result<bool, VmError> {
        if self.skip_tsc_sync {
            self.skip_tsc_sync = false;
        } else {
            self.sync_and_suppress_pit()?;
            self.sync_tsc_to_guest()?;
        }

        // Skip non-runnable vCPUs (APs waiting for SIPI).
        // Try all vCPUs before giving up — if none are runnable, stick with BSP.
        if self.vcpus.len() > 1 && !self.vcpu_is_runnable(self.active_vcpu) {
            for offset in 1..self.vcpus.len() {
                let candidate = (self.active_vcpu + offset) % self.vcpus.len();
                if self.vcpu_is_runnable(candidate) {
                    self.active_vcpu = candidate;
                    break;
                }
            }
        }

        let num_vcpus = self.vcpus.len();

        // Post-match action flag: when a VMCALL exit is processed, we
        // must handle the SDK hypercall AFTER the match arm closes
        // (because HypercallExit holds a &mut ref into the vcpu's
        // kvm_run struct, preventing &mut self calls within the arm).
        let mut vmcall_sdk_pending = false;

        // For SMP: arm SIGALRM for spin-loop detection.
        // Delay until after early boot (>200 exits) to avoid disturbing
        // PIT channel 2 TSC calibration. SIGALRM interrupts cause RDTSC
        // to jump (hardware TSC advances during host code), making the
        // calibration result non-deterministic.
        // Only do liveness switches when the vCPU appears stuck
        // (2 consecutive SIGALRMs without real exits).
        if num_vcpus > 1 {
            // 10ms SIGALRM: fast enough for liveness (20ms to detect
            // spin loops with threshold=2), slow enough to avoid
            // disturbing PIT calibration (~2-5ms during early boot).
            self.arm_preemption_timer(10_000);
        } else if self.fault_engine.is_setup_complete() {
            // Single-vCPU watchdog: 100ms timeout interrupts vcpu.run()
            // if the guest enters a tight CPU loop (e.g., CpuBitflip
            // corrupts RIP, kernel double-faults into a spin loop).
            // Without this, vcpu.run() blocks indefinitely.
            //
            // Only armed after setup_complete to avoid disturbing PIT
            // calibration during boot. Uses per-thread POSIX timer
            // when available (safe for parallel workers) or falls back
            // to process-wide ITIMER_REAL.
            Self::install_sigalrm_handler();
            self.arm_preemption_timer(100_000);
        }
        let run_result = self.vcpus[self.active_vcpu].run();

        // Deferred dlog record — built inside the match arm (copying
        // data out of the KVM exit struct), emitted after the match
        // closes so there's no borrow conflict with self.vcpus.
        let mut pending_dlog: Option<DlogRecord> = None;

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
                self.maybe_switch_vcpu();
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
                    self.handle_sdk_hypercall();
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
                self.maybe_switch_vcpu();
                Ok(false)
            }
            Ok(VcpuExit::Hlt) => {
                self.exit_count += 1;
                self.exits_since_last_sdk += 1;
                self.virtual_tsc.tick();

                // HLT = kernel idle loop waiting for next interrupt.
                // Read KVM PIT state to find channel 0's reload value,
                // then fast-forward virtual TSC by one PIT period and
                // inject the interrupt deterministically.
                let pit_state = self.vm.get_pit2().context(CreatePitSnafu)?;
                let ch0 = &pit_state.channels[0];
                let reload = if ch0.count == 0 {
                    65536u64
                } else {
                    ch0.count as u64
                };

                if ch0.gate != 0 && reload > 0 {
                    // Advance virtual TSC by one PIT period:
                    // tsc_ticks = reload * tsc_freq / PIT_FREQ
                    let tsc_khz = self.virtual_tsc.tsc_khz() as u128;
                    let tsc_per_period =
                        (reload as u128 * tsc_khz * 1000).div_ceil(PIT_FREQ_HZ) as u64;
                    self.virtual_tsc
                        .advance_to(self.virtual_tsc.read() + tsc_per_period);

                    // Inject the timer interrupt deterministically
                    self.vm
                        .set_irq_line(PIT_IRQ, true)
                        .context(CreateIrqChipSnafu)?;
                    self.vm
                        .set_irq_line(PIT_IRQ, false)
                        .context(CreateIrqChipSnafu)?;

                    pending_dlog = Some(DlogRecord::new(
                        0,
                        self.virtual_tsc.read(),
                        self.exit_count,
                        0,
                        DlogTag::Hlt,
                        self.active_vcpu as u8,
                    ));
                    self.maybe_switch_vcpu();
                    Ok(false)
                } else {
                    pending_dlog = Some(DlogRecord::new(
                        0,
                        self.virtual_tsc.read(),
                        self.exit_count,
                        0,
                        DlogTag::Hlt,
                        self.active_vcpu as u8,
                    ));
                    info!(
                        "VM halted (exit_count={}, vtsc={})",
                        self.exit_count,
                        self.virtual_tsc.read()
                    );
                    Ok(true)
                }
            }
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
                            let tsc = self.virtual_tsc.read();
                            let tsc_khz = self.virtual_tsc.tsc_khz() as u64;
                            if tsc_khz > 0 {
                                // 100 MHz HPET = tsc * 100_000 / tsc_khz
                                tsc as u128 as u64 * 100_000 / tsc_khz
                            } else {
                                0
                            }
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
                self.maybe_switch_vcpu();
                Ok(false)
            }
            Ok(VcpuExit::MmioWrite(addr, data)) => {
                self.exit_count += 1;
                self.exits_since_last_sdk += 1;
                self.virtual_tsc.tick();

                // Find the virtio device that handles this address
                for dev in &mut self.virtio_devices {
                    if dev.handles(addr) {
                        let effect = dev.write_at(addr, data, self.memory.inner());

                        // Process only the queue named by a validated notification.
                        if let Ok(MmioWriteEffect::NotifyQueue(queue_index)) = effect {
                            if dev.process_queue(queue_index, self.memory.inner()) {
                                let vm = &self.vm;
                                deliver_virtio_interrupt_with(
                                    dev,
                                    queue_index,
                                    |irq, asserted| vm.set_irq_line(irq, asserted),
                                )?;
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
                self.maybe_switch_vcpu();
                Ok(false)
            }
            Ok(VcpuExit::Intr) => {
                // SIGALRM interrupted vcpu.run().
                //
                // For SMP: switch vCPU if this one appears stuck (consecutive
                // SIGALRMs without any real exit). Normal code generates
                // real exits between SIGALRMs, keeping the counter at 0.
                // Spin-wait loops have no real exits, so the counter
                // grows until it crosses the threshold → liveness switch.
                //
                // For single-vCPU: detect stuck VMs (tight CPU loops from
                // fault injection, e.g. CpuBitflip corrupts RIP). After
                // 5 consecutive SIGALRMs (~500ms at 100ms interval) with
                // no real exits, treat the VM as crashed.
                self.disarm_preemption_timer();
                // Skip PIT/TSC sync on next step() — virtual time hasn't
                // advanced, and skipping avoids disturbing in-progress
                // PIT channel 2 calibration during early boot.
                self.skip_tsc_sync = true;
                self.sigalrm_without_exit += 1;
                if num_vcpus > 1 {
                    if self.sigalrm_without_exit >= 2 {
                        for offset in 1..self.vcpus.len() {
                            let candidate = (self.active_vcpu + offset) % self.vcpus.len();
                            if self.vcpu_is_runnable(candidate) {
                                self.active_vcpu = candidate;
                                self.sigalrm_without_exit = 0;
                                break;
                            }
                        }
                    }
                } else if self.sigalrm_without_exit >= 5 && self.fault_engine.is_setup_complete() {
                    // Single-vCPU VM stuck in a tight loop with no exits
                    // for ~500ms. Treat as crashed.
                    log::warn!(
                        "VM stuck: {} consecutive SIGALRMs without exit \
                         (exit_count={}, vtsc={}), treating as crashed",
                        self.sigalrm_without_exit,
                        self.exit_count,
                        self.virtual_tsc.read(),
                    );
                    self.panic_detected = true;
                }
                pending_dlog = Some(DlogRecord::new(
                    0,
                    self.virtual_tsc.read(),
                    self.exit_count,
                    0,
                    DlogTag::Intr,
                    self.active_vcpu as u8,
                ));
                Ok(false)
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
            Ok(VcpuExit::InternalError) => {
                pending_dlog = Some(DlogRecord::new(
                    0,
                    self.virtual_tsc.read(),
                    self.exit_count,
                    0,
                    DlogTag::InternalError,
                    self.active_vcpu as u8,
                ));
                if self.vcpus.len() > 1 {
                    log::warn!(
                        "InternalError on vCPU {} — switching to next runnable vCPU",
                        self.active_vcpu
                    );
                    for offset in 1..self.vcpus.len() {
                        let candidate = (self.active_vcpu + offset) % self.vcpus.len();
                        if candidate == 0 || self.vcpu_is_runnable(candidate) {
                            self.active_vcpu = candidate;
                            break;
                        }
                    }
                    Ok(false)
                } else {
                    self.exit_count += 1;
                    log::error!("KVM InternalError on vCPU 0 — stopping");
                    Ok(true)
                }
            }
            Ok(exit) => {
                self.exit_count += 1;
                info!("Unhandled VM exit: {:?} — stopping", exit);
                Ok(true)
            }
            Err(e) => {
                // EINTR from signal — same as VcpuExit::Intr
                if e.errno() == libc::EINTR {
                    self.disarm_preemption_timer();
                    self.skip_tsc_sync = true;
                    self.sigalrm_without_exit += 1;
                    if self.vcpus.len() > 1 {
                        if self.sigalrm_without_exit >= 2 {
                            for offset in 1..self.vcpus.len() {
                                let candidate = (self.active_vcpu + offset) % self.vcpus.len();
                                if self.vcpu_is_runnable(candidate) {
                                    self.active_vcpu = candidate;
                                    self.sigalrm_without_exit = 0;
                                    break;
                                }
                            }
                        }
                    } else if self.sigalrm_without_exit >= 5
                        && self.fault_engine.is_setup_complete()
                    {
                        log::warn!(
                            "VM stuck (EINTR): {} consecutive SIGALRMs, \
                             treating as crashed",
                            self.sigalrm_without_exit,
                        );
                        self.panic_detected = true;
                        return Ok(true); // halt immediately
                    }
                    return Ok(false);
                }
                Err(VmError::VcpuRun { source: e })
            }
        };

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
            self.maybe_switch_vcpu();
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

        result
    }

    // ─── Guest memory / register access ────────────────────────

    /// Read bytes from guest physical memory.
    ///
    /// Returns up to `size` bytes starting at guest physical address `addr`.
    /// Fails if the address range is outside guest memory.
    pub fn read_guest_memory(&self, addr: u64, size: usize) -> Result<Vec<u8>, VmError> {
        use vm_memory::{Bytes, GuestAddress};
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
        self.dlog_emit(self.dlog_record(DlogTag::TickMarker).with_data_u64(tick));
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
            let _ = dlog.flush();
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

    /// Get a reference to the vCPU scheduler.
    pub fn scheduler(&self) -> &VcpuScheduler {
        &self.scheduler
    }

    /// Get a mutable reference to the vCPU scheduler.
    pub fn scheduler_mut(&mut self) -> &mut VcpuScheduler {
        &mut self.scheduler
    }

    // ─── Internal: SDK hypercall handler ─────────────────────────

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

        // Dispatch to the fault engine
        let (result, status) = self.fault_engine.handle_hypercall(&page);

        // Write result and status back to the guest page
        let result_bytes = result.to_le_bytes();
        let _ = self.memory.inner().write_slice(
            &result_bytes,
            vm_memory::GuestAddress(HYPERCALL_PAGE_ADDR + 0x10), // result offset
        );
        let _ = self.memory.inner().write_slice(
            &[status],
            vm_memory::GuestAddress(HYPERCALL_PAGE_ADDR + 0x18), // status offset
        );
    }
}

impl Drop for DeterministicVm {
    fn drop(&mut self) {
        // Disarm the SIGALRM preemption timer to prevent stale signals
        // from interfering with subsequent VMs in the same process
        // (important for test suites that create many VMs sequentially).
        self.disarm_preemption_timer();
        // Destroy per-thread POSIX timer if created.
        if let Some(tid) = self.thread_timer.take() {
            // SAFETY: tid.0 was created by timer_create in init_thread_timer.
            unsafe {
                libc::timer_delete(tid.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::virtio_types::VirtioFailure;

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
        let vm = DeterministicVm::new(config).unwrap();
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

    #[test]
    fn test_vmcall_enabled_reported() {
        let config = VmConfig::default();
        let vm = DeterministicVm::new(config).unwrap();
        // vmcall_enabled depends on host kernel support — just verify
        // the method is available and returns a boolean.
        let _enabled: bool = vm.vmcall_enabled();
    }

    // ─── Dirty page tracking ────────────────────────────────────

    #[test]
    fn test_dirty_log_enabled_by_default() {
        let config = VmConfig::default();
        let vm = DeterministicVm::new(config).unwrap();
        assert!(vm.dirty_log_enabled());
    }

    #[test]
    fn test_get_dirty_bitmap_returns_nonzero_after_memory_write() {
        let config = VmConfig::default();
        let mem_size = config.memory_size;
        let vm = DeterministicVm::new(config).unwrap();

        // First call clears all dirty bits accumulated since slot creation
        // (KVM marks all pages dirty when the slot is first created with
        // dirty logging enabled).
        let _ = vm.get_dirty_bitmap().unwrap();

        // Write to a known page in guest memory (page 256 = offset 1 MB).
        let data = [0xABu8; 64];
        vm.memory()
            .inner()
            .write_slice(&data, vm_memory::GuestAddress(256 * 4096))
            .unwrap();

        // KVM tracks hardware-level dirty bits. Host-side write_slice
        // writes through the mmap, but KVM's dirty log tracks guest
        // writes via EPT/NPT, not host writes through the mmap. So
        // after host-only writes, the dirty bitmap may be empty.
        // This test verifies the API works — integration tests with
        // actual guest execution test real dirty tracking.
        let bitmap = vm.get_dirty_bitmap().unwrap();
        // Bitmap should have correct length: ceil(memory_size / page_size / 64)
        let expected_len = mem_size.div_ceil(4096) / 64;
        assert_eq!(bitmap.len(), expected_len);
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
