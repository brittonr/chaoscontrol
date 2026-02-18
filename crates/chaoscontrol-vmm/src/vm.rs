use kvm_bindings::{
    kvm_fpu, kvm_pit_config, kvm_regs, kvm_segment, kvm_userspace_memory_region,
    KVM_MAX_CPUID_ENTRIES, KVM_PIT_SPEAKER_DUMMY,
};
use kvm_ioctls::{Kvm, VcpuExit, VcpuFd, VmFd};
use linux_loader::configurator::linux::LinuxBootConfigurator;
use linux_loader::configurator::{BootConfigurator, BootParams};
use linux_loader::loader::bootparam::boot_params;
use linux_loader::loader::elf::Elf;
use linux_loader::loader::KernelLoader;
use log::{debug, info};
use std::fs::File;
use std::io::{self, Write};
use thiserror::Error;
use vm_memory::{Address, Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};
use vmm_sys_util::eventfd::EventFd;

// Memory layout constants (based on Firecracker)
const HIMEM_START: u64 = 0x0010_0000; // 1 MB
const BOOT_STACK_POINTER: u64 = 0x8ff0;
const ZERO_PAGE_START: u64 = 0x7000;
const CMDLINE_START: u64 = 0x20000;
const BOOT_GDT_OFFSET: u64 = 0x500;
const BOOT_IDT_OFFSET: u64 = 0x520;

// Page table addresses
const PML4_START: u64 = 0x9000;
const PDPTE_START: u64 = 0xa000;
const PDE_START: u64 = 0xb000;

// x86_64 control register flags
const X86_CR0_PE: u64 = 0x1; // Protected mode
const X86_CR0_PG: u64 = 0x8000_0000; // Paging
const X86_CR4_PAE: u64 = 0x20; // Physical Address Extension
const EFER_LME: u64 = 0x100; // Long Mode Enable
const EFER_LMA: u64 = 0x400; // Long Mode Active

// Serial port constants
const SERIAL_PORT_BASE: u16 = 0x3f8;
const SERIAL_PORT_END: u16 = 0x3ff;

// Serial port register offsets
const SERIAL_DATA: u16 = 0; // THR/RBR
const SERIAL_IER: u16 = 1; // Interrupt Enable Register
const SERIAL_IIR_FCR: u16 = 2; // IIR (read) / FCR (write)
const SERIAL_LCR: u16 = 3; // Line Control Register
const SERIAL_MCR: u16 = 4; // Modem Control Register
const SERIAL_LSR: u16 = 5; // Line Status Register
const SERIAL_MSR: u16 = 6; // Modem Status Register

