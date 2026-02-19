//! VM snapshot and restore — capture complete VM state and recreate it.

use crate::devices::block::BlockSnapshot;
use crate::devices::entropy::EntropySnapshot;
use crate::devices::pit::PitSnapshot;
use crate::scheduler::SchedulerSnapshot;
use chaoscontrol_fault::engine::EngineSnapshot;
use kvm_bindings::{
    kvm_clock_data, kvm_debugregs, kvm_fpu, kvm_irqchip, kvm_lapic_state, kvm_mp_state,
    kvm_pit_state2, kvm_regs, kvm_sregs, kvm_xcrs, KVM_IRQCHIP_IOAPIC, KVM_IRQCHIP_PIC_MASTER,
    KVM_IRQCHIP_PIC_SLAVE,
};
use kvm_ioctls::{VcpuFd, VmFd};
use log::info;
use vm_memory::{Address, Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

/// Snapshot of a single virtio device's host-side state.
#[derive(Clone, Debug)]
pub struct VirtioDeviceSnapshot {
    /// Device type ID (2 = block, 1 = net, 4 = rng).
    pub device_id: u32,
    /// Block device data snapshot (only for block devices).
    pub block_snapshot: Option<BlockSnapshot>,
}

/// VMM-side state parameters for snapshot capture.
///
/// Groups the non-KVM state to avoid excessive function arguments.
pub struct CaptureParams {
    pub serial_state: vm_superio::SerialState,
    pub entropy: EntropySnapshot,
    pub virtual_tsc: u64,
    pub exit_count: u64,
    pub io_exit_count: u64,
    pub exits_since_last_sdk: u64,
    pub pit_snapshot: PitSnapshot,
    pub last_kvm_pit_mode: u8,
    pub fault_engine_snapshot: EngineSnapshot,
    pub virtio_snapshots: Vec<VirtioDeviceSnapshot>,
    pub coverage_active: bool,
    pub scheduler_snapshot: SchedulerSnapshot,
    pub singlestep_remaining: u64,
}

/// Per-vCPU register state for snapshot/restore.
#[derive(Clone, Debug)]
pub struct VcpuSnapshot {
    pub regs: kvm_regs,
    pub sregs: kvm_sregs,
    pub fpu: kvm_fpu,
    pub debug_regs: kvm_debugregs,
    pub lapic: kvm_lapic_state,
    pub xcrs: kvm_xcrs,
    /// MP state (RUNNABLE, HALTED, UNINITIALIZED, etc.).
    /// Critical for SMP: without this, KVM doesn't know whether an AP
    /// should be running or waiting for SIPI after restore.
    pub mp_state: kvm_mp_state,
}

impl VcpuSnapshot {
    /// Capture all register state from a single vCPU.
    pub fn capture(vcpu: &VcpuFd) -> Result<Self, SnapshotError> {
        Ok(Self {
            regs: vcpu.get_regs().map_err(SnapshotError::GetRegs)?,
            sregs: vcpu.get_sregs().map_err(SnapshotError::GetSregs)?,
            fpu: vcpu.get_fpu().map_err(SnapshotError::GetFpu)?,
            debug_regs: vcpu.get_debug_regs().map_err(SnapshotError::GetDebugRegs)?,
            lapic: vcpu.get_lapic().map_err(SnapshotError::GetLapic)?,
            xcrs: vcpu.get_xcrs().map_err(SnapshotError::GetXcrs)?,
            mp_state: vcpu.get_mp_state().map_err(SnapshotError::GetMpState)?,
        })
    }

    /// Restore all register state to a single vCPU.
    pub fn restore(&self, vcpu: &VcpuFd) -> Result<(), SnapshotError> {
        // MP state MUST be set before registers — KVM refuses register
        // writes on vCPUs in UNINITIALIZED state on some host kernels.
        vcpu.set_mp_state(self.mp_state)
            .map_err(SnapshotError::SetMpState)?;
        vcpu.set_sregs(&self.sregs)
            .map_err(SnapshotError::SetSregs)?;
        vcpu.set_regs(&self.regs).map_err(SnapshotError::SetRegs)?;
        vcpu.set_fpu(&self.fpu).map_err(SnapshotError::SetFpu)?;
        vcpu.set_debug_regs(&self.debug_regs)
            .map_err(SnapshotError::SetDebugRegs)?;
        vcpu.set_lapic(&self.lapic)
            .map_err(SnapshotError::SetLapic)?;
        vcpu.set_xcrs(&self.xcrs).map_err(SnapshotError::SetXcrs)?;
        Ok(())
    }
}

/// Complete VM state — everything needed to restore a VM to an exact point.
#[derive(Clone, Debug)]
pub struct VmSnapshot {
    /// Per-vCPU register state (one entry per vCPU).
    pub vcpu_snapshots: Vec<VcpuSnapshot>,

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

    // DeterministicPit state
    pub pit_snapshot: PitSnapshot,
    pub last_kvm_pit_mode: u8,

    // Fault engine state
    pub fault_engine_snapshot: EngineSnapshot,

    // Virtio device state
    pub virtio_snapshots: Vec<VirtioDeviceSnapshot>,
    pub coverage_active: bool,

    /// Index of the active vCPU at snapshot time.
    pub active_vcpu: usize,

    /// vCPU scheduler state.
    pub scheduler_snapshot: SchedulerSnapshot,

    /// Single-step remaining count (for exact SMP preemption).
    pub singlestep_remaining: u64,
}

impl VmSnapshot {
    /// Convenience accessor: RIP of vCPU 0 (BSP).
    pub fn rip(&self) -> u64 {
        self.vcpu_snapshots[0].regs.rip
    }

    /// Capture the complete state of a running VM.
    pub fn capture(
        vcpus: &[VcpuFd],
        vm: &VmFd,
        guest_memory: &GuestMemoryMmap,
        params: CaptureParams,
    ) -> Result<Self, SnapshotError> {
        // Capture per-vCPU state
        let mut vcpu_snapshots = Vec::with_capacity(vcpus.len());
        for vcpu in vcpus {
            vcpu_snapshots.push(VcpuSnapshot::capture(vcpu)?);
        }

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
            "Snapshot captured: {} vCPUs, {} MB memory, BSP RIP=0x{:x}",
            vcpu_snapshots.len(),
            memory_size / 1024 / 1024,
            vcpu_snapshots[0].regs.rip,
        );

        Ok(Self {
            vcpu_snapshots,
            pic_master,
            pic_slave,
            ioapic,
            pit,
            clock,
            memory,
            memory_size,
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
        })
    }

    /// Restore VM state from this snapshot.
    pub fn restore(
        &self,
        vcpus: &[VcpuFd],
        vm: &VmFd,
        guest_memory: &GuestMemoryMmap,
    ) -> Result<(), SnapshotError> {
        if vcpus.len() != self.vcpu_snapshots.len() {
            return Err(SnapshotError::VcpuCountMismatch {
                snapshot: self.vcpu_snapshots.len(),
                current: vcpus.len(),
            });
        }

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

        // Restore per-vCPU state
        for (vcpu_snap, vcpu) in self.vcpu_snapshots.iter().zip(vcpus.iter()) {
            vcpu_snap.restore(vcpu)?;
        }

        info!(
            "Snapshot restored: {} vCPUs, BSP RIP=0x{:x}",
            self.vcpu_snapshots.len(),
            self.vcpu_snapshots[0].regs.rip,
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
    #[error("Failed to get MP state: {0}")]
    GetMpState(kvm_ioctls::Error),
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
    #[error("Failed to set MP state: {0}")]
    SetMpState(kvm_ioctls::Error),
    #[error("Failed to set IRQ chip: {0}")]
    SetIrqchip(kvm_ioctls::Error),
    #[error("Failed to set PIT: {0}")]
    SetPit(kvm_ioctls::Error),
    #[error("Failed to set clock: {0}")]
    SetClock(kvm_ioctls::Error),
    #[error("Failed to write guest memory")]
    WriteMemory,
    #[error("vCPU count mismatch: snapshot has {snapshot}, VM has {current}")]
    VcpuCountMismatch { snapshot: usize, current: usize },
}
