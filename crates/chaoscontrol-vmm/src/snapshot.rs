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
use snafu::ResultExt;
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
            regs: vcpu.get_regs().context(GetRegsSnafu)?,
            sregs: vcpu.get_sregs().context(GetSregsSnafu)?,
            fpu: vcpu.get_fpu().context(GetFpuSnafu)?,
            debug_regs: vcpu.get_debug_regs().context(GetDebugRegsSnafu)?,
            lapic: vcpu.get_lapic().context(GetLapicSnafu)?,
            xcrs: vcpu.get_xcrs().context(GetXcrsSnafu)?,
            mp_state: vcpu.get_mp_state().context(GetMpStateSnafu)?,
        })
    }

    /// Restore all register state to a single vCPU.
    pub fn restore(&self, vcpu: &VcpuFd) -> Result<(), SnapshotError> {
        // MP state MUST be set before registers — KVM refuses register
        // writes on vCPUs in UNINITIALIZED state on some host kernels.
        vcpu.set_mp_state(self.mp_state).context(SetMpStateSnafu)?;
        vcpu.set_sregs(&self.sregs).context(SetSregsSnafu)?;
        vcpu.set_regs(&self.regs).context(SetRegsSnafu)?;
        vcpu.set_fpu(&self.fpu).context(SetFpuSnafu)?;
        vcpu.set_debug_regs(&self.debug_regs)
            .context(SetDebugRegsSnafu)?;
        vcpu.set_lapic(&self.lapic).context(SetLapicSnafu)?;
        vcpu.set_xcrs(&self.xcrs).context(SetXcrsSnafu)?;
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
        vm.get_irqchip(&mut pic_master).context(GetIrqchipSnafu)?;

        let mut pic_slave = kvm_irqchip {
            chip_id: KVM_IRQCHIP_PIC_SLAVE,
            ..Default::default()
        };
        vm.get_irqchip(&mut pic_slave).context(GetIrqchipSnafu)?;

        let mut ioapic = kvm_irqchip {
            chip_id: KVM_IRQCHIP_IOAPIC,
            ..Default::default()
        };
        vm.get_irqchip(&mut ioapic).context(GetIrqchipSnafu)?;

        // Capture PIT and clock
        let pit = vm.get_pit2().context(GetPitSnafu)?;
        let clock = vm.get_clock().context(GetClockSnafu)?;

        // Capture guest memory
        let memory_size = guest_memory.last_addr().raw_value() as usize + 1;
        let mut memory = vec![0u8; memory_size];
        guest_memory
            .read_slice(&mut memory, GuestAddress(0))
            .map_err(|_| ReadMemorySnafu.build())?;

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
            return VcpuCountMismatchSnafu {
                snapshot: self.vcpu_snapshots.len(),
                current: vcpus.len(),
            }
            .fail();
        }

        // Restore guest memory
        guest_memory
            .write_slice(&self.memory, GuestAddress(0))
            .map_err(|_| WriteMemorySnafu.build())?;

        // Restore in-kernel devices BEFORE vCPU state
        vm.set_pit2(&self.pit).context(SetPitSnafu)?;
        vm.set_clock(&self.clock).context(SetClockSnafu)?;
        vm.set_irqchip(&self.pic_master).context(SetIrqchipSnafu)?;
        vm.set_irqchip(&self.pic_slave).context(SetIrqchipSnafu)?;
        vm.set_irqchip(&self.ioapic).context(SetIrqchipSnafu)?;

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

#[derive(Debug, snafu::Snafu)]
pub enum SnapshotError {
    #[snafu(display("Failed to get registers"))]
    GetRegs { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get special registers"))]
    GetSregs { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get FPU"))]
    GetFpu { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get debug registers"))]
    GetDebugRegs { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get LAPIC"))]
    GetLapic { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get XCRs"))]
    GetXcrs { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get MP state"))]
    GetMpState { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get IRQ chip"))]
    GetIrqchip { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get PIT"))]
    GetPit { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get clock"))]
    GetClock { source: kvm_ioctls::Error },
    #[snafu(display("Failed to read guest memory"))]
    ReadMemory,
    #[snafu(display("Failed to set registers"))]
    SetRegs { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set special registers"))]
    SetSregs { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set FPU"))]
    SetFpu { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set debug registers"))]
    SetDebugRegs { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set LAPIC"))]
    SetLapic { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set XCRs"))]
    SetXcrs { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set MP state"))]
    SetMpState { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set IRQ chip"))]
    SetIrqchip { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set PIT"))]
    SetPit { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set clock"))]
    SetClock { source: kvm_ioctls::Error },
    #[snafu(display("Failed to write guest memory"))]
    WriteMemory,
    #[snafu(display("vCPU count mismatch: snapshot has {snapshot}, VM has {current}"))]
    VcpuCountMismatch { snapshot: usize, current: usize },
}