// CPUID leaves to filter
const CPUID_EXT_FEATURES: u32 = 1;
const CPUID_STRUCTURED_EXT: u32 = 7;

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
    
    #[error("Failed to create guest memory")]
    GuestMemoryCreate,
    
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
    
    #[error("Failed to set CPUID: {0}")]
    SetCpuid(#[source] kvm_ioctls::Error),
    
    #[error("Failed to get CPUID: {0}")]
    GetCpuid(#[source] kvm_ioctls::Error),
    
    #[error("Failed to create in-kernel IRQ chip: {0}")]
    CreateIrqChip(#[source] kvm_ioctls::Error),
    
    #[error("Failed to create PIT: {0}")]
    CreatePit(#[source] kvm_ioctls::Error),
    
    #[error("Failed to run vCPU: {0}")]
    VcpuRun(#[source] kvm_ioctls::Error),
    
    #[error("Failed to set TSC frequency: {0}")]
    SetTscKhz(#[source] kvm_ioctls::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

/// COM1 IRQ line number (standard PC)
const SERIAL_IRQ: u32 = 4;

/// Wrapper to implement vm_superio::Trigger for EventFd
struct SerialTrigger(EventFd);

impl vm_superio::Trigger for SerialTrigger {
    type E = io::Error;

    fn trigger(&self) -> Result<(), Self::E> {
        self.0.write(1).map_err(|e| io::Error::other(e))
    }
}

pub struct DeterministicVm {
    kvm: Kvm,
    vm: VmFd,
    vcpu: VcpuFd,
    guest_memory: GuestMemoryMmap,
    serial: vm_superio::Serial<SerialTrigger, vm_superio::serial::NoEvents, io::Stdout>,
}

impl DeterministicVm {
    pub fn new(memory_size: usize) -> Result<Self, VmError> {
        let kvm = Kvm::new().map_err(VmError::KvmCreate)?;
        let vm = kvm.create_vm().map_err(VmError::VmCreate)?;

        // Create guest memory first (TSS address must be within addressable range)
        let guest_memory = create_guest_memory(memory_size)?;

        // Set up KVM memory region
        let mem_region = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0,
            memory_size: memory_size as u64,
            userspace_addr: guest_memory.get_host_address(GuestAddress(0)).unwrap() as u64,
            flags: 0,
        };
        
        unsafe {
            vm.set_user_memory_region(mem_region)
                .map_err(VmError::SetUserMemoryRegion)?;
        }

        // Set TSS address — MUST be done before create_irq_chip on x86_64
        // Place it at top of 32-bit address space (3 pages needed by KVM)
        const KVM_TSS_ADDRESS: usize = 0xfffb_d000;
        vm.set_tss_address(KVM_TSS_ADDRESS)
            .map_err(VmError::CreateIrqChip)?;

        // Create in-kernel IRQ chip (PIC, IOAPIC, LAPIC) — MUST be before create_vcpu
        vm.create_irq_chip().map_err(VmError::CreateIrqChip)?;

        // Create PIT (Programmable Interval Timer)
        let pit_config = kvm_pit_config {
            flags: KVM_PIT_SPEAKER_DUMMY,
            ..Default::default()
        };
        vm.create_pit2(pit_config).map_err(VmError::CreatePit)?;

        // Create vCPU AFTER irqchip (so LAPIC is in-kernel)
        let vcpu = vm.create_vcpu(0).map_err(VmError::VcpuCreate)?;

        // Set up serial port with interrupt support
        let serial_evt = EventFd::new(libc::EFD_NONBLOCK).map_err(VmError::Io)?;
        let serial_trigger = SerialTrigger(serial_evt.try_clone().map_err(VmError::Io)?);
        let serial = vm_superio::Serial::new(serial_trigger, io::stdout());

        // Register the serial EventFd with KVM IRQ line 4 (COM1)
        vm.register_irqfd(&serial_evt, SERIAL_IRQ)
            .map_err(VmError::CreateIrqChip)?;

        Ok(Self {
            kvm,
            vm,
            vcpu,
            guest_memory,
            serial,
        })
    }

    pub fn load_kernel(
        &mut self,
        kernel_path: &str,
        initrd_path: Option<&str>,
    ) -> Result<(), VmError> {
        info!("Loading kernel from {}", kernel_path);
        
        let mut kernel_file = File::open(kernel_path)?;
        
        // Load kernel using linux-loader
        let kernel_load_result = Elf::load(
            &self.guest_memory,
            None,
            &mut kernel_file,
            Some(GuestAddress(HIMEM_START)),
        )
        .map_err(VmError::KernelLoad)?;

        let entry_point = kernel_load_result.kernel_load;
        let kernel_end = kernel_load_result.kernel_end;
        info!(
            "Kernel entry point: 0x{:x}, end: 0x{:x}",
            entry_point.raw_value(),
            kernel_end
        );

        // Load initrd if provided (place it after the kernel, page-aligned)
        let initrd_info = if let Some(initrd_path) = initrd_path {
            info!("Loading initrd from {}", initrd_path);
            let initrd_data = std::fs::read(initrd_path)?;
            let initrd_addr = (kernel_end + 4095) & !4095; // Page-align
            self.guest_memory
                .write_slice(&initrd_data, GuestAddress(initrd_addr))
                .map_err(|_| VmError::GuestMemoryWrite)?;
            info!(
                "Initrd loaded at 0x{:x}, size: {} bytes",
                initrd_addr,
                initrd_data.len()
            );
            Some((initrd_addr, initrd_data.len() as u64))
        } else {
            None
        };

        // Set up boot parameters (zero page)
        self.setup_boot_params(initrd_info)?;

        // Set up x86_64 boot environment
        self.setup_x86_64_boot(entry_point)?;

        // Filter CPUID for determinism
        self.filter_cpuid()?;

        // Pin TSC
        self.pin_tsc()?;

        Ok(())
    }

    fn setup_boot_params(
        &self,
        initrd_info: Option<(u64, u64)>,
    ) -> Result<(), VmError> {
        const KERNEL_BOOT_FLAG_MAGIC: u16 = 0xaa55;
        const KERNEL_HDR_MAGIC: u32 = 0x5372_6448;
        const KERNEL_LOADER_OTHER: u8 = 0xff;
        const KERNEL_MIN_ALIGNMENT_BYTES: u32 = 0x0100_0000;

        // Write kernel command line to guest memory
        let cmdline = b"console=ttyS0 earlyprintk=serial nokaslr noapic nosmp \
            randomize_kstack_offset=off norandmaps panic=-1\0";
        self.guest_memory
            .write_slice(cmdline, GuestAddress(CMDLINE_START))
            .map_err(|_| VmError::GuestMemoryWrite)?;

        let mut hdr = linux_loader::loader::bootparam::setup_header {
            type_of_loader: KERNEL_LOADER_OTHER,
            boot_flag: KERNEL_BOOT_FLAG_MAGIC,
            header: KERNEL_HDR_MAGIC,
            cmd_line_ptr: CMDLINE_START as u32,
            cmdline_size: cmdline.len() as u32,
            kernel_alignment: KERNEL_MIN_ALIGNMENT_BYTES,
            ..Default::default()
        };

        // Set initrd address/size in boot header
        if let Some((initrd_addr, initrd_size)) = initrd_info {
            hdr.ramdisk_image = initrd_addr as u32;
            hdr.ramdisk_size = initrd_size as u32;
        }

        let mut params = boot_params {
            hdr,
            ..Default::default()
        };

        // Set up E820 memory map
        // Entry 0: Low memory (0 - 9FC00, conventional memory below EBDA)
        params.e820_table[0].addr = 0;
        params.e820_table[0].size = 0x9fc00;
        params.e820_table[0].type_ = 1; // E820_RAM

        // Entry 1: High memory (1MB - end)
        params.e820_table[1].addr = HIMEM_START;
        params.e820_table[1].size =
            self.guest_memory.last_addr().raw_value() - HIMEM_START + 1;
        params.e820_table[1].type_ = 1; // E820_RAM

        params.e820_entries = 2;

        // Write boot params to zero page
        let boot_params = BootParams::new(&params, GuestAddress(ZERO_PAGE_START));
        LinuxBootConfigurator::write_bootparams(&boot_params, &self.guest_memory)
            .map_err(|_| VmError::GuestMemoryWrite)?;

        Ok(())
    }

    fn setup_x86_64_boot(&self, entry_point: GuestAddress) -> Result<(), VmError> {
        // Set up GDT
        self.setup_gdt()?;

        // Set up page tables for long mode
        self.setup_page_tables()?;

        // Configure segments and special registers
        self.setup_sregs()?;

        // Set up general purpose registers
        self.setup_regs(entry_point)?;

        // Set up FPU
        self.setup_fpu()?;

        Ok(())
    }

    fn setup_gdt(&self) -> Result<(), VmError> {
        // GDT entries for 64-bit mode
        let gdt_table: [u64; 4] = [
            0,                           // NULL descriptor
            gdt_entry(0xa09b, 0, 0xfffff), // CODE segment (64-bit)
            gdt_entry(0xc093, 0, 0xfffff), // DATA segment
            gdt_entry(0x808b, 0, 0xfffff), // TSS
        ];

        // Write GDT to guest memory
        for (i, entry) in gdt_table.iter().enumerate() {
            let addr = GuestAddress(BOOT_GDT_OFFSET + (i as u64 * 8));
            self.guest_memory
                .write_obj(*entry, addr)
                .map_err(|_| VmError::GuestMemoryWrite)?;
        }

        // Write IDT (empty for now)
        self.guest_memory
            .write_obj(0u64, GuestAddress(BOOT_IDT_OFFSET))
            .map_err(|_| VmError::GuestMemoryWrite)?;

        Ok(())
    }

    fn setup_page_tables(&self) -> Result<(), VmError> {
        // Set up identity-mapped page tables for the first 1GB
        // PML4 entry pointing to PDPTE
        self.guest_memory
            .write_obj(PDPTE_START | 0x03, GuestAddress(PML4_START))
            .map_err(|_| VmError::GuestMemoryWrite)?;

        // PDPTE entry pointing to PDE
        self.guest_memory
            .write_obj(PDE_START | 0x03, GuestAddress(PDPTE_START))
            .map_err(|_| VmError::GuestMemoryWrite)?;

        // PDE entries: 512 entries of 2MB pages (covering 1GB)
        for i in 0..512u64 {
            let entry = (i << 21) | 0x83; // 2MB page, present, writable
            self.guest_memory
                .write_obj(entry, GuestAddress(PDE_START + i * 8))
                .map_err(|_| VmError::GuestMemoryWrite)?;
        }

        Ok(())
    }

    fn setup_sregs(&self) -> Result<(), VmError> {
        let mut sregs = self.vcpu.get_sregs().map_err(VmError::GetSregs)?;

        // Set up code segment
        sregs.cs = kvm_segment_from_gdt(gdt_entry(0xa09b, 0, 0xfffff), 1);

        // Set up data segments
        let data_seg = kvm_segment_from_gdt(gdt_entry(0xc093, 0, 0xfffff), 2);
        sregs.ds = data_seg;
        sregs.es = data_seg;
        sregs.fs = data_seg;
        sregs.gs = data_seg;
        sregs.ss = data_seg;

        // Set up TSS
        sregs.tr = kvm_segment_from_gdt(gdt_entry(0x808b, 0, 0xfffff), 3);

        // Set up GDT and IDT
        sregs.gdt.base = BOOT_GDT_OFFSET;
        sregs.gdt.limit = 4 * 8 - 1;
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

    fn filter_cpuid(&mut self) -> Result<(), VmError> {
        // Get supported CPUID
        let mut cpuid = self.kvm
            .get_supported_cpuid(KVM_MAX_CPUID_ENTRIES)
            .map_err(VmError::GetCpuid)?;

        // Filter out non-deterministic features
        for entry in cpuid.as_mut_slice() {
            match entry.function {
                CPUID_EXT_FEATURES => {
                    // Clear RDRAND (bit 30 of ECX)
                    entry.ecx &= !(1 << 30);
                }
                CPUID_STRUCTURED_EXT if entry.index == 0 => {
                    // Clear RDSEED (bit 18 of EBX)
                    entry.ebx &= !(1 << 18);
                }
                _ => {}
            }
        }

        self.vcpu.set_cpuid2(&cpuid).map_err(VmError::SetCpuid)?;
        Ok(())
    }

    fn pin_tsc(&self) -> Result<(), VmError> {
        // Set TSC to a fixed frequency (3.0 GHz)
        const TSC_KHZ: u32 = 3_000_000;
        self.vcpu
            .set_tsc_khz(TSC_KHZ)
            .map_err(VmError::SetTscKhz)?;
        
        info!("TSC pinned to {} KHz", TSC_KHZ);
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), VmError> {
        info!("Starting VM execution");
        
        loop {
            match self.vcpu.run() {
                Ok(exit_reason) => {
                    match exit_reason {
                        VcpuExit::IoIn(port, data) => {
                            if port >= SERIAL_PORT_BASE && port <= SERIAL_PORT_END {
                                let offset = (port - SERIAL_PORT_BASE) as u8;
                                data[0] = self.serial.read(offset);
                            } else {
                                for byte in data.iter_mut() {
                                    *byte = 0xff;
                                }
                            }
                        }
                        VcpuExit::IoOut(port, data) => {
                            if port >= SERIAL_PORT_BASE && port <= SERIAL_PORT_END {
                                let offset = (port - SERIAL_PORT_BASE) as u8;
                                let _ = self.serial.write(offset, data[0]);
                            }
                        }
                        VcpuExit::Hlt => {
                            info!("VM halted");
                            break;
                        }
                        VcpuExit::Shutdown => {
                            info!("VM shutdown");
                            break;
                        }
                        VcpuExit::MmioRead(addr, data) => {
                            debug!("MMIO read from 0x{:x}", addr);
                            // Fill with zeros
                            for byte in data {
                                *byte = 0;
                            }
                        }
                        VcpuExit::MmioWrite(addr, data) => {
                            debug!("MMIO write to 0x{:x}: {:?}", addr, data);
                        }
                        reason => {
                            info!("Unhandled exit reason: {:?}", reason);
                            break;
                        }
                    }
                }
                Err(e) => return Err(VmError::VcpuRun(e)),
            }
        }

        Ok(())
    }
}

// Helper function to create a GDT entry
fn gdt_entry(flags: u16, base: u32, limit: u32) -> u64 {
    ((u64::from(base) & 0xff00_0000u64) << (56 - 24))
        | ((u64::from(flags) & 0x0000_f0ffu64) << 40)
        | ((u64::from(limit) & 0x000f_0000u64) << (48 - 16))
        | ((u64::from(base) & 0x00ff_ffffu64) << 16)
        | (u64::from(limit) & 0x0000_ffffu64)
}

// Helper function to convert GDT entry to kvm_segment
fn kvm_segment_from_gdt(entry: u64, table_index: u8) -> kvm_segment {
    kvm_segment {
        base: get_base(entry),
        limit: get_limit(entry),
        selector: u16::from(table_index * 8),
        type_: get_type(entry),
        present: get_p(entry),
        dpl: get_dpl(entry),
        db: get_db(entry),
        s: get_s(entry),
        l: get_l(entry),
        g: get_g(entry),
        avl: get_avl(entry),
        padding: 0,
        unusable: if get_p(entry) == 0 { 1 } else { 0 },
    }
}

fn get_base(entry: u64) -> u64 {
    (((entry) & 0xFF00_0000_0000_0000) >> 32)
        | (((entry) & 0x0000_00FF_0000_0000) >> 16)
        | (((entry) & 0x0000_0000_FFFF_0000) >> 16)
}

fn get_limit(entry: u64) -> u32 {
    let limit: u32 =
        ((((entry) & 0x000F_0000_0000_0000) >> 32) | ((entry) & 0x0000_0000_0000_FFFF)) as u32;
    match get_g(entry) {
        0 => limit,
        _ => (limit << 12) | 0xFFF,
    }
}

fn get_g(entry: u64) -> u8 {
    ((entry & 0x0080_0000_0000_0000) >> 55) as u8
}

fn get_db(entry: u64) -> u8 {
    ((entry & 0x0040_0000_0000_0000) >> 54) as u8
}

fn get_l(entry: u64) -> u8 {
    ((entry & 0x0020_0000_0000_0000) >> 53) as u8
}

fn get_avl(entry: u64) -> u8 {
    ((entry & 0x0010_0000_0000_0000) >> 52) as u8
}

fn get_p(entry: u64) -> u8 {
    ((entry & 0x0000_8000_0000_0000) >> 47) as u8
}

fn get_dpl(entry: u64) -> u8 {
    ((entry & 0x0000_6000_0000_0000) >> 45) as u8
}

fn get_s(entry: u64) -> u8 {
    ((entry & 0x0000_1000_0000_0000) >> 44) as u8
}

fn get_type(entry: u64) -> u8 {
    ((entry & 0x0000_0F00_0000_0000) >> 40) as u8
}

fn create_guest_memory(size: usize) -> Result<GuestMemoryMmap, VmError> {
    let regions = vec![(GuestAddress(0), size)];
    
    GuestMemoryMmap::from_ranges(&regions)
        .map_err(|_| VmError::GuestMemoryCreate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gdt_entry() {
        let entry = gdt_entry(0xa09b, 0, 0xfffff);
        assert_ne!(entry, 0);
    }

    #[test]
    fn test_create_guest_memory() {
        let mem = create_guest_memory(128 * 1024 * 1024).unwrap();
        assert_eq!(mem.num_regions(), 1);
    }
}
