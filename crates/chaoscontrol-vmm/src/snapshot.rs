//! VM snapshot and restore — capture complete VM state and recreate it.

use crate::devices::block::BlockSnapshot;
use crate::devices::entropy::EntropySnapshot;
use crate::devices::net::NetSnapshot;
use crate::devices::pit::PitSnapshot;
use crate::devices::virtio_mmio::VirtioMmioSnapshot;
use crate::scheduler::core::MAX_SCHEDULE_VCPUS;
use crate::scheduler::SchedulerSnapshot;
use chaoscontrol_fault::engine::EngineSnapshot;
use kvm_bindings::{
    kvm_clock_data, kvm_debugregs, kvm_fpu, kvm_irqchip, kvm_lapic_state, kvm_mp_state,
    kvm_msr_entry, kvm_pit_state2, kvm_regs, kvm_sregs, kvm_vcpu_events, kvm_xcrs, kvm_xsave, Msrs,
    KVM_IRQCHIP_IOAPIC, KVM_IRQCHIP_PIC_MASTER, KVM_IRQCHIP_PIC_SLAVE,
};
use kvm_ioctls::{VcpuFd, VmFd};
use log::info;
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Deserialize)]
enum SnapshotMemorySerde {
    Full(Vec<u8>),
    Overlay {
        base: Vec<u8>,
        dirty_pages: BTreeMap<usize, Vec<u8>>,
    },
}

impl From<&SnapshotMemory> for SnapshotMemorySerde {
    fn from(memory: &SnapshotMemory) -> Self {
        match memory {
            SnapshotMemory::Full(data) => Self::Full(data.clone()),
            SnapshotMemory::Overlay { base, dirty_pages } => Self::Overlay {
                base: (**base).clone(),
                dirty_pages: dirty_pages
                    .iter()
                    .map(|(page, data)| (*page, data.to_vec()))
                    .collect(),
            },
        }
    }
}

impl TryFrom<SnapshotMemorySerde> for SnapshotMemory {
    type Error = String;

    fn try_from(value: SnapshotMemorySerde) -> Result<Self, Self::Error> {
        match value {
            SnapshotMemorySerde::Full(data) => Ok(Self::Full(data)),
            SnapshotMemorySerde::Overlay { base, dirty_pages } => {
                let mut pages = BTreeMap::new();
                for (page, data) in dirty_pages {
                    if data.len() != PAGE_SIZE {
                        return Err(format!(
                            "dirty page {page} byte length mismatch: expected {PAGE_SIZE}, got {}",
                            data.len()
                        ));
                    }
                    let mut page_data = [0u8; PAGE_SIZE];
                    page_data.copy_from_slice(&data);
                    pages.insert(page, Box::new(page_data));
                }
                Ok(Self::Overlay {
                    base: Arc::new(base),
                    dirty_pages: pages,
                })
            }
        }
    }
}

