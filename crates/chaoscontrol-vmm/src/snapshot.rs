//! VM snapshot and restore — capture complete VM state and recreate it.

use crate::devices::entropy::EntropySnapshot;
use kvm_bindings::{
    kvm_clock_data, kvm_debugregs, kvm_fpu, kvm_irqchip, kvm_lapic_state, kvm_pit_state2,
    kvm_regs, kvm_sregs, kvm_xcrs, KVM_IRQCHIP_IOAPIC, KVM_IRQCHIP_PIC_MASTER,
    KVM_IRQCHIP_PIC_SLAVE,
};
use kvm_ioctls::{VcpuFd, VmFd};
use log::info;
use vm_memory::{Address, Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

/// Complete VM state — everything needed to restore a VM to an exact point.
#[derive(Clone, Debug)]
pub struct VmSnapshot {
    // vCPU register state
    pub regs: kvm_regs,
    pub sregs: kvm_sregs,
    pub fpu: kvm_fpu,
    pub debug_regs: kvm_debugregs,
    pub lapic: kvm_lapic_state,
    pub xcrs: kvm_xcrs,

    // In-kernel device state
    pub pic_master: kvm_irqchip,
    pub pic_slave: kvm_irqchip,
    pub ioapic: kvm_irqchip,
    pub pit: kvm_pit_state2,
    pub clock: kvm_clock_data,

    // Guest memory (full copy)
    pub memory: Vec<u8>,
    pub memory_size: usize,

    // Deterministic device state
    pub serial_state: vm_superio::SerialState,
    pub entropy: EntropySnapshot,

    // VMM-side determinism counters
    pub virtual_tsc: u64,
    pub exit_count: u64,
    pub io_exit_count: u64,
    pub exits_since_last_sdk: u64,
}

impl VmSnapshot {
    /// Capture the complete state of a running VM.
    pub fn capture(
        vcpu: &VcpuFd,
        vm: &VmFd,
        guest_memory: &GuestMemoryMmap,
        serial_state: vm_superio::SerialState,
        entropy: EntropySnapshot,
        virtual_tsc: u64,
        exit_count: u64,
        io_exit_count: u64,
        exits_since_last_sdk: u64,
    ) -> Result<Self, SnapshotError> {
        // Capture vCPU state
        let regs = vcpu.get_regs().map_err(SnapshotError::GetRegs)?;
        let sregs = vcpu.get_sregs().map_err(SnapshotError::GetSregs)?;
        let fpu = vcpu.get_fpu().map_err(SnapshotError::GetFpu)?;
        let debug_regs = vcpu.get_debug_regs().map_err(SnapshotError::GetDebugRegs)?;
        let lapic = vcpu.get_lapic().map_err(SnapshotError::GetLapic)?;
        let xcrs = vcpu.get_xcrs().map_err(SnapshotError::GetXcrs)?;

        // Capture in-kernel IRQ chip state (3 chips: PIC master, PIC slave, IOAPIC)
        let mut pic_master = kvm_irqchip {
            chip_id: KVM_IRQCHIP_PIC_MASTER,
            ..Default::default()
        };
        vm.get_irqchip(&mut pic_master)
            .map_err(SnapshotError::GetIrqchip)?;

        let mut pic_slave = kvm_irqchip {
            chip_id: KVM_IRQCHIP_PIC_SLAVE,
            ..Default::default()
        };
        vm.get_irqchip(&mut pic_slave)
            .map_err(SnapshotError::GetIrqchip)?;

        let mut ioapic = kvm_irqchip {
            chip_id: KVM_IRQCHIP_IOAPIC,
            ..Default::default()
        };
        vm.get_irqchip(&mut ioapic)
            .map_err(SnapshotError::GetIrqchip)?;

        // Capture PIT and clock
        let pit = vm.get_pit2().map_err(SnapshotError::GetPit)?;
        let clock = vm.get_clock().map_err(SnapshotError::GetClock)?;

        // Capture guest memory
        let memory_size = guest_memory.last_addr().raw_value() as usize + 1;
        let mut memory = vec![0u8; memory_size];
        guest_memory
            .read_slice(&mut memory, GuestAddress(0))
            .map_err(|_| SnapshotError::ReadMemory)?;

        info!(
            "Snapshot captured: {} MB memory, RIP=0x{:x}, RSP=0x{:x}",
            memory_size / 1024 / 1024,
            regs.rip,
            regs.rsp
        );

        Ok(Self {
            regs,
            sregs,
            fpu,
            debug_regs,
            lapic,
            xcrs,
            pic_master,
            pic_slave,
            ioapic,
            pit,
            clock,
            memory,
            memory_size,
            serial_state,
            entropy,
            virtual_tsc,
            exit_count,
            io_exit_count,
            exits_since_last_sdk,
        })
    }

    /// Restore VM state from this snapshot.
    pub fn restore(
        &self,
        vcpu: &VcpuFd,
        vm: &VmFd,
        guest_memory: &GuestMemoryMmap,
    ) -> Result<(), SnapshotError> {
        // Restore guest memory
        guest_memory
            .write_slice(&self.memory, GuestAddress(0))
            .map_err(|_| SnapshotError::WriteMemory)?;

        // Restore in-kernel devices BEFORE vCPU state
        vm.set_pit2(&self.pit).map_err(SnapshotError::SetPit)?;
        vm.set_clock(&self.clock).map_err(SnapshotError::SetClock)?;
        vm.set_irqchip(&self.pic_master)
            .map_err(SnapshotError::SetIrqchip)?;
        vm.set_irqchip(&self.pic_slave)
            .map_err(SnapshotError::SetIrqchip)?;
        vm.set_irqchip(&self.ioapic)
            .map_err(SnapshotError::SetIrqchip)?;

        // Restore vCPU state
        vcpu.set_sregs(&self.sregs)
            .map_err(SnapshotError::SetSregs)?;
        vcpu.set_regs(&self.regs).map_err(SnapshotError::SetRegs)?;
        vcpu.set_fpu(&self.fpu).map_err(SnapshotError::SetFpu)?;
        vcpu.set_debug_regs(&self.debug_regs)
            .map_err(SnapshotError::SetDebugRegs)?;
        vcpu.set_lapic(&self.lapic)
            .map_err(SnapshotError::SetLapic)?;
        vcpu.set_xcrs(&self.xcrs).map_err(SnapshotError::SetXcrs)?;

        info!(
            "Snapshot restored: RIP=0x{:x}, RSP=0x{:x}",
            self.regs.rip, self.regs.rsp
        );

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("Failed to get registers: {0}")]
    GetRegs(kvm_ioctls::Error),
    #[error("Failed to get special registers: {0}")]
    GetSregs(kvm_ioctls::Error),
    #[error("Failed to get FPU: {0}")]
    GetFpu(kvm_ioctls::Error),
    #[error("Failed to get debug registers: {0}")]
    GetDebugRegs(kvm_ioctls::Error),
    #[error("Failed to get LAPIC: {0}")]
    GetLapic(kvm_ioctls::Error),
    #[error("Failed to get XCRs: {0}")]
    GetXcrs(kvm_ioctls::Error),
    #[error("Failed to get IRQ chip: {0}")]
    GetIrqchip(kvm_ioctls::Error),
    #[error("Failed to get PIT: {0}")]
    GetPit(kvm_ioctls::Error),
    #[error("Failed to get clock: {0}")]
    GetClock(kvm_ioctls::Error),
    #[error("Failed to read guest memory")]
    ReadMemory,
    #[error("Failed to set registers: {0}")]
    SetRegs(kvm_ioctls::Error),
    #[error("Failed to set special registers: {0}")]
    SetSregs(kvm_ioctls::Error),
    #[error("Failed to set FPU: {0}")]
    SetFpu(kvm_ioctls::Error),
    #[error("Failed to set debug registers: {0}")]
    SetDebugRegs(kvm_ioctls::Error),
    #[error("Failed to set LAPIC: {0}")]
    SetLapic(kvm_ioctls::Error),
    #[error("Failed to set XCRs: {0}")]
    SetXcrs(kvm_ioctls::Error),
    #[error("Failed to set IRQ chip: {0}")]
    SetIrqchip(kvm_ioctls::Error),
    #[error("Failed to set PIT: {0}")]
    SetPit(kvm_ioctls::Error),
    #[error("Failed to set clock: {0}")]
    SetClock(kvm_ioctls::Error),
    #[error("Failed to write guest memory")]
    WriteMemory,
}
