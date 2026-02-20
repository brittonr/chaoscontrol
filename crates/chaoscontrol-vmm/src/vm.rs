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
use crate::devices::virtio_mmio::VirtioMmioDevice;
use crate::scheduler::{SchedulerConfig, SchedulingStrategy, VcpuScheduler};

use crate::memory::{
    self, build_e820_map, code64_segment, data_segment, tss_segment, GuestMemoryManager,
    BOOT_GDT_OFFSET, BOOT_IDT_OFFSET, BOOT_STACK_POINTER, CMDLINE_START, GDT_ENTRY_COUNT,
    HIMEM_START, PML4_START, ZERO_PAGE_START,
};
use chaoscontrol_fault::engine::{EngineConfig, FaultEngine};
use chaoscontrol_protocol::{
    HypercallPage, COVERAGE_BITMAP_ADDR, COVERAGE_BITMAP_SIZE, COVERAGE_PORT, HYPERCALL_PAGE_ADDR,
    HYPERCALL_PAGE_SIZE, SDK_PORT,
};

use kvm_bindings::{
    kvm_clock_data, kvm_fpu, kvm_guest_debug, kvm_pit_config, kvm_regs,
    kvm_userspace_memory_region, KVM_GUESTDBG_ENABLE, KVM_GUESTDBG_SINGLESTEP, KVM_MP_STATE_HALTED,
    KVM_MP_STATE_RUNNABLE, KVM_PIT_SPEAKER_DUMMY,
};
use kvm_ioctls::{Kvm, VcpuExit, VcpuFd, VmFd};
use linux_loader::configurator::linux::LinuxBootConfigurator;
use linux_loader::configurator::{BootConfigurator, BootParams};
use linux_loader::loader::bootparam::boot_params;
use linux_loader::loader::elf::Elf;
use linux_loader::loader::KernelLoader;
use log::info;
use snafu::{ResultExt, Snafu};
use std::fs::File;
use std::io;
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

/// COM1 IRQ line number (standard PC).
const SERIAL_IRQ: u32 = 4;

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
            // no-kvmclock: prevent kvm-clock from being registered as clocksource
            // lpj=6000000: fixed loops_per_jiffy, skip runtime calibration
            // nokaslr norandmaps: disable address randomization
            // nosmp noapic: single CPU, no APIC probing
            // kfence.sample_interval=0: disable kfence (timing-dependent)
            // no_hash_pointers: make pointer output deterministic
            // virtio_mmio.device=<size>@<baseaddr>:<irq>: notify kernel of virtio devices
            cmdline: b"console=ttyS0 earlyprintk=serial \
                       clocksource=tsc tsc=reliable \
                       lpj=6000000 \
                       nokaslr noapic nosmp \
                       randomize_kstack_offset=off norandmaps \
                       kfence.sample_interval=0 \
                       no_hash_pointers \
                       virtio_mmio.device=4K@0xd0000000:5 \
                       virtio_mmio.device=4K@0xd0001000:6 \
                       virtio_mmio.device=4K@0xd0002000:7 \
                       panic=-1\0"
                .to_vec(),
            disk_image_path: None,
            extra_cmdline: None,
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

/// A writer that outputs to stdout AND captures bytes in a shared buffer.
///
/// Used as the output sink for the serial port so that serial output is
/// both visible in real time and available for programmatic inspection
/// via [`DeterministicVm::take_serial_output`] and
/// [`DeterministicVm::run_until`].
#[derive(Clone)]
pub struct CapturingWriter {
    buffer: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

impl CapturingWriter {
    fn new() -> Self {
        Self {
            buffer: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Take the captured output, clearing the internal buffer.
    pub fn take(&self) -> Vec<u8> {
        let mut buf = self.buffer.lock().unwrap();
        std::mem::take(&mut *buf)
    }

    /// Get the captured output as a string (lossy UTF-8).
    pub fn as_string(&self) -> String {
        let buf = self.buffer.lock().unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    }
}

impl io::Write for CapturingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::stdout().write_all(buf)?;
        io::stdout().flush()?;
        self.buffer.lock().unwrap().extend_from_slice(buf);
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

    /// Set after a signal-interrupted exit (EINTR/Intr). Causes the
    /// next `step()` to skip `sync_tsc_to_guest()` so the TSC resync
    /// doesn't happen at non-deterministic wall-clock times.
    skip_tsc_sync: bool,

    /// Extra kernel command line parameters (from VmConfig).
    extra_cmdline: Option<String>,
}

impl DeterministicVm {
    /// Install a no-op SIGALRM handler so the preemption timer doesn't
    /// kill the process. Called once on first multi-vCPU VM creation.
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

