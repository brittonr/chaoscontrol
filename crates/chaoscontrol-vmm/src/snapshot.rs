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
use std::collections::BTreeMap;
use std::sync::Arc;
use vm_memory::{Address, Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

/// Page size for dirty tracking granularity (4 KiB).
pub const PAGE_SIZE: usize = 4096;

/// Guest memory stored as either a full copy or a sparse dirty-page overlay.
///
/// The `Overlay` variant shares an immutable base image via `Arc` and
/// stores only the 4 KB pages that differ. Cloning an overlay copies
/// only the dirty-page map (O(dirty pages)), not the full base.
#[derive(Debug)]
pub enum SnapshotMemory {
    /// Full contiguous copy of guest memory.
    Full(Vec<u8>),

    /// Sparse overlay on top of a shared base.
    ///
    /// `base` is the snapshot taken at the start of the round (or at
    /// bootstrap). `dirty_pages` maps page index → page contents for
    /// pages that changed since the base was captured.
    Overlay {
        base: Arc<Vec<u8>>,
        dirty_pages: BTreeMap<usize, Box<[u8; PAGE_SIZE]>>,
    },
}

impl Clone for SnapshotMemory {
    fn clone(&self) -> Self {
        match self {
            Self::Full(data) => Self::Full(data.clone()),
            Self::Overlay { base, dirty_pages } => Self::Overlay {
                base: Arc::clone(base),
                dirty_pages: dirty_pages.clone(),
            },
        }
    }
}

impl SnapshotMemory {
    /// Total size of the guest memory this snapshot represents.
    pub fn memory_size(&self) -> usize {
        match self {
            Self::Full(data) => data.len(),
            Self::Overlay { base, .. } => base.len(),
        }
    }

    /// Number of dirty pages in the overlay (0 for Full).
    pub fn dirty_page_count(&self) -> usize {
        match self {
            Self::Full(_) => 0,
            Self::Overlay { dirty_pages, .. } => dirty_pages.len(),
        }
    }

    /// Materialize the snapshot into a contiguous byte vector.
    ///
    /// For `Full`, returns a clone. For `Overlay`, applies dirty pages
    /// on top of the base to produce the complete memory image.
    pub fn materialize(&self) -> Vec<u8> {
        match self {
            Self::Full(data) => data.clone(),
            Self::Overlay { base, dirty_pages } => {
                let mut result = (**base).clone();
                for (&page_idx, page_data) in dirty_pages {
                    let offset = page_idx * PAGE_SIZE;
                    if offset + PAGE_SIZE <= result.len() {
                        result[offset..offset + PAGE_SIZE].copy_from_slice(page_data.as_ref());
                    }
                }
                result
            }
        }
    }

    /// Build an overlay snapshot by reading only dirty pages from guest
    /// memory.
    ///
    /// `dirty_bitmap` is the KVM dirty log: each bit represents one
    /// 4 KB page. Bit N of element `N/64` (bit position `N%64`) is set
    /// if page N was written by the guest.
    pub fn from_dirty(
        base: &Arc<Vec<u8>>,
        dirty_bitmap: &[u64],
        guest_memory: &GuestMemoryMmap,
    ) -> Self {
        let mem_size = base.len();
        let total_pages = mem_size / PAGE_SIZE;
        let mut dirty_pages = BTreeMap::new();

        for page_idx in 0..total_pages {
            let word = page_idx / 64;
            let bit = page_idx % 64;
            if word < dirty_bitmap.len() && (dirty_bitmap[word] >> bit) & 1 == 1 {
                let mut page = Box::new([0u8; PAGE_SIZE]);
                let gpa = GuestAddress((page_idx * PAGE_SIZE) as u64);
                if guest_memory.read_slice(page.as_mut(), gpa).is_ok() {
                    dirty_pages.insert(page_idx, page);
                }
            }
        }

        Self::Overlay {
            base: Arc::clone(base),
            dirty_pages,
        }
    }

    /// Write this snapshot's memory to guest physical memory.
    ///
    /// For `Full`, writes the entire region. For `Overlay`, writes only
    /// the dirty pages. The caller is responsible for ensuring the base
    /// content is already present in guest memory (via a prior full
    /// restore or by reverting the previous branch's dirty pages).
    pub fn write_to_guest(&self, guest_memory: &GuestMemoryMmap) -> Result<(), SnapshotError> {
        match self {
            Self::Full(data) => {
                guest_memory
                    .write_slice(data, GuestAddress(0))
                    .map_err(|_| WriteMemorySnafu.build())?;
            }
            Self::Overlay { dirty_pages, .. } => {
                for (&page_idx, page_data) in dirty_pages {
                    let gpa = GuestAddress((page_idx * PAGE_SIZE) as u64);
                    guest_memory
                        .write_slice(page_data.as_ref(), gpa)
                        .map_err(|_| WriteMemorySnafu.build())?;
                }
            }
        }
        Ok(())
    }

    /// Revert specific pages in guest memory back to their base values.
    ///
    /// Used between branches in the same round: revert the previous
    /// branch's dirty pages from the base, so the next branch starts
    /// from a clean base state.
    pub fn revert_pages_from_base(
        base: &[u8],
        page_indices: impl Iterator<Item = usize>,
        guest_memory: &GuestMemoryMmap,
    ) -> Result<(), SnapshotError> {
        for page_idx in page_indices {
            let offset = page_idx * PAGE_SIZE;
            if offset + PAGE_SIZE <= base.len() {
                let gpa = GuestAddress(offset as u64);
                guest_memory
                    .write_slice(&base[offset..offset + PAGE_SIZE], gpa)
                    .map_err(|_| WriteMemorySnafu.build())?;
            }
        }
        Ok(())
    }
}

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

    // Guest memory (full copy or sparse overlay)
    pub memory: SnapshotMemory,

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

        // Capture guest memory (full copy for initial snapshots)
        let memory_size = guest_memory.last_addr().raw_value() as usize + 1;
        let mut mem_data = vec![0u8; memory_size];
        guest_memory
            .read_slice(&mut mem_data, GuestAddress(0))
            .map_err(|_| ReadMemorySnafu.build())?;
        let memory = SnapshotMemory::Full(mem_data);

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

        // Restore guest memory (handles both Full and Overlay)
        self.memory.write_to_guest(guest_memory)?;

        // Restore KVM devices and vCPU registers
        self.restore_devices_only(vcpus, vm)?;

        info!(
            "Snapshot restored: {} vCPUs, BSP RIP=0x{:x}",
            self.vcpu_snapshots.len(),
            self.vcpu_snapshots[0].regs.rip,
        );

        Ok(())
    }

    /// Restore in-kernel device state and vCPU registers without
    /// touching guest memory. Used by incremental restore where
    /// memory is handled separately.
    pub fn restore_devices_only(&self, vcpus: &[VcpuFd], vm: &VmFd) -> Result<(), SnapshotError> {
        if vcpus.len() != self.vcpu_snapshots.len() {
            return VcpuCountMismatchSnafu {
                snapshot: self.vcpu_snapshots.len(),
                current: vcpus.len(),
            }
            .fail();
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_base(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    #[test]
    fn full_materialize_is_identity() {
        let data = make_base(PAGE_SIZE * 4);
        let snap = SnapshotMemory::Full(data.clone());
        assert_eq!(snap.materialize(), data);
    }

    #[test]
    fn full_memory_size() {
        let snap = SnapshotMemory::Full(vec![0u8; PAGE_SIZE * 10]);
        assert_eq!(snap.memory_size(), PAGE_SIZE * 10);
    }

    #[test]
    fn full_dirty_page_count_is_zero() {
        let snap = SnapshotMemory::Full(vec![0u8; PAGE_SIZE]);
        assert_eq!(snap.dirty_page_count(), 0);
    }

    #[test]
    fn overlay_materialize_applies_dirty_pages() {
        let base = make_base(PAGE_SIZE * 4);
        let base_arc = Arc::new(base.clone());

        let mut dirty = BTreeMap::new();
        // Override page 1 with all 0xFF
        dirty.insert(1, Box::new([0xFF; PAGE_SIZE]));
        // Override page 3 with all 0xAA
        dirty.insert(3, Box::new([0xAA; PAGE_SIZE]));

        let snap = SnapshotMemory::Overlay {
            base: Arc::clone(&base_arc),
            dirty_pages: dirty,
        };

        let mat = snap.materialize();
        assert_eq!(mat.len(), PAGE_SIZE * 4);

        // Page 0: unchanged from base
        assert_eq!(&mat[..PAGE_SIZE], &base[..PAGE_SIZE]);

        // Page 1: all 0xFF
        assert!(mat[PAGE_SIZE..PAGE_SIZE * 2].iter().all(|&b| b == 0xFF));

        // Page 2: unchanged
        assert_eq!(
            &mat[PAGE_SIZE * 2..PAGE_SIZE * 3],
            &base[PAGE_SIZE * 2..PAGE_SIZE * 3]
        );

        // Page 3: all 0xAA
        assert!(mat[PAGE_SIZE * 3..PAGE_SIZE * 4].iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn overlay_memory_size() {
        let base = Arc::new(vec![0u8; PAGE_SIZE * 8]);
        let snap = SnapshotMemory::Overlay {
            base,
            dirty_pages: BTreeMap::new(),
        };
        assert_eq!(snap.memory_size(), PAGE_SIZE * 8);
    }

    #[test]
    fn overlay_dirty_page_count() {
        let base = Arc::new(vec![0u8; PAGE_SIZE * 4]);
        let mut dirty = BTreeMap::new();
        dirty.insert(0, Box::new([0u8; PAGE_SIZE]));
        dirty.insert(2, Box::new([0u8; PAGE_SIZE]));
        let snap = SnapshotMemory::Overlay {
            base,
            dirty_pages: dirty,
        };
        assert_eq!(snap.dirty_page_count(), 2);
    }

    #[test]
    fn clone_overlay_shares_base_arc() {
        let base = Arc::new(vec![0u8; PAGE_SIZE * 4]);
        let mut dirty = BTreeMap::new();
        dirty.insert(1, Box::new([0xBB; PAGE_SIZE]));

        let snap = SnapshotMemory::Overlay {
            base: Arc::clone(&base),
            dirty_pages: dirty,
        };
        let cloned = snap.clone();

        // Both point to the same base allocation
        if let (
            SnapshotMemory::Overlay {
                base: b1,
                dirty_pages: d1,
            },
            SnapshotMemory::Overlay {
                base: b2,
                dirty_pages: d2,
            },
        ) = (&snap, &cloned)
        {
            assert!(Arc::ptr_eq(b1, b2), "clone must share Arc base");
            assert_eq!(d1.len(), d2.len());
        } else {
            panic!("expected Overlay variants");
        }

        // Materialized content is identical
        assert_eq!(snap.materialize(), cloned.materialize());
    }

    #[test]
    fn empty_overlay_materializes_to_base() {
        let base = make_base(PAGE_SIZE * 2);
        let snap = SnapshotMemory::Overlay {
            base: Arc::new(base.clone()),
            dirty_pages: BTreeMap::new(),
        };
        assert_eq!(snap.materialize(), base);
    }

    #[test]
    fn from_dirty_picks_correct_pages() {
        // Create a GuestMemoryMmap with known content
        let mem_size = PAGE_SIZE * 4;
        let regions = vec![(GuestAddress(0), mem_size)];
        let guest_mem = GuestMemoryMmap::from_ranges(&regions).unwrap();

        // Write recognizable patterns to each page
        for page in 0..4 {
            let pattern = vec![(page as u8 + 1) * 0x11; PAGE_SIZE];
            guest_mem
                .write_slice(&pattern, GuestAddress((page * PAGE_SIZE) as u64))
                .unwrap();
        }

        // Base is all zeros (different from guest memory)
        let base = Arc::new(vec![0u8; mem_size]);

        // Dirty bitmap: pages 1 and 3 are dirty
        // Word 0: bit 1 and bit 3 set = 0b1010 = 10
        let dirty_bitmap = vec![0b1010u64];

        let snap = SnapshotMemory::from_dirty(&base, &dirty_bitmap, &guest_mem);

        if let SnapshotMemory::Overlay { dirty_pages, .. } = &snap {
            assert_eq!(dirty_pages.len(), 2);
            assert!(dirty_pages.contains_key(&1));
            assert!(dirty_pages.contains_key(&3));

            // Page 1 should have pattern 0x22
            assert!(dirty_pages[&1].iter().all(|&b| b == 0x22));
            // Page 3 should have pattern 0x44
            assert!(dirty_pages[&3].iter().all(|&b| b == 0x44));
        } else {
            panic!("expected Overlay");
        }
    }
}
