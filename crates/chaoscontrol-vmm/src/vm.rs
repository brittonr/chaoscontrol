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

use crate::cpu::{self, CpuConfig, VirtualTsc};
use crate::devices::entropy::DeterministicEntropy;
use crate::devices::pit::DeterministicPit;

use chaoscontrol_fault::engine::{FaultEngine, EngineConfig};
use chaoscontrol_protocol::{HypercallPage, HYPERCALL_PAGE_ADDR, HYPERCALL_PAGE_SIZE, SDK_PORT};
use crate::memory::{
    self, build_e820_map, code64_segment, data_segment, tss_segment, GuestMemoryManager,
    BOOT_GDT_OFFSET, BOOT_IDT_OFFSET, BOOT_STACK_POINTER, CMDLINE_START, GDT_ENTRY_COUNT,
    HIMEM_START, PML4_START, ZERO_PAGE_START,
};

use kvm_bindings::{
    kvm_clock_data, kvm_fpu, kvm_pit_config, kvm_regs, kvm_userspace_memory_region,
    KVM_PIT_SPEAKER_DUMMY,
};
use kvm_ioctls::{Kvm, VcpuExit, VcpuFd, VmFd};
use linux_loader::configurator::linux::LinuxBootConfigurator;
use linux_loader::configurator::{BootConfigurator, BootParams};
use linux_loader::loader::bootparam::boot_params;
use linux_loader::loader::elf::Elf;
use linux_loader::loader::KernelLoader;
use log::info;
use std::fs::File;
use std::io;
use thiserror::Error;
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