        // Register guest memory with KVM
        let mem_region = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0,
            memory_size: config.memory_size as u64,
            userspace_addr: memory.host_address(),
            flags: 0,
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
        let virtio_devices =
            Self::create_virtio_devices(config.cpu.seed, config.disk_image_path.as_deref())?;

        info!(
            "VM created: {} MB memory, {} vCPU(s), TSC {} kHz, seed {}, {} virtio devices",
            config.memory_size / (1024 * 1024),
            num_vcpus,
            config.cpu.tsc_khz,
            config.cpu.seed,
            virtio_devices.len(),
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
            coverage_active: false,
            skip_tsc_sync: false,
            extra_cmdline: config.extra_cmdline.clone(),
        })
    }

    /// Create the virtio MMIO devices (block, net, entropy).
    ///
    /// If `disk_image_path` is `Some`, the block device is initialized
    /// from that file (read once, then copy-on-write for snapshots).
    /// Otherwise a zero-filled 16 MB disk is created.
    fn create_virtio_devices(
        seed: u64,
        disk_image_path: Option<&str>,
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

        // Device 1: virtio-net
        let mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]; // QEMU-style MAC
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
            "  Device 1: virtio-net  @ {:#010x} IRQ {}",
            VIRTIO_MMIO_BASE_1, VIRTIO_MMIO_IRQ_1
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
    fn build_cmdline(&self) -> Vec<u8> {
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

        let extra = self
            .extra_cmdline
            .as_deref()
            .unwrap_or("");

        let cmdline = format!(
            "console=ttyS0 earlyprintk=serial \
             {clock_params} \
             lpj=6000000 \
             nokaslr {smp_params} \
             randomize_kstack_offset=off norandmaps \
             kfence.sample_interval=0 \
             no_hash_pointers \
             virtio_mmio.device=4K@0xd0000000:5 \
             virtio_mmio.device=4K@0xd0001000:6 \
             virtio_mmio.device=4K@0xd0002000:7 \
             {extra} \
             panic=-1\0"
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

        // Write kernel command line (dynamic based on num_vcpus)
        let cmdline = self.build_cmdline();
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

        for i in 0..max_exits {
            if self.step()? {
                // Disarm timer on early exit to prevent stale SIGALRMs
                // from leaking into subsequent VM runs in the same process.
                if self.vcpus.len() > 1 {
                    self.disarm_preemption_timer();
                }
                return Ok((i + 1, true));
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
                if self.vcpus.len() > 1 {
                    self.disarm_preemption_timer();
                }
                return Ok((i + 1, true));
            }
        }
        // Disarm preemption timer at end of bounded run so it doesn't
        // fire on a future vcpu.run() call from a different VM.
        if self.vcpus.len() > 1 {
            self.disarm_preemption_timer();
        }
        Ok((max_exits, false))
    }

    // ─── Public API: serial output ───────────────────────────────────

    /// Take all serial output captured since the last call.
    pub fn take_serial_output(&mut self) -> String {
        String::from_utf8_lossy(&self.serial_writer.take()).into_owned()
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
    /// [`CoverageBitmap::from_slice`](chaoscontrol_explore::coverage::CoverageBitmap::from_slice).
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

    // ─── Public API: snapshot / restore ──────────────────────────────

    /// Take a snapshot of the current VM state.
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

        crate::snapshot::VmSnapshot::capture(&self.vcpus, &self.vm, self.memory.inner(), params)
            .map_err(|e| {
                SnapshotSnafu {
                    message: e.to_string(),
                }
                .build()
            })
    }

    /// Restore VM state from a snapshot.
    pub fn restore(&mut self, snapshot: &crate::snapshot::VmSnapshot) -> Result<(), VmError> {
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

        // Restore DeterministicPit state
        self.pit = DeterministicPit::restore(&snapshot.pit_snapshot);
        self.last_kvm_pit_mode = snapshot.last_kvm_pit_mode;

        // Restore fault engine state
        self.fault_engine.restore(&snapshot.fault_engine_snapshot);

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
        }
        let run_result = self.vcpus[self.active_vcpu].run();

        match run_result {
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
                } else {
                    for byte in data.iter_mut() {
                        *byte = 0xff;
                    }
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

                let tsc = self.virtual_tsc.read();
                if port == SDK_PORT {
                    self.handle_sdk_hypercall();
                } else if port == COVERAGE_PORT {
                    self.coverage_active = true;
                    log::info!("Coverage instrumentation activated by guest");
                } else if (SERIAL_PORT_BASE..=SERIAL_PORT_END).contains(&port) {
                    let offset = (port - SERIAL_PORT_BASE) as u8;
                    let byte = data[0];
                    let _ = self.serial.write(offset, byte);
                } else if DeterministicPit::handles_port(port) {
                    self.pit.write_port(port, data[0], tsc);
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

                    self.maybe_switch_vcpu();
                    Ok(false)
                } else {
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
                info!("VM shutdown (exit_count={})", self.exit_count);
                Ok(true)
            }
            Ok(VcpuExit::MmioRead(addr, data)) => {
                self.exit_count += 1;
                self.exits_since_last_sdk += 1;
                self.virtual_tsc.tick();

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
                    for byte in data {
                        *byte = 0;
                    }
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
                        let offset = addr - dev.base_addr();
                        dev.write(offset, data, self.memory.inner());

                        // Process queues and raise interrupt if needed
                        if dev.process_queues(self.memory.inner()) {
                            let irq = dev.irq();
                            let _ = self.vm.set_irq_line(irq, true);
                            let _ = self.vm.set_irq_line(irq, false);
                        }
                        break;
                    }
                }

                self.maybe_switch_vcpu();
                Ok(false)
            }
            Ok(VcpuExit::Intr) => {
                // SIGALRM interrupted vcpu.run().
                //
                // Only switch vCPU if this vCPU appears stuck (consecutive
                // SIGALRMs without any real exit). Normal code generates
                // real exits between SIGALRMs, keeping the counter at 0.
                // Spin-wait loops have no real exits, so the counter
                // grows until it crosses the threshold → liveness switch.
                //
                // The liveness switch is INVISIBLE to the scheduler:
                // we change active_vcpu but not scheduler state.
                self.disarm_preemption_timer();
                // Skip PIT/TSC sync on next step() — virtual time hasn't
                // advanced, and skipping avoids disturbing in-progress
                // PIT channel 2 calibration during early boot.
                self.skip_tsc_sync = true;
                if num_vcpus > 1 {
                    self.sigalrm_without_exit += 1;
                    // Threshold: 2 consecutive SIGALRMs with no real exit.
                    // At 500µs per SIGALRM, this triggers after ~1ms of
                    // no real exits — clearly a spin-wait loop.
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
                }
                Ok(false)
            }
            Ok(VcpuExit::Debug(_debug)) => {
                // Debug exit (reserved for future single-step support).
                Ok(false)
            }
            Ok(VcpuExit::IrqWindowOpen) => {
                // KVM needs to inject a pending interrupt — retry immediately.
                Ok(false)
            }
            Ok(VcpuExit::InternalError) => {
                // KVM_EXIT_INTERNAL_ERROR: emulation failure or inconsistent
                // vCPU state. In SMP mode this can happen after snapshot/restore
                // if the active vCPU (AP) was HALTED and the in-kernel LAPIC
                // state wasn't perfectly consistent for re-entry.
                //
                // Recovery: switch to another runnable vCPU (fall back to BSP).
                // If this is a single-vCPU VM, treat it as a fatal halt.
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
                    if self.vcpus.len() > 1 {
                        self.sigalrm_without_exit += 1;
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
                    }
                    return Ok(false);
                }
                Err(VmError::VcpuRun { source: e })
            }
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

    // ─── Internal: SDK hypercall handler ─────────────────────────

    /// Handle an SDK hypercall triggered by `outb(0x510, 0)`.
    ///
    /// Reads the [`HypercallPage`] from guest memory at
    /// [`HYPERCALL_PAGE_ADDR`], dispatches to the fault engine,
    /// and writes the result back.
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
        if self.vcpus.len() > 1 {
            self.disarm_preemption_timer();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_config_default() {
        let config = VmConfig::default();
        assert_eq!(config.memory_size, 256 * 1024 * 1024);
        assert_eq!(config.num_vcpus, 1);
        assert_eq!(config.cpu.tsc_khz, 3_000_000);
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
}