impl Serialize for SnapshotMemory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SnapshotMemorySerde::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SnapshotMemory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        SnapshotMemorySerde::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
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

    /// Read retained bytes without materializing an image or touching guest memory.
    /// Overlay reads may cross page boundaries; dirty bytes take precedence over base bytes.
    pub fn read_u16(&self, address: u64) -> Option<u16> {
        let start = usize::try_from(address).ok()?;
        let end = start.checked_add(std::mem::size_of::<u16>())?;
        if end > self.memory_size() {
            return None;
        }
        let byte = |offset: usize| match self {
            Self::Full(data) => data[offset],
            Self::Overlay { base, dirty_pages } => dirty_pages
                .get(&(offset / PAGE_SIZE))
                .map_or(base[offset], |page| page[offset % PAGE_SIZE]),
        };
        Some(u16::from_le_bytes([byte(start), byte(start + 1)]))
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

    /// Validate memory geometry before any guest-memory write.
    pub fn validate_for_guest_size(&self, guest_size: usize) -> Result<(), String> {
        if self.memory_size() != guest_size {
            return Err(format!(
                "snapshot memory size {} differs from guest size {guest_size}",
                self.memory_size()
            ));
        }
        if let Self::Overlay { dirty_pages, .. } = self {
            let total_pages = guest_size.div_ceil(PAGE_SIZE);
            for page in dirty_pages.keys() {
                if *page >= total_pages {
                    return Err(format!(
                        "snapshot dirty page {page} exceeds guest page count {total_pages}"
                    ));
                }
            }
        }
        Ok(())
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

pub const SNAPSHOT_STATE_SCHEMA_VERSION: u32 = 3;
pub const SNAPSHOT_PROFILE_EXACT_X86_KVM_V1: &str = "exact-x86-kvm-v1";

const VIRTIO_NET_DEVICE_ID: u32 = 1;
const VIRTIO_BLOCK_DEVICE_ID: u32 = 2;
const VIRTIO_ENTROPY_DEVICE_ID: u32 = 4;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VirtioDeviceIdentity {
    pub base_addr: u64,
    pub irq: u32,
    pub device_id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SnapshotComponent {
    GuestMemory,
    InKernelIrqChip,
    InKernelPit,
    InKernelClock,
    VmmDeterminism,
    Serial,
    FaultEngine,
    Scheduler,
    VcpuArchitecture {
        vcpu_id: u32,
    },
    VcpuEvents {
        vcpu_id: u32,
    },
    VcpuMsrs {
        vcpu_id: u32,
    },
    VcpuXsave {
        vcpu_id: u32,
    },
    VirtioTransport {
        identity: VirtioDeviceIdentity,
    },
    VirtioQueue {
        identity: VirtioDeviceIdentity,
        queue_index: u32,
    },
    VirtioBackend {
        identity: VirtioDeviceIdentity,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotTopology {
    pub vcpu_count: u32,
    /// Canonical KVM MSR capability inventory required by every vCPU.
    pub msr_indices: Vec<u32>,
    pub virtio_devices: Vec<(VirtioDeviceIdentity, u32)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub state_schema_version: u32,
    pub completeness_profile: String,
    pub topology: SnapshotTopology,
    pub inventory: Vec<SnapshotComponent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotPreflightError {
    LegacyOrIncomplete,
    SchemaVersion { actual: u32 },
    Profile { actual: String },
    TopologyMismatch,
    DuplicateDeviceIdentity(VirtioDeviceIdentity),
    InventoryMismatch,
}

pub fn build_snapshot_inventory(topology: &SnapshotTopology) -> Vec<SnapshotComponent> {
    let mut components = vec![
        SnapshotComponent::GuestMemory,
        SnapshotComponent::InKernelIrqChip,
        SnapshotComponent::InKernelPit,
        SnapshotComponent::InKernelClock,
        SnapshotComponent::VmmDeterminism,
        SnapshotComponent::Serial,
        SnapshotComponent::FaultEngine,
        SnapshotComponent::Scheduler,
    ];
    for vcpu_id in 0..topology.vcpu_count {
        components.push(SnapshotComponent::VcpuArchitecture { vcpu_id });
        components.push(SnapshotComponent::VcpuEvents { vcpu_id });
        components.push(SnapshotComponent::VcpuMsrs { vcpu_id });
        components.push(SnapshotComponent::VcpuXsave { vcpu_id });
    }
    for (identity, queue_count) in &topology.virtio_devices {
        components.push(SnapshotComponent::VirtioTransport {
            identity: identity.clone(),
        });
        for queue_index in 0..*queue_count {
            components.push(SnapshotComponent::VirtioQueue {
                identity: identity.clone(),
                queue_index,
            });
        }
        components.push(SnapshotComponent::VirtioBackend {
            identity: identity.clone(),
        });
    }
    components.sort();
    components
}

pub fn validate_snapshot_metadata(
    metadata: Option<&SnapshotMetadata>,
    expected_topology: &SnapshotTopology,
) -> Result<(), SnapshotPreflightError> {
    let metadata = metadata.ok_or(SnapshotPreflightError::LegacyOrIncomplete)?;
    if metadata.state_schema_version != SNAPSHOT_STATE_SCHEMA_VERSION {
        return Err(SnapshotPreflightError::SchemaVersion {
            actual: metadata.state_schema_version,
        });
    }
    if metadata.completeness_profile != SNAPSHOT_PROFILE_EXACT_X86_KVM_V1 {
        return Err(SnapshotPreflightError::Profile {
            actual: metadata.completeness_profile.clone(),
        });
    }
    let mut identities = std::collections::BTreeSet::new();
    for (identity, _) in &metadata.topology.virtio_devices {
        if !identities.insert(identity.clone()) {
            return Err(SnapshotPreflightError::DuplicateDeviceIdentity(
                identity.clone(),
            ));
        }
    }
    if &metadata.topology != expected_topology {
        return Err(SnapshotPreflightError::TopologyMismatch);
    }
    if metadata.inventory != build_snapshot_inventory(expected_topology) {
        return Err(SnapshotPreflightError::InventoryMismatch);
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VirtioBackendSnapshot {
    Block(Box<BlockSnapshot>),
    Net(Box<NetSnapshot>),
    Entropy(Box<EntropySnapshot>),
}

impl VirtioBackendSnapshot {
    pub fn device_id(&self) -> u32 {
        match self {
            Self::Block(_) => VIRTIO_BLOCK_DEVICE_ID,
            Self::Net(_) => VIRTIO_NET_DEVICE_ID,
            Self::Entropy(_) => VIRTIO_ENTROPY_DEVICE_ID,
        }
    }
}

/// Snapshot of a single virtio device's transport and backend state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VirtioDeviceSnapshot {
    pub transport: VirtioMmioSnapshot,
    pub backend: VirtioBackendSnapshot,
}

impl VirtioDeviceSnapshot {
    pub fn identity(&self) -> VirtioDeviceIdentity {
        VirtioDeviceIdentity {
            base_addr: self.transport.base_addr,
            irq: self.transport.irq,
            device_id: self.transport.device_id,
        }
    }
}

/// VMM-side state parameters for snapshot capture.
///
/// Groups the non-KVM state to avoid excessive function arguments.
pub struct CaptureParams {
    pub topology: SnapshotTopology,
    pub serial_state: vm_superio::SerialState,
    pub entropy: EntropySnapshot,
    pub virtual_tsc: u64,
    pub exit_count: u64,
    pub io_exit_count: u64,
    pub exits_since_last_sdk: u64,
    pub panic_detected: bool,
    pub panic_match_state: u64,
    pub pit_snapshot: PitSnapshot,
    pub last_kvm_pit_mode: u8,
    pub fault_engine_snapshot: EngineSnapshot,
    pub virtio_snapshots: Vec<VirtioDeviceSnapshot>,
    pub coverage_active: bool,
    pub scheduler_snapshot: SchedulerSnapshot,
    pub hlt_latched_vcpus: Vec<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MsrSnapshot {
    pub index: u32,
    pub data: u64,
}

/// Per-vCPU architecture state for snapshot/restore.
#[derive(Clone, Debug)]
pub struct VcpuSnapshot {
    pub regs: kvm_regs,
    pub sregs: kvm_sregs,
    pub fpu: kvm_fpu,
    pub debug_regs: kvm_debugregs,
    pub lapic: kvm_lapic_state,
    pub xcrs: kvm_xcrs,
    /// Raw fixed-size KVM XSAVE image. Owned bytes make cloning explicit.
    pub xsave: Vec<u8>,
    pub events: kvm_vcpu_events,
    pub msrs: Vec<MsrSnapshot>,
    /// MP state (RUNNABLE, HALTED, UNINITIALIZED, etc.).
    /// Critical for SMP: without this, KVM doesn't know whether an AP
    /// should be running or waiting for SIPI after restore.
    pub mp_state: kvm_mp_state,
}

/// The x86 shell's register command payload is exactly one native KVM register image.
pub(crate) fn decode_execution_registers(bytes: &[u8]) -> Result<kvm_regs, String> {
    pod_from_bytes(bytes, "command.register_bytes")
}

fn pod_to_bytes<T>(value: &T) -> Vec<u8> {
    let len = std::mem::size_of::<T>();
    let ptr = value as *const T as *const u8;
    // SAFETY: we only copy bytes out of a plain KVM bindings value.
    unsafe { std::slice::from_raw_parts(ptr, len).to_vec() }
}

fn pod_from_bytes<T>(bytes: &[u8], field: &'static str) -> Result<T, String> {
    let len = std::mem::size_of::<T>();
    if bytes.len() != len {
        return Err(format!(
            "{field} byte length mismatch: expected {len}, got {}",
            bytes.len()
        ));
    }
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    // SAFETY: the bytes were emitted from the same concrete KVM bindings type
    // by `pod_to_bytes`; version/schema validation lives at the artifact layer.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), value.as_mut_ptr() as *mut u8, len);
        Ok(value.assume_init())
    }
}

#[derive(Serialize, Deserialize)]
struct VcpuSnapshotSerde {
    regs: Vec<u8>,
    sregs: Vec<u8>,
    fpu: Vec<u8>,
    debug_regs: Vec<u8>,
    lapic: Vec<u8>,
    xcrs: Vec<u8>,
    #[serde(default)]
    xsave: Vec<u8>,
    #[serde(default)]
    events: Vec<u8>,
    #[serde(default)]
    msrs: Vec<MsrSnapshot>,
    mp_state: Vec<u8>,
}

impl From<&VcpuSnapshot> for VcpuSnapshotSerde {
    fn from(snapshot: &VcpuSnapshot) -> Self {
        Self {
            regs: pod_to_bytes(&snapshot.regs),
            sregs: pod_to_bytes(&snapshot.sregs),
            fpu: pod_to_bytes(&snapshot.fpu),
            debug_regs: pod_to_bytes(&snapshot.debug_regs),
            lapic: pod_to_bytes(&snapshot.lapic),
            xcrs: pod_to_bytes(&snapshot.xcrs),
            xsave: snapshot.xsave.clone(),
            events: pod_to_bytes(&snapshot.events),
            msrs: snapshot.msrs.clone(),
            mp_state: pod_to_bytes(&snapshot.mp_state),
        }
    }
}

impl TryFrom<VcpuSnapshotSerde> for VcpuSnapshot {
    type Error = String;

    fn try_from(value: VcpuSnapshotSerde) -> Result<Self, Self::Error> {
        if value.xsave.is_empty() || value.events.is_empty() || value.msrs.is_empty() {
            return Err(
                "legacy vCPU snapshot lacks XSAVE, event, or required MSR state".to_string(),
            );
        }
        Ok(Self {
            regs: pod_from_bytes(&value.regs, "regs")?,
            sregs: pod_from_bytes(&value.sregs, "sregs")?,
            fpu: pod_from_bytes(&value.fpu, "fpu")?,
            debug_regs: pod_from_bytes(&value.debug_regs, "debug_regs")?,
            lapic: pod_from_bytes(&value.lapic, "lapic")?,
            xcrs: pod_from_bytes(&value.xcrs, "xcrs")?,
            xsave: {
                let _: kvm_xsave = pod_from_bytes(&value.xsave, "xsave")?;
                value.xsave
            },
            events: pod_from_bytes(&value.events, "events")?,
            msrs: value.msrs,
            mp_state: pod_from_bytes(&value.mp_state, "mp_state")?,
        })
    }
}

impl Serialize for VcpuSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        VcpuSnapshotSerde::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VcpuSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        VcpuSnapshotSerde::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
struct SerialStateSerde {
    baud_divisor_low: u8,
    baud_divisor_high: u8,
    interrupt_enable: u8,
    interrupt_identification: u8,
    line_control: u8,
    line_status: u8,
    modem_control: u8,
    modem_status: u8,
    scratch: u8,
    in_buffer: Vec<u8>,
}

impl From<&vm_superio::SerialState> for SerialStateSerde {
    fn from(state: &vm_superio::SerialState) -> Self {
        Self {
            baud_divisor_low: state.baud_divisor_low,
            baud_divisor_high: state.baud_divisor_high,
            interrupt_enable: state.interrupt_enable,
            interrupt_identification: state.interrupt_identification,
            line_control: state.line_control,
            line_status: state.line_status,
            modem_control: state.modem_control,
            modem_status: state.modem_status,
            scratch: state.scratch,
            in_buffer: state.in_buffer.clone(),
        }
    }
}

impl From<SerialStateSerde> for vm_superio::SerialState {
    fn from(state: SerialStateSerde) -> Self {
        Self {
            baud_divisor_low: state.baud_divisor_low,
            baud_divisor_high: state.baud_divisor_high,
            interrupt_enable: state.interrupt_enable,
            interrupt_identification: state.interrupt_identification,
            line_control: state.line_control,
            line_status: state.line_status,
            modem_control: state.modem_control,
            modem_status: state.modem_status,
            scratch: state.scratch,
            in_buffer: state.in_buffer,
        }
    }
}

fn deserialize_bounded_hlt_latches<'de, D>(deserializer: D) -> Result<Vec<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct HltLatchVisitor;

    impl<'de> serde::de::Visitor<'de> for HltLatchVisitor {
        type Value = Vec<bool>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                formatter,
                "at most {MAX_SCHEDULE_VCPUS} vCPU HLT latch values"
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let hinted = sequence.size_hint().unwrap_or(0);
            if hinted > MAX_SCHEDULE_VCPUS {
                return Err(serde::de::Error::custom(
                    "snapshot HLT latch bound exceeded",
                ));
            }
            let mut latches = Vec::with_capacity(hinted);
            while let Some(latched) = sequence.next_element()? {
                if latches.len() >= MAX_SCHEDULE_VCPUS {
                    return Err(serde::de::Error::custom(
                        "snapshot HLT latch bound exceeded",
                    ));
                }
                latches.push(latched);
            }
            Ok(latches)
        }
    }

    deserializer.deserialize_seq(HltLatchVisitor)
}

#[derive(Serialize, Deserialize)]
struct VmSnapshotSerde {
    #[serde(default)]
    metadata: Option<SnapshotMetadata>,
    vcpu_snapshots: Vec<VcpuSnapshot>,
    pic_master: Vec<u8>,
    pic_slave: Vec<u8>,
    ioapic: Vec<u8>,
    pit: Vec<u8>,
    clock: Vec<u8>,
    memory: SnapshotMemory,
    serial_state: SerialStateSerde,
    entropy: EntropySnapshot,
    virtual_tsc: u64,
    exit_count: u64,
    io_exit_count: u64,
    exits_since_last_sdk: u64,
    #[serde(default)]
    panic_detected: bool,
    #[serde(default)]
    panic_match_state: u64,
    pit_snapshot: PitSnapshot,
    last_kvm_pit_mode: u8,
    fault_engine_snapshot: EngineSnapshot,
    virtio_snapshots: Vec<VirtioDeviceSnapshot>,
    coverage_active: bool,
    scheduler_snapshot: SchedulerSnapshot,
    #[serde(default, deserialize_with = "deserialize_bounded_hlt_latches")]
    hlt_latched_vcpus: Vec<bool>,
}

impl From<&VmSnapshot> for VmSnapshotSerde {
    fn from(snapshot: &VmSnapshot) -> Self {
        Self {
            metadata: snapshot.metadata.clone(),
            vcpu_snapshots: snapshot.vcpu_snapshots.clone(),
            pic_master: pod_to_bytes(&snapshot.pic_master),
            pic_slave: pod_to_bytes(&snapshot.pic_slave),
            ioapic: pod_to_bytes(&snapshot.ioapic),
            pit: pod_to_bytes(&snapshot.pit),
            clock: pod_to_bytes(&snapshot.clock),
            memory: snapshot.memory.clone(),
            serial_state: SerialStateSerde::from(&snapshot.serial_state),
            entropy: snapshot.entropy.clone(),
            virtual_tsc: snapshot.virtual_tsc,
            exit_count: snapshot.exit_count,
            io_exit_count: snapshot.io_exit_count,
            exits_since_last_sdk: snapshot.exits_since_last_sdk,
            panic_detected: snapshot.panic_detected,
            panic_match_state: snapshot.panic_match_state,
            pit_snapshot: snapshot.pit_snapshot.clone(),
            last_kvm_pit_mode: snapshot.last_kvm_pit_mode,
            fault_engine_snapshot: snapshot.fault_engine_snapshot.clone(),
            virtio_snapshots: snapshot.virtio_snapshots.clone(),
            coverage_active: snapshot.coverage_active,
            scheduler_snapshot: snapshot.scheduler_snapshot.clone(),
            hlt_latched_vcpus: snapshot.hlt_latched_vcpus.clone(),
        }
    }
}

impl TryFrom<VmSnapshotSerde> for VmSnapshot {
    type Error = String;

    fn try_from(value: VmSnapshotSerde) -> Result<Self, Self::Error> {
        Ok(Self {
            metadata: value.metadata,
            vcpu_snapshots: value.vcpu_snapshots,
            pic_master: pod_from_bytes(&value.pic_master, "pic_master")?,
            pic_slave: pod_from_bytes(&value.pic_slave, "pic_slave")?,
            ioapic: pod_from_bytes(&value.ioapic, "ioapic")?,
            pit: pod_from_bytes(&value.pit, "pit")?,
            clock: pod_from_bytes(&value.clock, "clock")?,
            memory: value.memory,
            serial_state: value.serial_state.into(),
            entropy: value.entropy,
            virtual_tsc: value.virtual_tsc,
            exit_count: value.exit_count,
            io_exit_count: value.io_exit_count,
            exits_since_last_sdk: value.exits_since_last_sdk,
            panic_detected: value.panic_detected,
            panic_match_state: value.panic_match_state,
            pit_snapshot: value.pit_snapshot,
            last_kvm_pit_mode: value.last_kvm_pit_mode,
            fault_engine_snapshot: value.fault_engine_snapshot,
            virtio_snapshots: value.virtio_snapshots,
            coverage_active: value.coverage_active,
            scheduler_snapshot: value.scheduler_snapshot,
            hlt_latched_vcpus: value.hlt_latched_vcpus,
        })
    }
}

impl Serialize for VmSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        VmSnapshotSerde::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VmSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        VmSnapshotSerde::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}

impl VcpuSnapshot {
    /// Capture all architecture state from a single vCPU.
    pub fn capture(vcpu: &VcpuFd, msr_indices: &[u32]) -> Result<Self, SnapshotError> {
        if msr_indices.is_empty() || msr_indices.windows(2).any(|pair| pair[0] >= pair[1]) {
            return MsrInventorySnafu.fail();
        }
        let requested = msr_indices
            .iter()
            .map(|index| kvm_msr_entry {
                index: *index,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let mut msrs = Msrs::from_entries(&requested).map_err(|_| MsrBufferSnafu.build())?;
        let read = vcpu.get_msrs(&mut msrs).context(GetMsrsSnafu)?;
        if read != msr_indices.len() {
            return MsrCountSnafu {
                phase: "capture",
                expected: msr_indices.len(),
                actual: read,
            }
            .fail();
        }
        let msrs = msrs
            .as_slice()
            .iter()
            .map(|entry| MsrSnapshot {
                index: entry.index,
                data: entry.data,
            })
            .collect();
        Ok(Self {
            regs: vcpu.get_regs().context(GetRegsSnafu)?,
            sregs: vcpu.get_sregs().context(GetSregsSnafu)?,
            fpu: vcpu.get_fpu().context(GetFpuSnafu)?,
            debug_regs: vcpu.get_debug_regs().context(GetDebugRegsSnafu)?,
            lapic: vcpu.get_lapic().context(GetLapicSnafu)?,
            xcrs: vcpu.get_xcrs().context(GetXcrsSnafu)?,
            xsave: pod_to_bytes(&vcpu.get_xsave().context(GetXsaveSnafu)?),
            events: vcpu.get_vcpu_events().context(GetVcpuEventsSnafu)?,
            msrs,
            mp_state: vcpu.get_mp_state().context(GetMpStateSnafu)?,
        })
    }

    pub fn validate(&self) -> Result<(), SnapshotError> {
        let _: kvm_xsave =
            pod_from_bytes(&self.xsave, "xsave").map_err(|_| XsaveImageSnafu.build())?;
        // This profile selects the standard (not compacted) KVM XSAVE image.
        // XCOMP_BV and the rest of the header's reserved bytes must be zero.
        if self.xsave[520..576].iter().any(|byte| *byte != 0) {
            return ArchitectureFieldSnafu {
                field: "xsave.header",
            }
            .fail();
        }
        let xsave_mxcsr = u32::from_le_bytes([
            self.xsave[24],
            self.xsave[25],
            self.xsave[26],
            self.xsave[27],
        ]);
        if self.fpu.mxcsr & !0xffff != 0 || xsave_mxcsr & !0xffff != 0 {
            return ArchitectureFieldSnafu { field: "fpu.mxcsr" }.fail();
        }
        if self.sregs.cr8 > 15 {
            return ArchitectureFieldSnafu { field: "sregs.cr8" }.fail();
        }
        if self.debug_regs.flags != 0 {
            return ArchitectureFieldSnafu {
                field: "debug_regs.flags",
            }
            .fail();
        }
        // RFLAGS bit 1 is fixed-one; bits 3, 5, 15 and 22..63 are reserved-zero.
        const ARCHITECTURAL_RFLAGS: u64 = 0x003f_7fd7;
        if self.regs.rflags & 2 == 0 || self.regs.rflags & !ARCHITECTURAL_RFLAGS != 0 {
            return ArchitectureFieldSnafu {
                field: "regs.rflags",
            }
            .fail();
        }
        // x86 MP states: runnable, uninitialized, INIT received, halted,
        // SIPI received, and AP reset hold. Other architectures' states are invalid.
        if !matches!(self.mp_state.mp_state, 0..=4 | 9) {
            return ArchitectureFieldSnafu { field: "mp_state" }.fail();
        }
        for segment in [
            &self.sregs.cs,
            &self.sregs.ds,
            &self.sregs.es,
            &self.sregs.fs,
            &self.sregs.gs,
            &self.sregs.ss,
            &self.sregs.tr,
            &self.sregs.ldt,
        ] {
            if segment.type_ > 15
                || segment.dpl > 3
                || [
                    segment.present,
                    segment.db,
                    segment.s,
                    segment.l,
                    segment.g,
                    segment.avl,
                    segment.unusable,
                ]
                .iter()
                .any(|value| *value > 1)
            {
                return ArchitectureFieldSnafu {
                    field: "sregs.segment",
                }
                .fail();
            }
        }
        if self.xcrs.nr_xcrs as usize > self.xcrs.xcrs.len() || self.xcrs.flags != 0 {
            return ArchitectureFieldSnafu {
                field: "xcrs.inventory",
            }
            .fail();
        }
        let mut previous_xcr = None;
        for xcr in &self.xcrs.xcrs[..self.xcrs.nr_xcrs as usize] {
            if xcr.xcr != 0
                || xcr.reserved != 0
                || previous_xcr == Some(xcr.xcr)
                || xcr.value & 1 == 0
                || (xcr.value & 4 != 0 && xcr.value & 2 == 0)
            {
                return ArchitectureFieldSnafu { field: "xcrs.xcr0" }.fail();
            }
            previous_xcr = Some(xcr.xcr);
        }
        // ABI validity bits: NMI pending, SIPI vector, interrupt shadow,
        // SMM, exception payload, and triple fault. No unknown flag is admitted.
        const EVENT_VALID_FLAGS: u32 = kvm_bindings::KVM_VCPUEVENT_VALID_NMI_PENDING
            | kvm_bindings::KVM_VCPUEVENT_VALID_SIPI_VECTOR
            | kvm_bindings::KVM_VCPUEVENT_VALID_SHADOW
            | kvm_bindings::KVM_VCPUEVENT_VALID_SMM
            | kvm_bindings::KVM_VCPUEVENT_VALID_PAYLOAD
            | kvm_bindings::KVM_VCPUEVENT_VALID_TRIPLE_FAULT;
        let events = &self.events;
        if [
            events.exception.injected,
            events.exception.has_error_code,
            events.exception.pending,
            events.interrupt.injected,
            events.interrupt.soft,
            events.nmi.injected,
            events.nmi.masked,
            events.smi.smm,
            events.smi.pending,
            events.smi.smm_inside_nmi,
            events.smi.latched_init,
            events.exception_has_payload,
            events.triple_fault.pending,
        ]
        .iter()
        .any(|value| *value > 1)
            || events.exception.nr > 31
            || events.interrupt.shadow & !3 != 0
            // Pending NMIs are a count, not a boolean; unmasked x86 permits two.
            || events.nmi.pending > 2
            || events.sipi_vector > 255
            || events.flags & !EVENT_VALID_FLAGS != 0
        {
            return ArchitectureFieldSnafu {
                field: "vcpu_events",
            }
            .fail();
        }
        if self.msrs.is_empty()
            || self
                .msrs
                .windows(2)
                .any(|pair| pair[0].index >= pair[1].index)
        {
            return MsrInventorySnafu.fail();
        }
        Ok(())
    }

    pub fn validate_msr_inventory(&self, expected: &[u32]) -> Result<(), SnapshotError> {
        self.validate()?;
        if self.msrs.len() != expected.len()
            || self
                .msrs
                .iter()
                .zip(expected)
                .any(|(entry, expected_index)| entry.index != *expected_index)
        {
            return MsrInventorySnafu.fail();
        }
        Ok(())
    }

    /// Restore all architecture state to a single vCPU.
    pub fn restore(&self, vcpu: &VcpuFd) -> Result<(), SnapshotError> {
        self.validate()?;
        let entries = self
            .msrs
            .iter()
            .map(|entry| kvm_msr_entry {
                index: entry.index,
                data: entry.data,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let msrs = Msrs::from_entries(&entries).map_err(|_| MsrBufferSnafu.build())?;
        // MP state MUST be set before registers — KVM refuses register
        // writes on vCPUs in UNINITIALIZED state on some host kernels.
        vcpu.set_mp_state(self.mp_state).context(SetMpStateSnafu)?;
        vcpu.set_sregs(&self.sregs).context(SetSregsSnafu)?;
        vcpu.set_regs(&self.regs).context(SetRegsSnafu)?;
        vcpu.set_xcrs(&self.xcrs).context(SetXcrsSnafu)?;
        vcpu.set_fpu(&self.fpu).context(SetFpuSnafu)?;
        let xsave: kvm_xsave =
            pod_from_bytes(&self.xsave, "xsave").map_err(|_| XsaveImageSnafu.build())?;
        vcpu.set_xsave(&xsave).context(SetXsaveSnafu)?;
        vcpu.set_debug_regs(&self.debug_regs)
            .context(SetDebugRegsSnafu)?;
        vcpu.set_lapic(&self.lapic).context(SetLapicSnafu)?;
        let written = vcpu.set_msrs(&msrs).context(SetMsrsSnafu)?;
        if written != entries.len() {
            return MsrCountSnafu {
                phase: "restore",
                expected: entries.len(),
                actual: written,
            }
            .fail();
        }
        vcpu.set_vcpu_events(&self.events)
            .context(SetVcpuEventsSnafu)?;
        Ok(())
    }
}

/// Complete VM state — everything needed to restore a VM to an exact point.
#[derive(Clone, Debug)]
pub struct VmSnapshot {
    /// Internal fidelity schema and exact component inventory.
    pub metadata: Option<SnapshotMetadata>,

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
    pub panic_detected: bool,
    pub panic_match_state: u64,

    // DeterministicPit state
    pub pit_snapshot: PitSnapshot,
    pub last_kvm_pit_mode: u8,

    // Fault engine state
    pub fault_engine_snapshot: EngineSnapshot,

    // Virtio device state
    pub virtio_snapshots: Vec<VirtioDeviceSnapshot>,
    pub coverage_active: bool,

    /// Complete vCPU scheduler and exact-progress state.
    pub scheduler_snapshot: SchedulerSnapshot,
    /// Userspace HLT latches required because KVM leaves MP state runnable.
    pub hlt_latched_vcpus: Vec<bool>,
}

impl VmSnapshot {
    /// Validate retained architectural values, never consulting live KVM state.
    pub(crate) fn validate_kernel_state(&self) -> Result<(), SnapshotError> {
        for (chip, identity, field) in [
            (&self.pic_master, KVM_IRQCHIP_PIC_MASTER, "pic_master"),
            (&self.pic_slave, KVM_IRQCHIP_PIC_SLAVE, "pic_slave"),
        ] {
            if chip.chip_id != identity {
                return ArchitectureFieldSnafu { field }.fail();
            }
            // SAFETY: the checked stable chip identity selects the PIC ABI arm.
            let pic = unsafe { chip.chip.pic };
            if pic.priority_add > 7
                || pic.irq_base & 7 != 0
                || pic.init_state > 3
                || [
                    pic.read_reg_select,
                    pic.poll,
                    pic.special_mask,
                    pic.auto_eoi,
                    pic.rotate_on_auto_eoi,
                    pic.special_fully_nested_mode,
                    pic.init4,
                ]
                .iter()
                .any(|value| *value > 1)
            {
                return ArchitectureFieldSnafu { field }.fail();
            }
        }
        if self.ioapic.chip_id != KVM_IRQCHIP_IOAPIC {
            return ArchitectureFieldSnafu { field: "ioapic" }.fail();
        }
        // SAFETY: the checked stable chip identity selects the IOAPIC ABI arm.
        let ioapic = unsafe { self.ioapic.chip.ioapic };
        if ioapic.base_address & 0xfff != 0
            || ioapic.ioregsel > 255
            || ioapic.id > 15
            || ioapic.irr >> ioapic.redirtbl.len() != 0
        {
            return ArchitectureFieldSnafu { field: "ioapic" }.fail();
        }
        // KVM's PIT read/write cursor has four states (LSB, MSB, WORD0,
        // WORD1), plus the reset/unlatched zero state. WORD1 is not a bool.
        for channel in &self.pit.channels {
            if channel.count > 65_536
                || channel.count_latched > 3
                || channel.status_latched > 1
                || channel.read_state > 4
                || channel.write_state > 4
                || channel.rw_mode > 3
                // KVM normalizes modes 6/7 to 2/3; 0xff is its reset sentinel.
                || (channel.mode > 5 && channel.mode != u8::MAX)
                || channel.bcd > 1
                || channel.gate > 1
            {
                return ArchitectureFieldSnafu {
                    field: "pit.channel",
                }
                .fail();
            }
        }
        if self.pit.flags
            & !(kvm_bindings::KVM_PIT_FLAGS_HPET_LEGACY
                | kvm_bindings::KVM_PIT_FLAGS_SPEAKER_DATA_ON)
            != 0
            || self.pit.reserved.iter().any(|value| *value != 0)
        {
            return ArchitectureFieldSnafu { field: "pit.flags" }.fail();
        }
        if self.clock.flags
            & !(kvm_bindings::KVM_CLOCK_TSC_STABLE
                | kvm_bindings::KVM_CLOCK_REALTIME
                | kvm_bindings::KVM_CLOCK_HOST_TSC)
            != 0
            || self.clock.pad0 != 0
            || self.clock.pad.iter().any(|value| *value != 0)
        {
            return ArchitectureFieldSnafu { field: "clock" }.fail();
        }
        Ok(())
    }

    pub fn validate_assertion_identity(
        &self,
    ) -> Result<(), chaoscontrol_fault::oracle_validation::OracleValidationError> {
        chaoscontrol_fault::engine::validate_engine_snapshot(&self.fault_engine_snapshot)
    }

    pub fn assertion_validation_diagnostic(&self) -> String {
        chaoscontrol_fault::engine::engine_snapshot_validation_diagnostic(
            &self.fault_engine_snapshot,
        )
    }

    pub fn validate_assertion_evidence(
        &self,
        identity: &chaoscontrol_protocol::admission::AssertionEvidenceIdentity,
    ) -> Result<(), chaoscontrol_fault::oracle_validation::OracleValidationError> {
        chaoscontrol_fault::engine::validate_engine_snapshot_assertion_evidence(
            &self.fault_engine_snapshot,
            identity,
        )
    }

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
        let vcpu_count = u32::try_from(vcpus.len()).map_err(|_| SnapshotError::Topology {
            message: "vCPU count exceeds snapshot schema width".to_string(),
        })?;
        if params.topology.vcpu_count != vcpu_count {
            return Err(SnapshotError::Topology {
                message: "capture topology has the wrong vCPU count".to_string(),
            });
        }

        // Capture per-vCPU state
        let mut vcpu_snapshots = Vec::with_capacity(vcpus.len());
        for vcpu in vcpus {
            vcpu_snapshots.push(VcpuSnapshot::capture(vcpu, &params.topology.msr_indices)?);
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

        let mut devices = Vec::with_capacity(params.virtio_snapshots.len());
        for snapshot in &params.virtio_snapshots {
            let queue_count = u32::try_from(snapshot.transport.queues.len()).map_err(|_| {
                SnapshotError::Topology {
                    message: "virtio queue count exceeds snapshot schema width".to_string(),
                }
            })?;
            devices.push((snapshot.identity(), queue_count));
        }
        devices.sort();
        if devices != params.topology.virtio_devices {
            return Err(SnapshotError::Topology {
                message: "capture virtio state conflicts with declared topology".to_string(),
            });
        }
        let metadata = SnapshotMetadata {
            state_schema_version: SNAPSHOT_STATE_SCHEMA_VERSION,
            completeness_profile: SNAPSHOT_PROFILE_EXACT_X86_KVM_V1.to_string(),
            inventory: build_snapshot_inventory(&params.topology),
            topology: params.topology,
        };

        Ok(Self {
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
        })
    }

    fn validate_kernel_restore(&self, vcpu_count: usize) -> Result<(), SnapshotError> {
        if vcpu_count != self.vcpu_snapshots.len() {
            return VcpuCountMismatchSnafu {
                snapshot: self.vcpu_snapshots.len(),
                current: vcpu_count,
            }
            .fail();
        }
        let metadata = self
            .metadata
            .as_ref()
            .ok_or_else(|| SnapshotError::Topology {
                message: "exact kernel restore requires completeness metadata".to_string(),
            })?;
        validate_snapshot_metadata(Some(metadata), &metadata.topology).map_err(|error| {
            SnapshotError::Topology {
                message: format!("snapshot completeness preflight failed: {error:?}"),
            }
        })?;
        if metadata.topology.vcpu_count as usize != vcpu_count {
            return Err(SnapshotError::Topology {
                message: "snapshot architecture inventory has the wrong vCPU count".to_string(),
            });
        }
        for vcpu in &self.vcpu_snapshots {
            vcpu.validate_msr_inventory(&metadata.topology.msr_indices)?;
        }
        self.validate_kernel_state()
    }

    /// Restore VM state from this snapshot.
    pub fn restore(
        &self,
        vcpus: &[VcpuFd],
        vm: &VmFd,
        guest_memory: &GuestMemoryMmap,
    ) -> Result<(), SnapshotError> {
        self.validate_kernel_restore(vcpus.len())?;
        self.memory
            .validate_for_guest_size(guest_memory.last_addr().raw_value() as usize + 1)
            .map_err(|message| SnapshotError::Topology { message })?;

        if let SnapshotMemory::Overlay { base, .. } = &self.memory {
            guest_memory
                .write_slice(base, GuestAddress(0))
                .map_err(|_| WriteMemorySnafu.build())?;
        }
        // Full restore establishes the base as well as the overlay pages.
        self.memory.write_to_guest(guest_memory)?;

        // Restore KVM devices and vCPU registers
        self.restore_kernel_state_validated(vcpus, vm)?;

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
        self.validate_kernel_restore(vcpus.len())?;
        self.restore_kernel_state_validated(vcpus, vm)
    }

    fn restore_kernel_state_validated(
        &self,
        vcpus: &[VcpuFd],
        vm: &VmFd,
    ) -> Result<(), SnapshotError> {
        // Restore in-kernel devices BEFORE vCPU state
        // Older snapshots may lack the host-timer inhibit flag. Set it in the
        // restored KVM state without changing the serialized snapshot identity.
        let mut pit = self.pit;
        pit.flags |= kvm_bindings::KVM_PIT_FLAGS_HPET_LEGACY;
        vm.set_pit2(&pit).context(SetPitSnafu)?;
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
    #[snafu(display("Failed to get vCPU events"))]
    GetVcpuEvents { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get XSAVE state"))]
    GetXsave { source: kvm_ioctls::Error },
    #[snafu(display("Failed to get required MSRs"))]
    GetMsrs { source: kvm_ioctls::Error },
    #[snafu(display("Failed to allocate the required MSR buffer"))]
    MsrBuffer,
    #[snafu(display("Required MSR inventory is incomplete or reordered"))]
    MsrInventory,
    #[snafu(display("Invalid architecture field: {field}"))]
    ArchitectureField { field: &'static str },
    #[snafu(display("Required MSR {phase} count mismatch: expected {expected}, got {actual}"))]
    MsrCount {
        phase: &'static str,
        expected: usize,
        actual: usize,
    },
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
    #[snafu(display("Failed to set vCPU events"))]
    SetVcpuEvents { source: kvm_ioctls::Error },
    #[snafu(display("Failed to set XSAVE state"))]
    SetXsave { source: kvm_ioctls::Error },
    #[snafu(display("Invalid fixed-size XSAVE image"))]
    XsaveImage,
    #[snafu(display("Failed to set required MSRs"))]
    SetMsrs { source: kvm_ioctls::Error },
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
    #[snafu(display("Invalid snapshot topology: {message}"))]
    Topology { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MSR_INDICES: [u32; 2] = [0x10, 0x1B];

    fn make_base(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    fn decode_hlt_latches(values: &[bool]) -> Result<Vec<bool>, serde_json::Error> {
        let encoded = serde_json::to_string(values)?;
        let mut deserializer = serde_json::Deserializer::from_str(&encoded);
        deserialize_bounded_hlt_latches(&mut deserializer)
    }

    #[test]
    fn hlt_latch_decode_accepts_the_public_vcpu_bound() {
        let values = vec![false; MAX_SCHEDULE_VCPUS];
        assert_eq!(decode_hlt_latches(&values).unwrap(), values);
    }

    #[test]
    fn hlt_latch_decode_rejects_before_allocating_beyond_the_public_bound() {
        const EXCESS_LATCH_COUNT: usize = MAX_SCHEDULE_VCPUS + 1;
        let values = vec![false; EXCESS_LATCH_COUNT];
        let error = decode_hlt_latches(&values).unwrap_err();
        assert!(error.to_string().contains("HLT latch bound exceeded"));
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

    fn test_topology() -> SnapshotTopology {
        SnapshotTopology {
            vcpu_count: 2,
            msr_indices: TEST_MSR_INDICES.to_vec(),
            virtio_devices: vec![(
                VirtioDeviceIdentity {
                    base_addr: 0xD000_0000,
                    irq: 5,
                    device_id: VIRTIO_BLOCK_DEVICE_ID,
                },
                1,
            )],
        }
    }

    fn exact_metadata(topology: &SnapshotTopology) -> SnapshotMetadata {
        SnapshotMetadata {
            state_schema_version: SNAPSHOT_STATE_SCHEMA_VERSION,
            completeness_profile: SNAPSHOT_PROFILE_EXACT_X86_KVM_V1.to_string(),
            topology: topology.clone(),
            inventory: build_snapshot_inventory(topology),
        }
    }

    #[test]
    fn exact_metadata_accepts_complete_matching_inventory() {
        let topology = test_topology();
        let metadata = exact_metadata(&topology);
        assert_eq!(
            validate_snapshot_metadata(Some(&metadata), &topology),
            Ok(())
        );
    }

    #[test]
    fn exact_metadata_rejects_legacy_and_missing_components() {
        let topology = test_topology();
        assert_eq!(
            validate_snapshot_metadata(None, &topology),
            Err(SnapshotPreflightError::LegacyOrIncomplete)
        );

        let mut metadata = exact_metadata(&topology);
        metadata.inventory.pop();
        assert_eq!(
            validate_snapshot_metadata(Some(&metadata), &topology),
            Err(SnapshotPreflightError::InventoryMismatch)
        );
    }

    #[test]
    fn exact_metadata_rejects_topology_mismatch_and_duplicate_identity() {
        let topology = test_topology();
        let mut wrong_topology = topology.clone();
        wrong_topology.vcpu_count = 1;
        let metadata = exact_metadata(&wrong_topology);
        assert_eq!(
            validate_snapshot_metadata(Some(&metadata), &topology),
            Err(SnapshotPreflightError::TopologyMismatch)
        );

        let mut duplicate_topology = topology.clone();
        duplicate_topology
            .virtio_devices
            .push(duplicate_topology.virtio_devices[0].clone());
        let metadata = exact_metadata(&duplicate_topology);
        assert!(matches!(
            validate_snapshot_metadata(Some(&metadata), &duplicate_topology),
            Err(SnapshotPreflightError::DuplicateDeviceIdentity(_))
        ));
    }

    fn complete_vcpu_snapshot() -> VcpuSnapshot {
        VcpuSnapshot {
            regs: kvm_regs {
                rflags: 2,
                ..Default::default()
            },
            sregs: Default::default(),
            fpu: Default::default(),
            debug_regs: Default::default(),
            lapic: Default::default(),
            xcrs: Default::default(),
            xsave: pod_to_bytes(&kvm_xsave::default()),
            events: Default::default(),
            msrs: TEST_MSR_INDICES
                .iter()
                .map(|index| MsrSnapshot {
                    index: *index,
                    data: 0,
                })
                .collect(),
            mp_state: Default::default(),
        }
    }

    #[test]
    fn vcpu_snapshot_rejects_missing_required_msr() {
        let mut snapshot = complete_vcpu_snapshot();
        assert!(snapshot.validate_msr_inventory(&TEST_MSR_INDICES).is_ok());
        snapshot.msrs.pop();
        assert!(matches!(
            snapshot.validate_msr_inventory(&TEST_MSR_INDICES),
            Err(SnapshotError::MsrInventory)
        ));

        let mut malformed_xsave = complete_vcpu_snapshot();
        malformed_xsave.xsave.pop();
        assert!(matches!(
            malformed_xsave.validate(),
            Err(SnapshotError::XsaveImage)
        ));
    }

    #[test]
    fn vcpu_snapshot_serialization_round_trips_and_rejects_truncation() {
        let mut snapshot = complete_vcpu_snapshot();
        snapshot.regs.rax = 42;
        snapshot.regs.rip = 0x200;
        snapshot.regs.rflags = 2;
        snapshot.sregs.cs.base = 0x1000;
        snapshot.sregs.cs.limit = 0xffff;
        snapshot.sregs.cs.present = 1;
        snapshot.fpu.mxcsr = 0x1f80;
        snapshot.fpu.xmm[3] = [0xa5; 16];
        snapshot.debug_regs.dr7 = 0x400;
        snapshot.lapic.regs[0x80] = 0x10;
        snapshot.xcrs.nr_xcrs = 1;
        snapshot.xcrs.xcrs[0].value = 3;
        snapshot.xsave[512] = 3;
        snapshot.events.flags = kvm_bindings::KVM_VCPUEVENT_VALID_NMI_PENDING;
        snapshot.events.nmi.pending = 1;
        snapshot.msrs[0].data = 42;
        snapshot.mp_state.mp_state = kvm_bindings::KVM_MP_STATE_HALTED;
        snapshot.validate().unwrap();
        let mut encoded = serde_json::to_vec(&snapshot).unwrap();
        let decoded: VcpuSnapshot = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            serde_json::to_value(&decoded).unwrap(),
            serde_json::to_value(&snapshot).unwrap()
        );
        encoded.pop();
        assert!(serde_json::from_slice::<VcpuSnapshot>(&encoded).is_err());
    }

    #[test]
    fn architecture_enum_range_inventory_rejects_invalid_facts() {
        let cases: &[fn(&mut VcpuSnapshot)] = &[
            |state| state.mp_state.mp_state = u32::MAX,
            |state| state.sregs.cs.type_ = 16,
            |state| state.sregs.ds.dpl = 4,
            |state| state.sregs.ss.present = 2,
            |state| state.sregs.cr8 = 16,
            |state| state.debug_regs.flags = 1,
            |state| state.regs.rflags = 0,
            |state| state.regs.rflags |= 1 << 3,
            |state| state.fpu.mxcsr = 1 << 16,
            |state| state.xsave[26] = 1,
            |state| state.xsave[520] = 1,
            |state| state.xsave[528] = 1,
            |state| state.xcrs.nr_xcrs = 17,
            |state| state.xcrs.flags = 1,
            |state| {
                state.xcrs.nr_xcrs = 1;
                state.xcrs.xcrs[0].xcr = 1;
            },
            |state| {
                state.xcrs.nr_xcrs = 1;
                state.xcrs.xcrs[0].value = 5;
            },
            |state| state.events.flags = 1 << 31,
            |state| state.events.nmi.pending = 3,
            |state| state.events.exception.nr = 32,
            |state| state.events.interrupt.shadow = 4,
            |state| state.events.sipi_vector = 256,
            |state| state.events.triple_fault.pending = 2,
        ];
        for mutate in cases {
            let mut state = complete_vcpu_snapshot();
            mutate(&mut state);
            assert!(matches!(
                state.validate(),
                Err(SnapshotError::ArchitectureField { .. })
            ));
        }
    }

    #[test]
    fn retained_snapshot_reads_cross_dirty_page_boundaries_without_materialization() {
        let base = Arc::new(vec![0x11; PAGE_SIZE * 2]);
        let mut dirty_pages = BTreeMap::new();
        dirty_pages.insert(1, Box::new([0x22; PAGE_SIZE]));
        let memory = SnapshotMemory::Overlay { base, dirty_pages };
        assert_eq!(memory.read_u16((PAGE_SIZE - 1) as u64), Some(0x2211));
        assert_eq!(memory.read_u16((PAGE_SIZE * 2 - 1) as u64), None);
        assert_eq!(memory.read_u16(u64::MAX), None);
    }

    #[test]
    fn legacy_vcpu_snapshot_is_rejected_with_an_explicit_completeness_error() {
        let snapshot = complete_vcpu_snapshot();
        let mut value = serde_json::to_value(&snapshot).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("xsave");
        object.remove("events");
        object.remove("msrs");

        let error = serde_json::from_value::<VcpuSnapshot>(value).unwrap_err();
        assert!(error
            .to_string()
            .contains("legacy vCPU snapshot lacks XSAVE, event, or required MSR state"));
    }
}