/// KVM TSS address — must be set before create_irq_chip.
/// Placed at the top of the 32-bit address space (3 pages needed by KVM).
const KVM_TSS_ADDRESS: usize = 0xfffb_d000;

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
    /// Kernel command line (NUL-terminated).
    pub cmdline: Vec<u8>,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            memory_size: 256 * 1024 * 1024,
            cpu: CpuConfig {
                // Hide KVM so guest doesn't use kvm-clock (reads host wall time).
                // Set fixed family=6 (Intel) so kernel's native_calibrate_tsc()
                // trusts CPUID leaf 0x15 for exact TSC frequency instead of
                // doing non-deterministic PIT-based calibration.
                hide_hypervisor: true,
                fixed_family: Some(6),
                fixed_model: Some(85),    // Skylake-SP
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
            cmdline: b"console=ttyS0 earlyprintk=serial \
                       clocksource=tsc tsc=reliable \
                       lpj=6000000 \
                       nokaslr noapic nosmp \
                       randomize_kstack_offset=off norandmaps \
                       kfence.sample_interval=0 \
                       no_hash_pointers \
                       panic=-1\0"
                .to_vec(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Error type
// ═══════════════════════════════════════════════════════════════════════

#[derive(Error, Debug)]
pub enum VmError {
    #[error("Failed to create KVM instance: {0}")]
    KvmCreate(#[source] kvm_ioctls::Error),

    #[error("Failed to create VM: {0}")]
    VmCreate(#[source] kvm_ioctls::Error),

    #[error("Failed to create vCPU: {0}")]
    VcpuCreate(#[source] kvm_ioctls::Error),

    #[error("Failed to set user memory region: {0}")]
    SetUserMemoryRegion(#[source] kvm_ioctls::Error),

    #[error("Guest memory error: {0}")]
    Memory(#[from] memory::MemoryError),

    #[error("CPU configuration error: {0}")]
    Cpu(#[from] cpu::CpuError),

    #[error("Failed to load kernel: {0}")]
    KernelLoad(#[source] linux_loader::loader::Error),

    #[error("Failed to write to guest memory")]
    GuestMemoryWrite,

    #[error("Failed to set vCPU registers: {0}")]
    SetRegisters(#[source] kvm_ioctls::Error),

    #[error("Failed to set vCPU special registers: {0}")]
    SetSregs(#[source] kvm_ioctls::Error),

    #[error("Failed to get vCPU special registers: {0}")]
    GetSregs(#[source] kvm_ioctls::Error),

    #[error("Failed to set FPU: {0}")]
    SetFpu(#[source] kvm_ioctls::Error),

    #[error("Failed to create in-kernel IRQ chip: {0}")]
    CreateIrqChip(#[source] kvm_ioctls::Error),

    #[error("Failed to configure PIT: {0}")]
    CreatePit(#[source] kvm_ioctls::Error),

    #[error("Failed to set KVM clock: {0}")]
    SetClock(#[source] kvm_ioctls::Error),

    #[error("Failed to run vCPU: {0}")]
    VcpuRun(#[source] kvm_ioctls::Error),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Snapshot error: {0}")]
    Snapshot(String),
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
    vcpu: VcpuFd,
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

    // Execution statistics
    exit_count: u64,
    io_exit_count: u64,
}

impl DeterministicVm {
    /// Create a new deterministic VM with the given configuration.
    ///
    /// This sets up KVM, guest memory, IRQ chip, PIT, and the serial
    /// console. The VM is ready for [`load_kernel`](Self::load_kernel)
    /// after construction.
    pub fn new(config: VmConfig) -> Result<Self, VmError> {
        let kvm = Kvm::new().map_err(VmError::KvmCreate)?;
        let vm = kvm.create_vm().map_err(VmError::VmCreate)?;

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
                .map_err(VmError::SetUserMemoryRegion)?;
        }

        // Set TSS address — MUST be before create_irq_chip on x86_64
        vm.set_tss_address(KVM_TSS_ADDRESS)
            .map_err(VmError::CreateIrqChip)?;

        // Create in-kernel IRQ chip (PIC, IOAPIC, LAPIC) — MUST be before create_vcpu
        vm.create_irq_chip().map_err(VmError::CreateIrqChip)?;

        // Create KVM PIT with speaker dummy flag.
        // KVM's PIT handles I/O ports 0x40-0x43, 0x61 internally and
        // delivers IRQ 0 via the in-kernel PIC. We use set_pit2() to
        // reset its count_load_time before each vcpu.run(), pinning
        // timer delivery to our virtual TSC instead of host wall time.
        let pit_config = kvm_pit_config {
            flags: KVM_PIT_SPEAKER_DUMMY,
            ..Default::default()
        };
        vm.create_pit2(pit_config)
            .map_err(VmError::CreatePit)?;

        // Immediately disable KVM PIT channel 0 timer so it never fires
        // on host time. We'll deliver IRQ 0 ourselves via set_irq_line
        // at deterministic virtual-time points.
        {
            let mut pit_state = vm.get_pit2().map_err(VmError::CreatePit)?;
            // Set channel 0 to mode 0 (one-shot) with max count and
            // a far-future load time so it never triggers
            pit_state.channels[0].count = 0; // 0 = 65536
            pit_state.channels[0].mode = 0; // mode 0 = one-shot
            pit_state.channels[0].gate = 1;
            // Set count_load_time far in the future (year 2100)
            pit_state.channels[0].count_load_time = i64::MAX / 2;
            vm.set_pit2(&pit_state).map_err(VmError::CreatePit)?;
        }

        // DETERMINISM: Set KVM clock to zero so guest always sees the same
        // starting time. Without this, the guest reads host wall-clock time
        // via the KVM paravirt clock MSRs, breaking reproducibility.
        let clock_data = kvm_clock_data {
            clock: 0,
            ..Default::default()
        };
        vm.set_clock(&clock_data).map_err(VmError::SetClock)?;
        info!("KVM clock set to 0 (deterministic)");

        // Create vCPU AFTER irqchip (so LAPIC is in-kernel)
        let vcpu = vm.create_vcpu(0).map_err(VmError::VcpuCreate)?;

        // Apply deterministic CPUID filtering
        let cpuid = cpu::filter_cpuid(&kvm, &config.cpu)?;
        vcpu.set_cpuid2(&cpuid).map_err(cpu::CpuError::SetCpuid)?;

        // Pin TSC to fixed frequency
        cpu::setup_tsc(&vcpu, config.cpu.tsc_khz)?;

        // Create virtual TSC for deterministic time tracking
        let virtual_tsc = VirtualTsc::from_config(&config.cpu);

        // Create deterministic entropy source seeded from master seed
        let entropy = DeterministicEntropy::new(config.cpu.seed);

        // Deterministic PIT driven by virtual TSC — delivers timer
        // interrupts at exact virtual-time points via set_irq_line.
        let pit = DeterministicPit::new(config.cpu.tsc_khz);

        // Set up serial port with interrupt support
        let serial_evt = EventFd::new(libc::EFD_NONBLOCK).map_err(VmError::Io)?;
        let serial_trigger = SerialTrigger(serial_evt.try_clone().map_err(VmError::Io)?);
        let serial_writer = CapturingWriter::new();
        let serial = vm_superio::Serial::new(serial_trigger, serial_writer.clone());

        // Register the serial EventFd with KVM IRQ line 4 (COM1)
        vm.register_irqfd(&serial_evt, SERIAL_IRQ)
            .map_err(VmError::CreateIrqChip)?;

        // Create fault injection engine for SDK hypercalls
        let fault_engine = FaultEngine::new(EngineConfig {
            seed: config.cpu.seed,
            num_vms: 1,
            ..EngineConfig::default()
        });

        info!(
            "VM created: {} MB memory, TSC {} kHz, seed {}",
            config.memory_size / (1024 * 1024),
            config.cpu.tsc_khz,
            config.cpu.seed,
        );

        Ok(Self {
            kvm,
            vm,
            vcpu,
            memory,
            virtual_tsc,
            entropy,
            pit,
            serial,
            serial_writer,
            fault_engine,
            last_kvm_pit_mode: 0xFF, // impossible value forces first sync
            exit_count: 0,
            io_exit_count: 0,
        })
    }

    /// Load a Linux kernel (and optional initrd) into guest memory.
    ///
    /// This sets up:
    /// - Kernel loaded at HIMEM_START (1 MB)
    /// - Optional initrd placed after the kernel (page-aligned)
    /// - Boot parameters (zero page) with E820 memory map
    /// - GDT, page tables, segment registers for 64-bit mode
    /// - General-purpose registers with entry point and stack pointer
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
        .map_err(VmError::KernelLoad)?;

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
                .map_err(|_| VmError::GuestMemoryWrite)?;
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

        // Set up x86_64 registers
        self.setup_sregs()?;
        self.setup_regs(entry_point)?;
        self.setup_fpu()?;

        Ok(())
    }

    /// Reset the vCPU's TSC to 0 via MSR write.
    fn reset_tsc_to_zero(&self) -> Result<(), VmError> {
        use kvm_bindings::{kvm_msr_entry, Msrs};

        const MSR_IA32_TSC: u32 = 0x10;

        let msrs = Msrs::from_entries(&[kvm_msr_entry {
            index: MSR_IA32_TSC,
            data: 0,
            ..Default::default()
        }])
        .map_err(|_| VmError::GuestMemoryWrite)?;

        self.vcpu.set_msrs(&msrs).map_err(VmError::SetRegisters)?;
        Ok(())
    }

    fn setup_boot_params(&self, initrd_info: Option<(u64, u64)>) -> Result<(), VmError> {
        const KERNEL_BOOT_FLAG_MAGIC: u16 = 0xaa55;
        const KERNEL_HDR_MAGIC: u32 = 0x5372_6448;
        const KERNEL_LOADER_OTHER: u8 = 0xff;
        const KERNEL_MIN_ALIGNMENT_BYTES: u32 = 0x0100_0000;

        // Write kernel command line
        let cmdline = b"console=ttyS0 earlyprintk=serial \
                       clocksource=tsc tsc=reliable \
                       lpj=6000000 \
                       nokaslr noapic nosmp \
                       randomize_kstack_offset=off norandmaps \
                       kfence.sample_interval=0 \
                       no_hash_pointers \
                       panic=-1\0";
        self.memory.write_cmdline(cmdline)?;

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
            .map_err(|_| VmError::GuestMemoryWrite)?;

        Ok(())
    }

    fn setup_sregs(&self) -> Result<(), VmError> {
        let mut sregs = self.vcpu.get_sregs().map_err(VmError::GetSregs)?;

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

        self.vcpu.set_sregs(&sregs).map_err(VmError::SetSregs)?;
        Ok(())
    }

    fn setup_regs(&self, entry_point: GuestAddress) -> Result<(), VmError> {
        let regs = kvm_regs {
            rip: entry_point.raw_value(),
            rsp: BOOT_STACK_POINTER,
            rbp: BOOT_STACK_POINTER,
            rsi: ZERO_PAGE_START, // Pointer to boot params
            rflags: 0x2,          // Reserved bit must be set
            ..Default::default()
        };
        self.vcpu.set_regs(&regs).map_err(VmError::SetRegisters)?;
        Ok(())
    }

    fn setup_fpu(&self) -> Result<(), VmError> {
        let fpu = kvm_fpu {
            fcw: 0x37f,
            mxcsr: 0x1f80,
            ..Default::default()
        };
        self.vcpu.set_fpu(&fpu).map_err(VmError::SetFpu)?;
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
        self.vm.set_clock(&clock_data).map_err(VmError::SetClock)?;

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
        for i in 0..max_exits {
            if self.step()? {
                return Ok((i + 1, true));
            }
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

    /// Get a reference to the guest memory manager.
    pub fn memory(&self) -> &GuestMemoryManager {
        &self.memory
    }

    // ─── Public API: snapshot / restore ──────────────────────────────

    /// Take a snapshot of the current VM state.
    pub fn snapshot(&self) -> Result<crate::snapshot::VmSnapshot, VmError> {
        crate::snapshot::VmSnapshot::capture(
            &self.vcpu,
            &self.vm,
            self.memory.inner(),
            self.serial.state(),
            self.entropy.snapshot(),
        )
        .map_err(|e| VmError::Snapshot(e.to_string()))
    }

    /// Restore VM state from a snapshot.
    pub fn restore(&mut self, snapshot: &crate::snapshot::VmSnapshot) -> Result<(), VmError> {
        snapshot
            .restore(&self.vcpu, &self.vm, self.memory.inner())
            .map_err(|e| VmError::Snapshot(e.to_string()))?;

        // Restore deterministic entropy PRNG state
        self.entropy = DeterministicEntropy::restore(&snapshot.entropy);

        // Restore serial state with new EventFd and our capturing writer
        let serial_evt = EventFd::new(libc::EFD_NONBLOCK).map_err(VmError::Io)?;
        let serial_trigger = SerialTrigger(serial_evt.try_clone().map_err(VmError::Io)?);
        self.serial_writer = CapturingWriter::new();
        self.serial = vm_superio::Serial::from_state(
            &snapshot.serial_state,
            serial_trigger,
            vm_superio::serial::NoEvents,
            self.serial_writer.clone(),
        )
        .map_err(|e| VmError::Snapshot(format!("serial restore: {e}")))?;

        // Re-register IRQ fd
        self.vm
            .register_irqfd(&serial_evt, SERIAL_IRQ)
            .map_err(VmError::CreateIrqChip)?;

        info!("VM restored from snapshot (RIP={:#x})", snapshot.regs.rip,);

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
        let mut pit_state = self.vm.get_pit2().map_err(VmError::CreatePit)?;
        let ch0 = &pit_state.channels[0];

        // Mirror KVM PIT channel 0 config to our DeterministicPit
        // when it gets reprogrammed by the guest.
        let reload = ch0.count as u16;
        let mode = ch0.mode;
        if ch0.gate != 0
            && (reload != self.pit.channel_reload(0)
                || mode != self.last_kvm_pit_mode)
        {
            let tsc = self.virtual_tsc.read();
            // Program our DeterministicPit with the same config
            // Encode command byte: ch0, lohi, mode, binary
            let cmd = 0x30 | ((mode & 0x7) << 1);
            self.pit.write_port(0x43, cmd, tsc);
            self.pit.write_port(0x40, reload as u8, tsc);
            self.pit.write_port(0x40, (reload >> 8) as u8, tsc);
            self.last_kvm_pit_mode = mode;
        }

        // Suppress KVM PIT timer: push count_load_time far into future
        // so KVM never thinks the counter expired.
        pit_state.channels[0].count_load_time = i64::MAX / 2;
        self.vm.set_pit2(&pit_state).map_err(VmError::CreatePit)?;

        // Now deliver our deterministic IRQ 0 if it's time
        let current_tsc = self.virtual_tsc.read();
        if self.pit.pending_irq(current_tsc) {
            self.vm
                .set_irq_line(PIT_IRQ, true)
                .map_err(VmError::CreateIrqChip)?;
            self.vm
                .set_irq_line(PIT_IRQ, false)
                .map_err(VmError::CreateIrqChip)?;
            self.pit.acknowledge_irq();
        }
        Ok(())
    }

    fn step(&mut self) -> Result<bool, VmError> {
        self.sync_and_suppress_pit()?;

        match self.vcpu.run() {
            Ok(VcpuExit::IoIn(port, data)) => {
                self.exit_count += 1;
                self.io_exit_count += 1;
                self.virtual_tsc.tick();



                let tsc = self.virtual_tsc.read();
                if port == SDK_PORT {
                    // SDK hypercall result — guest reads status byte
                    data[0] = 0; // STATUS_OK
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
                Ok(false)
            }
            Ok(VcpuExit::IoOut(port, data)) => {
                self.exit_count += 1;
                self.io_exit_count += 1;
                self.virtual_tsc.tick();

                let tsc = self.virtual_tsc.read();
                if port == SDK_PORT {
                    self.handle_sdk_hypercall();
                } else if (SERIAL_PORT_BASE..=SERIAL_PORT_END).contains(&port) {
                    let offset = (port - SERIAL_PORT_BASE) as u8;
                    let byte = data[0];
                    let _ = self.serial.write(offset, byte);
                } else if DeterministicPit::handles_port(port) {
                    self.pit.write_port(port, data[0], tsc);
                }
                Ok(false)
            }
            Ok(VcpuExit::Hlt) => {
                self.exit_count += 1;
                self.virtual_tsc.tick();

                // HLT = kernel idle loop waiting for next interrupt.
                // Read KVM PIT state to find channel 0's reload value,
                // then fast-forward virtual TSC by one PIT period and
                // inject the interrupt deterministically.
                let pit_state = self.vm.get_pit2().map_err(VmError::CreatePit)?;
                let ch0 = &pit_state.channels[0];
                let reload = if ch0.count == 0 { 65536u64 } else { ch0.count as u64 };

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
                        .map_err(VmError::CreateIrqChip)?;
                    self.vm
                        .set_irq_line(PIT_IRQ, false)
                        .map_err(VmError::CreateIrqChip)?;

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
            Ok(VcpuExit::MmioRead(_, data)) => {
                self.exit_count += 1;
                self.virtual_tsc.tick();
                for byte in data {
                    *byte = 0;
                }
                Ok(false)
            }
            Ok(VcpuExit::MmioWrite(_, _)) => {
                self.exit_count += 1;
                self.virtual_tsc.tick();
                Ok(false)
            }
            Ok(exit) => {
                self.exit_count += 1;
                info!("Unhandled VM exit: {:?} — stopping", exit);
                Ok(true)
            }
            Err(e) => Err(VmError::VcpuRun(e)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_config_default() {
        let config = VmConfig::default();
        assert_eq!(config.memory_size, 256 * 1024 * 1024);
        assert_eq!(config.cpu.tsc_khz, 3_000_000);
    }
}
