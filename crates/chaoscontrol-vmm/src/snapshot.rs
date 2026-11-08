//! VM snapshot and restore — capture complete VM state and recreate it.

use crate::devices::block::BlockSnapshot;
use crate::devices::entropy::EntropySnapshot;
use crate::devices::net::NetSnapshot;
use crate::devices::pit::PitSnapshot;
use crate::devices::virtio_mmio::VirtioMmioSnapshot;
use crate::scheduler::core::{BoundaryProfileId, ProgressBudget, ProgressMode, MAX_SCHEDULE_VCPUS};
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

pub const SNAPSHOT_STATE_SCHEMA_VERSION: u32 = 4;
pub const SNAPSHOT_PROFILE_EXACT_X86_KVM_V2: &str = "exact-x86-kvm-v2";
pub const SNAPSHOT_EXECUTION_SCHEMA_VERSION: u16 = 2;

const SNAPSHOT_KVM_CAPABILITIES: [u32; 11] = [
    kvm_bindings::KVM_CAP_IRQCHIP,
    kvm_bindings::KVM_CAP_PIT2,
    kvm_bindings::KVM_CAP_ADJUST_CLOCK,
    kvm_bindings::KVM_CAP_VCPU_EVENTS,
    kvm_bindings::KVM_CAP_SET_GUEST_DEBUG,
    kvm_bindings::KVM_CAP_IMMEDIATE_EXIT,
    kvm_bindings::KVM_CAP_XSAVE,
    kvm_bindings::KVM_CAP_XCRS,
    kvm_bindings::KVM_CAP_TSC_CONTROL,
    kvm_bindings::KVM_CAP_GET_TSC_KHZ,
    kvm_bindings::KVM_CAP_X86_MSR_FILTER,
];
const SNAPSHOT_NATIVE_CPUID_LEAVES: [u32; 6] = [0, 1, 7, 0x8000_0000, 0x8000_0001, 0x8000_0007];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotMsrFilterProfile {
    KvmCoarseClockDenyV1,
}

impl SnapshotMsrFilterProfile {
    pub fn schema_version(self) -> u16 {
        1
    }

    /// Default-allow filter; a zero bitmap denies the named access flags.
    /// The backend installs this exact table before any coarse guest entry.
    pub fn ranges(self) -> &'static [(u32, u32, u32)] {
        const RW: u32 = kvm_bindings::KVM_MSR_FILTER_READ | kvm_bindings::KVM_MSR_FILTER_WRITE;
        const W: u32 = kvm_bindings::KVM_MSR_FILTER_WRITE;
        &[
            (0x10, 1, RW),
            (0x11, 2, RW),
            (0x3b, 1, RW),
            (0xe7, 2, RW),
            (0x6e0, 1, RW),
            (0x832, 1, W),
            (0x838, 1, W),
            (0x4000_0000, 0x1000, RW),
            (0x4b56_4d00, 0x100, RW),
        ]
    }

    pub fn table_identity(self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"chaoscontrol.kvm-coarse-clock-msr-filter.v1");
        hasher.update(&self.schema_version().to_le_bytes());
        hasher.update(&kvm_bindings::KVM_MSR_FILTER_DEFAULT_ALLOW.to_le_bytes());
        hasher.update(&(self.ranges().len() as u32).to_le_bytes());
        for (base, count, flags) in self.ranges() {
            hasher.update(&base.to_le_bytes());
            hasher.update(&count.to_le_bytes());
            hasher.update(&flags.to_le_bytes());
        }
        *hasher.finalize().as_bytes()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotSafeProjection {
    FullState,
    ExcludeGuardedCoarseClockSamplesV2,
}

impl SnapshotSafeProjection {
    /// Only data samples are projected; MSR indices, inventory and order remain exact.
    pub fn msr_data_members(self) -> &'static [u32] {
        match self {
            Self::FullState => &[],
            Self::ExcludeGuardedCoarseClockSamplesV2 => &[0x10, 0x4000_0010, 0x4000_0020],
        }
    }

    /// Bind the comparison members independently of the installed denyfilter table.
    pub fn table_identity(self) -> Option<[u8; 32]> {
        match self {
            Self::FullState => None,
            Self::ExcludeGuardedCoarseClockSamplesV2 => {
                let mut hasher = blake3::Hasher::new();
                hasher.update(b"chaoscontrol.guarded-coarse-clock-projection.v2");
                hasher.update(&2u16.to_le_bytes());
                for member in [
                    b"clock".as_slice(),
                    b"vcpu_snapshots[*].msrs[index].data".as_slice(),
                ] {
                    hasher.update(&(member.len() as u32).to_le_bytes());
                    hasher.update(member);
                }
                hasher.update(&(self.msr_data_members().len() as u32).to_le_bytes());
                for index in self.msr_data_members() {
                    hasher.update(&index.to_le_bytes());
                }
                hasher.update(&SnapshotMsrFilterProfile::KvmCoarseClockDenyV1.table_identity());
                Some(*hasher.finalize().as_bytes())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotBoundaryProfile {
    pub id: BoundaryProfileId,
    pub schema_version: u16,
    pub table_identity: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotCpuidEntry {
    pub function: u32,
    pub index: u32,
    pub flags: u32,
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

impl SnapshotCpuidEntry {
    /// Remove only architectural processor-location identifiers, not topology
    /// widths/counts, CPU signatures, feature bits or XSTATE capabilities.
    fn canonicalize_host_placement(&mut self) {
        match self.function {
            // Initial APIC ID; EBX[23:16] logical-processor count is retained.
            1 => self.ebx &= 0x00ff_ffff,
            // Extended topology leaves report the querying CPU's x2APIC ID.
            0xB | 0x1F => self.edx = 0,
            // AMD extended APIC, core and node IDs. Threads/core and nodes/
            // processor remain in EBX[15:8] and ECX[10:8], respectively.
            0x8000_001E => {
                self.eax = 0;
                self.ebx &= !0xff;
                self.ecx &= !0xff;
            }
            _ => {}
        }
    }

    pub(crate) fn to_kvm_entry(self) -> kvm_bindings::kvm_cpuid_entry2 {
        kvm_bindings::kvm_cpuid_entry2 {
            function: self.function,
            index: self.index,
            flags: self.flags,
            eax: self.eax,
            ebx: self.ebx,
            ecx: self.ecx,
            edx: self.edx,
            padding: [0; 3],
        }
    }

    fn from_kvm_entry(entry: &kvm_bindings::kvm_cpuid_entry2) -> Self {
        Self {
            function: entry.function,
            index: entry.index,
            flags: entry.flags,
            eax: entry.eax,
            ebx: entry.ebx,
            ecx: entry.ecx,
            edx: entry.edx,
        }
    }

    fn execution_capability(mut self, amd: bool) -> Self {
        self.canonicalize_host_placement();
        if amd {
            match self.function {
                // AMD L3 size in 512-KiB units; preserve line size/associativity.
                0x8000_0006 if self.index == 0 => self.edx &= (1 << 18) - 1,
                // Number of sets is informational geometry, only for L3.
                0x8000_001D if (self.eax >> 5) & 7 == 3 && self.eax & 0x1f != 0 => self.ecx = 0,
                _ => {}
            }
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotHostOrigin {
    Measured,
    PortableFixture,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotHostIdentity {
    pub origin: SnapshotHostOrigin,
    pub kernel_release: String,
    pub kernel_version: String,
    pub machine: String,
    pub native_cpuid: Vec<SnapshotCpuidEntry>,
    pub kvm_api_version: i32,
    pub kvm_capabilities: Vec<(u32, i32)>,
    pub kvm_supported_cpuid: Vec<SnapshotCpuidEntry>,
    pub kvm_msr_indices: Vec<u32>,
}

impl SnapshotHostIdentity {
    pub fn capture() -> Result<Self, String> {
        let mut uts = std::mem::MaybeUninit::<libc::utsname>::uninit();
        // SAFETY: uname initializes the complete output on success.
        if unsafe { libc::uname(uts.as_mut_ptr()) } != 0 {
            return Err(format!(
                "cannot capture host uname: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: the successful call above initialized uts.
        let uts = unsafe { uts.assume_init() };
        let text = |bytes: &[libc::c_char]| {
            // SAFETY: uname fields are NUL-terminated arrays.
            unsafe { std::ffi::CStr::from_ptr(bytes.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        };
        let kvm = kvm_ioctls::Kvm::new().map_err(|error| format!("cannot inspect KVM: {error}"))?;
        let cpuid = kvm
            .get_supported_cpuid(kvm_bindings::KVM_MAX_CPUID_ENTRIES)
            .map_err(|error| format!("cannot inspect KVM CPUID: {error}"))?;
        let mut kvm_supported_cpuid: Vec<_> = cpuid
            .as_slice()
            .iter()
            .map(|entry| SnapshotCpuidEntry {
                function: entry.function,
                index: entry.index,
                flags: entry.flags,
                eax: entry.eax,
                ebx: entry.ebx,
                ecx: entry.ecx,
                edx: entry.edx,
            })
            .collect();
        kvm_supported_cpuid.sort_by_key(|entry| (entry.function, entry.index, entry.flags));
        let mut kvm_msr_indices = kvm
            .get_msr_index_list()
            .map_err(|error| format!("cannot inspect KVM MSRs: {error}"))?
            .as_slice()
            .to_vec();
        kvm_msr_indices.sort_unstable();
        let mut capabilities = SNAPSHOT_KVM_CAPABILITIES;
        capabilities.sort_unstable();
        let kvm_capabilities = capabilities
            .into_iter()
            .map(|capability| {
                (
                    capability,
                    kvm.check_extension_raw(capability as libc::c_ulong),
                )
            })
            .collect();
        let mut native_cpuid = Vec::new();
        for function in SNAPSHOT_NATIVE_CPUID_LEAVES {
            // CPUID is available on x86_64 and the current intrinsic is safe.
            let result = std::arch::x86_64::__cpuid_count(function, 0);
            native_cpuid.push(SnapshotCpuidEntry {
                function,
                index: 0,
                flags: 0,
                eax: result.eax,
                ebx: result.ebx,
                ecx: result.ecx,
                edx: result.edx,
            });
        }
        Ok(Self {
            origin: SnapshotHostOrigin::Measured,
            kernel_release: text(&uts.release),
            kernel_version: text(&uts.version),
            machine: text(&uts.machine),
            native_cpuid,
            kvm_api_version: kvm.get_api_version(),
            kvm_capabilities,
            kvm_supported_cpuid,
            kvm_msr_indices,
        })
    }

    /// Raw measurements remain exact provenance. Only this admission relation
    /// separates host placement and AMD L3 capacity from execution capability;
    /// the guest CPUID table itself is captured/restored/compared without loss.
    fn execution_capabilities_match(&self, other: &Self) -> bool {
        let amd = self.native_cpuid.iter().any(|entry| {
            entry.function == 0
                && entry.ebx == u32::from_le_bytes(*b"Auth")
                && entry.edx == u32::from_le_bytes(*b"enti")
                && entry.ecx == u32::from_le_bytes(*b"cAMD")
        });
        self.origin == other.origin
            && self.kernel_release == other.kernel_release
            && self.kernel_version == other.kernel_version
            && self.machine == other.machine
            && self.kvm_api_version == other.kvm_api_version
            && self.kvm_capabilities == other.kvm_capabilities
            && self.kvm_msr_indices == other.kvm_msr_indices
            && self.native_cpuid.len() == other.native_cpuid.len()
            && self
                .native_cpuid
                .iter()
                .zip(&other.native_cpuid)
                .all(|(left, right)| {
                    left.execution_capability(false) == right.execution_capability(false)
                })
            && self.kvm_supported_cpuid.len() == other.kvm_supported_cpuid.len()
            && self
                .kvm_supported_cpuid
                .iter()
                .zip(&other.kvm_supported_cpuid)
                .all(|(left, right)| {
                    left.execution_capability(amd) == right.execution_capability(amd)
                })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SnapshotClockDevicePolicy {
    DeterministicJiffies {
        in_kernel_pit_present: bool,
        lapic_timer_supported: bool,
    },
    VirtualTscPerExit,
}

/// CPU feature/clock policy, excluding per-VM entropy seeds retained separately.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotCpuPolicy {
    pub tsc_khz: u32,
    pub tsc_mode: crate::cpu::TscAdvanceMode,
    pub allow_avx2: bool,
    pub allow_avx512: bool,
    pub hide_hypervisor: bool,
    pub hide_tsc: bool,
    pub fixed_family: Option<u8>,
    pub fixed_model: Option<u8>,
    pub fixed_stepping: Option<u8>,
    pub fixed_frequency_mhz: Option<u16>,
    pub tsc_advance_per_tick: u64,
}

impl From<&crate::cpu::CpuConfig> for SnapshotCpuPolicy {
    fn from(cpu: &crate::cpu::CpuConfig) -> Self {
        Self {
            tsc_khz: cpu.tsc_khz,
            tsc_mode: cpu.tsc_mode,
            allow_avx2: cpu.allow_avx2,
            allow_avx512: cpu.allow_avx512,
            hide_hypervisor: cpu.hide_hypervisor,
            hide_tsc: cpu.hide_tsc,
            fixed_family: cpu.fixed_family,
            fixed_model: cpu.fixed_model,
            fixed_stepping: cpu.fixed_stepping,
            fixed_frequency_mhz: cpu.fixed_frequency_mhz,
            tsc_advance_per_tick: cpu.tsc_advance_per_tick,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotExecutionDescriptor {
    pub schema_version: u16,
    pub progress_mode: ProgressMode,
    pub initial_budget: ProgressBudget,
    pub boundary_profile: Option<SnapshotBoundaryProfile>,
    pub host: SnapshotHostIdentity,
    pub clock_device_policy: SnapshotClockDevicePolicy,
    pub cpu_policy: SnapshotCpuPolicy,
    pub coarse_msr_filter: Option<SnapshotMsrFilterProfile>,
    pub safe_projection: SnapshotSafeProjection,
}

impl SnapshotExecutionDescriptor {
    /// Construct a mechanism fixture, never a measured execution admission.
    /// `validate_current_host` rejects this domain before querying KVM.
    pub fn portable_fixture(config: &crate::vm::VmConfig) -> Result<Self, String> {
        Self::for_host(
            config,
            SnapshotHostIdentity {
                origin: SnapshotHostOrigin::PortableFixture,
                kernel_release: String::new(),
                kernel_version: String::new(),
                machine: String::new(),
                native_cpuid: Vec::new(),
                kvm_api_version: 0,
                kvm_capabilities: Vec::new(),
                kvm_supported_cpuid: Vec::new(),
                kvm_msr_indices: Vec::new(),
            },
        )
    }

    pub fn capture_host(config: &crate::vm::VmConfig) -> Result<Self, String> {
        Self::for_host(config, SnapshotHostIdentity::capture()?)
    }

    /// Reuse the measured source table, not whichever CCD a second ioctl sees.
    pub(crate) fn guest_cpuid_template(&self) -> Result<kvm_bindings::CpuId, String> {
        let mut cpuid = kvm_bindings::CpuId::new(self.host.kvm_supported_cpuid.len())
            .map_err(|error| format!("cannot allocate retained CPUID: {error}"))?;
        for (target, entry) in cpuid
            .as_mut_slice()
            .iter_mut()
            .zip(&self.host.kvm_supported_cpuid)
        {
            *target = entry.to_kvm_entry();
        }
        let policy = &self.cpu_policy;
        let config = crate::cpu::CpuConfig {
            tsc_khz: policy.tsc_khz,
            tsc_mode: policy.tsc_mode,
            allow_avx2: policy.allow_avx2,
            allow_avx512: policy.allow_avx512,
            hide_hypervisor: policy.hide_hypervisor,
            hide_tsc: policy.hide_tsc,
            fixed_family: policy.fixed_family,
            fixed_model: policy.fixed_model,
            fixed_stepping: policy.fixed_stepping,
            fixed_frequency_mhz: policy.fixed_frequency_mhz,
            tsc_advance_per_tick: policy.tsc_advance_per_tick,
            // CPUID filtering never consumes per-VM entropy.
            seed: 0,
        };
        crate::cpu::filter_cpuid_table(cpuid, &config).map_err(|error| error.to_string())
    }

    fn restore_compatible(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.progress_mode == other.progress_mode
            && self.initial_budget == other.initial_budget
            && self.boundary_profile == other.boundary_profile
            && self.clock_device_policy == other.clock_device_policy
            && self.cpu_policy == other.cpu_policy
            && self.coarse_msr_filter == other.coarse_msr_filter
            && self.safe_projection == other.safe_projection
            && self.host.execution_capabilities_match(&other.host)
    }

    fn for_host(config: &crate::vm::VmConfig, host: SnapshotHostIdentity) -> Result<Self, String> {
        let cpu = config.effective_cpu_config();
        let progress_mode = if config.num_vcpus > 1 {
            config.smp_progress_mode
        } else {
            ProgressMode::ExactSingleStep
        };
        let boundary_profile = match config.smp_progress_budget {
            ProgressBudget::KvmBoundaries { profile_id, .. } => Some(SnapshotBoundaryProfile {
                id: profile_id,
                schema_version: profile_id.schema_version(),
                table_identity: profile_id.table_identity(),
            }),
            ProgressBudget::Instructions(_) => None,
        };
        let descriptor = Self {
            schema_version: SNAPSHOT_EXECUTION_SCHEMA_VERSION,
            progress_mode,
            initial_budget: config.smp_progress_budget,
            boundary_profile,
            host,
            coarse_msr_filter: (progress_mode == ProgressMode::KvmDebugBoundary)
                .then_some(SnapshotMsrFilterProfile::KvmCoarseClockDenyV1),
            safe_projection: if progress_mode == ProgressMode::KvmDebugBoundary {
                SnapshotSafeProjection::ExcludeGuardedCoarseClockSamplesV2
            } else {
                SnapshotSafeProjection::FullState
            },
            clock_device_policy: if cpu.hide_tsc {
                SnapshotClockDevicePolicy::DeterministicJiffies {
                    in_kernel_pit_present: progress_mode != ProgressMode::KvmDebugBoundary,
                    lapic_timer_supported: progress_mode != ProgressMode::KvmDebugBoundary,
                }
            } else {
                SnapshotClockDevicePolicy::VirtualTscPerExit
            },
            cpu_policy: (&cpu).into(),
        };
        descriptor
            .validate()
            .map_err(|error| format!("invalid execution descriptor: {error:?}"))?;
        Ok(descriptor)
    }

    pub fn validate(&self) -> Result<(), SnapshotPreflightError> {
        if self.schema_version != SNAPSHOT_EXECUTION_SCHEMA_VERSION
            || self.initial_budget.count() == 0
        {
            return Err(SnapshotPreflightError::ExecutionDescriptorIncomplete);
        }
        match (
            self.progress_mode,
            self.initial_budget,
            &self.boundary_profile,
        ) {
            (
                ProgressMode::KvmDebugBoundary,
                ProgressBudget::KvmBoundaries { profile_id, .. },
                Some(profile),
            ) if profile.id == profile_id
                && profile.schema_version == profile_id.schema_version()
                && profile.table_identity == profile_id.table_identity()
                && self.cpu_policy.hide_tsc
                && self.cpu_policy.hide_hypervisor
                && self.cpu_policy.tsc_mode == crate::cpu::TscAdvanceMode::PerExit
                && self.clock_device_policy
                    == (SnapshotClockDevicePolicy::DeterministicJiffies {
                        in_kernel_pit_present: false,
                        lapic_timer_supported: false,
                    }) => {}
            (
                ProgressMode::ExactSingleStep | ProgressMode::PmuAccelerated { .. },
                ProgressBudget::Instructions(_),
                None,
            ) => {}
            _ => return Err(SnapshotPreflightError::ExecutionDescriptorMismatch),
        }
        if self.progress_mode == ProgressMode::KvmDebugBoundary {
            if self.coarse_msr_filter != Some(SnapshotMsrFilterProfile::KvmCoarseClockDenyV1)
                || self.safe_projection
                    != SnapshotSafeProjection::ExcludeGuardedCoarseClockSamplesV2
            {
                return Err(SnapshotPreflightError::ExecutionDescriptorMismatch);
            }
            if self.host.origin == SnapshotHostOrigin::Measured
                && !self
                    .host
                    .kvm_capabilities
                    .iter()
                    .any(|(capability, value)| {
                        *capability == kvm_bindings::KVM_CAP_X86_MSR_FILTER && *value > 0
                    })
            {
                return Err(SnapshotPreflightError::ExecutionDescriptorIncomplete);
            }
        } else if self.coarse_msr_filter.is_some()
            || self.safe_projection != SnapshotSafeProjection::FullState
        {
            return Err(SnapshotPreflightError::ExecutionDescriptorMismatch);
        }
        let expected_clock = if self.cpu_policy.hide_tsc {
            SnapshotClockDevicePolicy::DeterministicJiffies {
                in_kernel_pit_present: self.progress_mode != ProgressMode::KvmDebugBoundary,
                lapic_timer_supported: self.progress_mode != ProgressMode::KvmDebugBoundary,
            }
        } else {
            SnapshotClockDevicePolicy::VirtualTscPerExit
        };
        if self.clock_device_policy != expected_clock || self.cpu_policy.tsc_khz == 0 {
            return Err(SnapshotPreflightError::ExecutionDescriptorMismatch);
        }
        if self.host.origin == SnapshotHostOrigin::Measured
            && (self.host.kernel_release.is_empty()
                || self.host.kernel_version.is_empty()
                || self.host.machine != "x86_64"
                || self.host.kvm_api_version != 12
                || self.host.native_cpuid.len() != SNAPSHOT_NATIVE_CPUID_LEAVES.len()
                || self
                    .host
                    .native_cpuid
                    .iter()
                    .zip(SNAPSHOT_NATIVE_CPUID_LEAVES)
                    .any(|(entry, leaf)| {
                        entry.function != leaf || entry.index != 0 || entry.flags != 0
                    })
                || validate_cpuid_entries(&self.host.kvm_supported_cpuid).is_err()
                || self.host.kvm_capabilities.len() != SNAPSHOT_KVM_CAPABILITIES.len()
                || SNAPSHOT_KVM_CAPABILITIES.iter().any(|capability| {
                    !self
                        .host
                        .kvm_capabilities
                        .iter()
                        .any(|(id, value)| id == capability && *value >= 0)
                })
                || self.host.kvm_msr_indices.is_empty()
                || self
                    .host
                    .kvm_capabilities
                    .windows(2)
                    .any(|pair| pair[0].0 >= pair[1].0)
                || self
                    .host
                    .kvm_msr_indices
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1]))
        {
            return Err(SnapshotPreflightError::ExecutionDescriptorIncomplete);
        }
        Ok(())
    }

    pub fn validate_current_host(&self) -> Result<(), SnapshotPreflightError> {
        self.validate()?;
        if self.host.origin != SnapshotHostOrigin::Measured {
            return Err(SnapshotPreflightError::HostMismatch);
        }
        let host =
            SnapshotHostIdentity::capture().map_err(SnapshotPreflightError::HostUnavailable)?;
        if !host.execution_capabilities_match(&self.host) {
            return Err(SnapshotPreflightError::HostMismatch);
        }
        Ok(())
    }

    /// Coarse guests use only the deterministic userspace PIT. Even an inhibited
    /// kernel PIT exposes host-clock-driven channel/speaker reads.
    pub fn kernel_pit_present(&self) -> bool {
        !matches!(
            self.clock_device_policy,
            SnapshotClockDevicePolicy::DeterministicJiffies {
                in_kernel_pit_present: false,
                ..
            }
        )
    }

    /// Retained eligibility for raw KVM clock and the exact guarded MSR data samples.
    /// This does not establish an installed filter or healthy live execution authority.
    pub fn permits_coarse_clock_projection(&self) -> bool {
        self.validate().is_ok()
            && self.progress_mode == ProgressMode::KvmDebugBoundary
            && self.safe_projection == SnapshotSafeProjection::ExcludeGuardedCoarseClockSamplesV2
            && self.coarse_msr_filter == Some(SnapshotMsrFilterProfile::KvmCoarseClockDenyV1)
            && self.host.origin == SnapshotHostOrigin::Measured
            && self
                .host
                .kvm_capabilities
                .iter()
                .any(|(capability, value)| {
                    *capability == kvm_bindings::KVM_CAP_X86_MSR_FILTER && *value > 0
                })
    }

    pub fn validate_config(
        &self,
        config: &crate::vm::VmConfig,
    ) -> Result<(), SnapshotPreflightError> {
        self.validate()?;
        let mode = if config.num_vcpus > 1 {
            config.smp_progress_mode
        } else {
            ProgressMode::ExactSingleStep
        };
        if self.progress_mode != mode
            || self.initial_budget != config.smp_progress_budget
            || self.cpu_policy != SnapshotCpuPolicy::from(&config.effective_cpu_config())
        {
            return Err(SnapshotPreflightError::ExecutionDescriptorMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InjectionKind {
    Irq { irq: u32, level: bool },
    Nmi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InjectionSource {
    Shell,
    Pit,
    Serial,
    Virtio,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InjectionRecord {
    pub target_vcpu: Option<u32>,
    pub kind: InjectionKind,
    pub source: InjectionSource,
    pub requested_state_id: [u8; 32],
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InjectionLedger {
    pub sequence: u64,
    pub prefix_digest: [u8; 32],
}

impl InjectionLedger {
    pub fn new() -> Self {
        Self {
            sequence: 0,
            prefix_digest: *blake3::hash(b"chaoscontrol.injection-ledger.v1").as_bytes(),
        }
    }

    /// Append canonical requested/accepted facts. These never imply observed delivery.
    pub fn record(&mut self, fact: &InjectionRecord) -> Result<(), SnapshotPreflightError> {
        if fact.requested_state_id == [0; 32]
            || matches!(fact.kind, InjectionKind::Nmi) && fact.target_vcpu.is_none()
        {
            return Err(SnapshotPreflightError::InjectionLedgerInvalid);
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(SnapshotPreflightError::InjectionLedgerInvalid)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"chaoscontrol.injection-ledger.v1");
        hasher.update(&self.prefix_digest);
        hasher.update(&sequence.to_le_bytes());
        match fact.target_vcpu {
            Some(vcpu) => {
                hasher.update(&[1]);
                hasher.update(&vcpu.to_le_bytes());
            }
            None => {
                hasher.update(&[0]);
            }
        }
        match fact.kind {
            InjectionKind::Irq { irq, level } => {
                hasher.update(&[0]);
                hasher.update(&irq.to_le_bytes());
                hasher.update(&[u8::from(level)]);
            }
            InjectionKind::Nmi => {
                hasher.update(&[1]);
            }
        }
        hasher.update(&[match fact.source {
            InjectionSource::Shell => 0,
            InjectionSource::Pit => 1,
            InjectionSource::Serial => 2,
            InjectionSource::Virtio => 3,
        }]);
        hasher.update(&fact.requested_state_id);
        hasher.update(&[u8::from(fact.accepted)]);
        self.prefix_digest = *hasher.finalize().as_bytes();
        self.sequence = sequence;
        Ok(())
    }
}

impl Default for InjectionLedger {
    fn default() -> Self {
        Self::new()
    }
}

/// Check an ordered, already-observed injection prefix without inventing delivery.
pub fn validate_injection_records(
    prior: &InjectionLedger,
    records: &[InjectionRecord],
    expected: &InjectionLedger,
    vcpu_count: usize,
) -> Result<(), SnapshotPreflightError> {
    if vcpu_count == 0
        || vcpu_count > MAX_SCHEDULE_VCPUS
        || prior.prefix_digest == [0; 32]
        || expected.prefix_digest == [0; 32]
        || (prior.sequence == 0 && *prior != InjectionLedger::new())
    {
        return Err(SnapshotPreflightError::InjectionLedgerInvalid);
    }
    let mut actual = prior.clone();
    for record in records {
        if record
            .target_vcpu
            .is_some_and(|vcpu| vcpu as usize >= vcpu_count)
        {
            return Err(SnapshotPreflightError::InjectionLedgerInvalid);
        }
        actual.record(record)?;
    }
    if &actual != expected {
        return Err(SnapshotPreflightError::InjectionLedgerInvalid);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SnapshotPendingIoState {
    Settled,
    Pending { vcpu: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotDeliveryState {
    /// Available events/shadows, LAPIC/PIC/IOAPIC/PIT and virtio state are retained;
    /// no unobservable delivery instruction or order is asserted.
    CapturedKernelAndUserspace,
    Incomplete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotBoundaryState {
    pub pending_io: SnapshotPendingIoState,
    pub injection_ledger: InjectionLedger,
    pub delivery_state: SnapshotDeliveryState,
}

impl SnapshotBoundaryState {
    pub fn validate(&self) -> Result<(), SnapshotPreflightError> {
        if self.pending_io != SnapshotPendingIoState::Settled {
            return Err(SnapshotPreflightError::PendingIoIncomplete);
        }
        if self.delivery_state != SnapshotDeliveryState::CapturedKernelAndUserspace {
            return Err(SnapshotPreflightError::DeliveryStateIncomplete);
        }
        if self.injection_ledger.prefix_digest == [0; 32]
            || (self.injection_ledger.sequence == 0
                && self.injection_ledger != InjectionLedger::new())
        {
            return Err(SnapshotPreflightError::InjectionLedgerInvalid);
        }
        Ok(())
    }
}

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
    ExecutionDescriptor,
    PendingIoSettlement,
    AvailableDelivery,
    InjectionLedger,
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
#[serde(deny_unknown_fields)]
pub struct SnapshotMetadata {
    pub state_schema_version: u32,
    pub completeness_profile: String,
    pub topology: SnapshotTopology,
    pub inventory: Vec<SnapshotComponent>,
    pub execution_descriptor: SnapshotExecutionDescriptor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotPreflightError {
    LegacyOrIncomplete,
    SchemaVersion { actual: u32 },
    Profile { actual: String },
    TopologyMismatch,
    DuplicateDeviceIdentity(VirtioDeviceIdentity),
    InventoryMismatch,
    ExecutionDescriptorIncomplete,
    ExecutionDescriptorMismatch,
    HostUnavailable(String),
    HostMismatch,
    PendingIoIncomplete,
    DeliveryStateIncomplete,
    InjectionLedgerInvalid,
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
        SnapshotComponent::ExecutionDescriptor,
        SnapshotComponent::PendingIoSettlement,
        SnapshotComponent::AvailableDelivery,
        SnapshotComponent::InjectionLedger,
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
    expected_execution: &SnapshotExecutionDescriptor,
) -> Result<(), SnapshotPreflightError> {
    let metadata = metadata.ok_or(SnapshotPreflightError::LegacyOrIncomplete)?;
    if metadata.state_schema_version != SNAPSHOT_STATE_SCHEMA_VERSION {
        return Err(SnapshotPreflightError::SchemaVersion {
            actual: metadata.state_schema_version,
        });
    }
    if metadata.completeness_profile != SNAPSHOT_PROFILE_EXACT_X86_KVM_V2 {
        return Err(SnapshotPreflightError::Profile {
            actual: metadata.completeness_profile.clone(),
        });
    }
    metadata.execution_descriptor.validate()?;
    if !metadata
        .execution_descriptor
        .restore_compatible(expected_execution)
    {
        return Err(SnapshotPreflightError::ExecutionDescriptorMismatch);
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
    if metadata.topology.vcpu_count == 0
        || metadata.topology.vcpu_count as usize > MAX_SCHEDULE_VCPUS
        || metadata.topology.msr_indices.is_empty()
        || metadata
            .topology
            .msr_indices
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
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
    pub execution_descriptor: SnapshotExecutionDescriptor,
    pub boundary_state: SnapshotBoundaryState,
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

fn validate_cpuid_entries(entries: &[SnapshotCpuidEntry]) -> Result<(), &'static str> {
    if entries.is_empty() || entries.len() > kvm_bindings::KVM_MAX_CPUID_ENTRIES {
        return Err("CPUID entry count is outside the admitted bound");
    }
    for (index, entry) in entries.iter().enumerate() {
        if entry.flags & !kvm_bindings::KVM_CPUID_FLAG_SIGNIFCANT_INDEX != 0
            || entry.index != 0 && entry.flags & kvm_bindings::KVM_CPUID_FLAG_SIGNIFCANT_INDEX == 0
            || entries[..index]
                .iter()
                .any(|prior| prior.function == entry.function && prior.index == entry.index)
        {
            return Err("CPUID keys or flags are invalid");
        }
    }
    Ok(())
}

fn deserialize_bounded_cpuid<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<SnapshotCpuidEntry>, D::Error> {
    struct CpuidVisitor;
    impl<'de> serde::de::Visitor<'de> for CpuidVisitor {
        type Value = Vec<SnapshotCpuidEntry>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                formatter,
                "1..={} CPUID entries",
                kvm_bindings::KVM_MAX_CPUID_ENTRIES
            )
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let hinted = sequence.size_hint().unwrap_or(0);
            if hinted > kvm_bindings::KVM_MAX_CPUID_ENTRIES {
                return Err(serde::de::Error::custom("CPUID entry bound exceeded"));
            }
            let mut entries = Vec::with_capacity(hinted);
            while let Some(entry) = sequence.next_element()? {
                if entries.len() == kvm_bindings::KVM_MAX_CPUID_ENTRIES {
                    return Err(serde::de::Error::custom("CPUID entry bound exceeded"));
                }
                entries.push(entry);
            }
            validate_cpuid_entries(&entries).map_err(serde::de::Error::custom)?;
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(CpuidVisitor)
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
    /// Exact guest-facing CPUID, including KVM-derived runtime bits.
    pub cpuid: Vec<SnapshotCpuidEntry>,
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
    #[serde(deserialize_with = "deserialize_bounded_cpuid")]
    cpuid: Vec<SnapshotCpuidEntry>,
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
            cpuid: snapshot.cpuid.clone(),
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
            cpuid: value.cpuid,
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

/// Absence is part of the admitted device topology, never synthetic zero state.
#[derive(Clone, Debug)]
pub enum SnapshotKernelPitState {
    Absent,
    Present(kvm_pit_state2),
}

impl Serialize for SnapshotKernelPitState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Absent => {
                serializer.serialize_unit_variant("SnapshotKernelPitState", 0, "Absent")
            }
            Self::Present(pit) => serializer.serialize_newtype_variant(
                "SnapshotKernelPitState",
                1,
                "Present",
                &BorrowedPod(pit),
            ),
        }
    }
}

impl<'de> Deserialize<'de> for SnapshotKernelPitState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum EncodedPit {
            Absent,
            Present(Vec<u8>),
        }
        match EncodedPit::deserialize(deserializer)? {
            EncodedPit::Absent => Ok(Self::Absent),
            EncodedPit::Present(bytes) => pod_from_bytes(&bytes, "pit")
                .map(Self::Present)
                .map_err(serde::de::Error::custom),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct VmSnapshotSerde {
    #[serde(default)]
    metadata: Option<SnapshotMetadata>,
    boundary_state: SnapshotBoundaryState,
    vcpu_snapshots: Vec<VcpuSnapshot>,
    pic_master: Vec<u8>,
    pic_slave: Vec<u8>,
    ioapic: Vec<u8>,
    pit: SnapshotKernelPitState,
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
    #[serde(deserialize_with = "deserialize_bounded_hlt_latches")]
    hlt_latched_vcpus: Vec<bool>,
}

impl From<&VmSnapshot> for VmSnapshotSerde {
    fn from(snapshot: &VmSnapshot) -> Self {
        Self {
            metadata: snapshot.metadata.clone(),
            boundary_state: snapshot.boundary_state.clone(),
            vcpu_snapshots: snapshot.vcpu_snapshots.clone(),
            pic_master: pod_to_bytes(&snapshot.pic_master),
            pic_slave: pod_to_bytes(&snapshot.pic_slave),
            ioapic: pod_to_bytes(&snapshot.ioapic),
            pit: snapshot.pit.clone(),
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
            boundary_state: value.boundary_state,
            vcpu_snapshots: value.vcpu_snapshots,
            pic_master: pod_from_bytes(&value.pic_master, "pic_master")?,
            pic_slave: pod_from_bytes(&value.pic_slave, "pic_slave")?,
            ioapic: pod_from_bytes(&value.ioapic, "ioapic")?,
            pit: value.pit,
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
            cpuid: vcpu
                .get_cpuid2(kvm_bindings::KVM_MAX_CPUID_ENTRIES)
                .context(GetVcpuCpuidSnafu)?
                .as_slice()
                .iter()
                .map(SnapshotCpuidEntry::from_kvm_entry)
                .collect(),
            mp_state: vcpu.get_mp_state().context(GetMpStateSnafu)?,
        })
    }

    pub fn validate(&self) -> Result<(), SnapshotError> {
        validate_cpuid_entries(&self.cpuid)
            .map_err(|_| ArchitectureFieldSnafu { field: "cpuid" }.build())?;
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

    fn validate_guest_cpuid(
        &self,
        descriptor: &SnapshotExecutionDescriptor,
        vcpu_id: u32,
        vcpu_count: u32,
    ) -> Result<(), SnapshotError> {
        // Portable tables exercise codec mechanics, never measured admission.
        if descriptor.host.origin == SnapshotHostOrigin::PortableFixture {
            return Ok(());
        }
        let expected = descriptor
            .guest_cpuid_template()
            .map_err(|message| SnapshotError::Topology { message })?;
        self.validate_guest_cpuid_template(expected, vcpu_id, vcpu_count)
    }

    fn validate_guest_cpuid_template(
        &self,
        mut expected: kvm_bindings::CpuId,
        vcpu_id: u32,
        vcpu_count: u32,
    ) -> Result<(), SnapshotError> {
        crate::cpu::patch_cpuid_apic_id_in_place(&mut expected, vcpu_id, vcpu_count);
        let xstate = expected
            .as_slice()
            .iter()
            .find(|entry| entry.function == 0xD && entry.index == 0);
        let xcr0 = if let Some(xstate) = xstate {
            let xcr0 = self.xcrs.xcrs[..self.xcrs.nr_xcrs as usize]
                .iter()
                .find(|xcr| xcr.xcr == 0)
                .ok_or_else(|| {
                    ArchitectureFieldSnafu {
                        field: "cpuid.xcr0",
                    }
                    .build()
                })?
                .value;
            if xcr0 & !(u64::from(xstate.eax) | (u64::from(xstate.edx) << 32)) != 0 {
                return ArchitectureFieldSnafu {
                    field: "cpuid.xcr0",
                }
                .fail();
            }
            xcr0
        } else {
            0
        };
        let supported_xss = expected
            .as_slice()
            .iter()
            .find(|entry| entry.function == 0xD && entry.index == 1)
            .map_or(0, |entry| {
                u64::from(entry.ecx) | (u64::from(entry.edx) << 32)
            });
        let xss = if supported_xss != 0 {
            self.msrs
                .iter()
                .find(|msr| msr.index == 0xDA0)
                .ok_or_else(|| ArchitectureFieldSnafu { field: "cpuid.xss" }.build())?
                .data
        } else {
            0
        };
        if xss & !supported_xss != 0 {
            return ArchitectureFieldSnafu { field: "cpuid.xss" }.fail();
        }
        crate::cpu::apply_cpuid_runtime_state(
            &mut expected,
            self.sregs.cr4,
            self.sregs.apic_base,
            xcr0,
            xss,
        )
        .map_err(|_| {
            ArchitectureFieldSnafu {
                field: "cpuid.runtime",
            }
            .build()
        })?;
        if self.cpuid.len() != expected.as_slice().len() {
            return ArchitectureFieldSnafu {
                field: "cpuid.inventory",
            }
            .fail();
        }
        let kvm_signature = expected.as_slice().iter().any(|entry| {
            entry.function == 0x4000_0000
                && entry.ebx == u32::from_le_bytes(*b"KVMK")
                && entry.ecx == u32::from_le_bytes(*b"VMKV")
                && entry.edx == u32::from_le_bytes(*b"M\0\0\0")
        });
        for baseline in expected.as_slice() {
            let actual = self
                .cpuid
                .iter()
                .find(|entry| entry.function == baseline.function && entry.index == baseline.index)
                .ok_or_else(|| {
                    ArchitectureFieldSnafu {
                        field: "cpuid.inventory",
                    }
                    .build()
                })?;
            let mut baseline = SnapshotCpuidEntry::from_kvm_entry(baseline);
            // With an in-kernel IRQ chip, KVM may remove PV_UNHALT when HLT
            // executes in-guest. It may not add it or rewrite any other bit.
            if kvm_signature && baseline.function == 0x4000_0001 && actual.eax & (1 << 7) == 0 {
                baseline.eax &= !(1 << 7);
            }
            if *actual != baseline {
                return ArchitectureFieldSnafu {
                    field: "cpuid.policy",
                }
                .fail();
            }
        }
        Ok(())
    }

    fn restore_cpuid(&self, vcpu: &VcpuFd) -> Result<(), SnapshotError> {
        let mut cpuid =
            kvm_bindings::CpuId::new(self.cpuid.len()).map_err(|_| CpuidBufferSnafu.build())?;
        for (target, entry) in cpuid.as_mut_slice().iter_mut().zip(&self.cpuid) {
            *target = entry.to_kvm_entry();
        }
        vcpu.set_cpuid2(&cpuid).context(SetVcpuCpuidSnafu)
    }

    fn validate_restored_cpuid(&self, vcpu: &VcpuFd) -> Result<(), SnapshotError> {
        let actual = vcpu
            .get_cpuid2(kvm_bindings::KVM_MAX_CPUID_ENTRIES)
            .context(GetVcpuCpuidSnafu)?;
        if actual.as_slice().len() != self.cpuid.len()
            || actual
                .as_slice()
                .iter()
                .zip(&self.cpuid)
                .any(|(actual, retained)| SnapshotCpuidEntry::from_kvm_entry(actual) != *retained)
        {
            return ArchitectureFieldSnafu {
                field: "cpuid.restored",
            }
            .fail();
        }
        Ok(())
    }

    /// Restore all architecture state to a single vCPU.
    pub fn restore(&self, vcpu: &VcpuFd) -> Result<(), SnapshotError> {
        self.validate()?;
        self.restore_cpuid(vcpu)?;
        self.restore_after_cpuid(vcpu)
    }

    fn restore_after_cpuid(&self, vcpu: &VcpuFd) -> Result<(), SnapshotError> {
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
        self.validate_restored_cpuid(vcpu)
    }
}

/// Complete VM state — everything needed to restore a VM to an exact point.
#[derive(Clone, Debug)]
pub struct VmSnapshot {
    /// Internal fidelity schema and exact component inventory.
    pub metadata: Option<SnapshotMetadata>,
    pub boundary_state: SnapshotBoundaryState,

    /// Per-vCPU register state (one entry per vCPU).
    pub vcpu_snapshots: Vec<VcpuSnapshot>,

    // In-kernel device state
    pub pic_master: kvm_irqchip,
    pub pic_slave: kvm_irqchip,
    pub ioapic: kvm_irqchip,
    pub pit: SnapshotKernelPitState,
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

const SNAPSHOT_CONTINUATION_SCHEMA_VERSION: u16 = 1;
const SNAPSHOT_CONTINUATION_DOMAIN: &str = "chaoscontrol.vm-snapshot-continuation.v1";

/// A streaming comparison view. The retained snapshot serializer stays unchanged.
pub struct VmSnapshotContinuation<'a> {
    snapshot: &'a VmSnapshot,
    project_coarse_clock_samples: bool,
}

struct BorrowedPod<'a, T>(&'a T);

impl<T> Serialize for BorrowedPod<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // SAFETY: callers only wrap the same plain KVM ABI structures supported
        // by pod_to_bytes; the borrowed view cannot outlive the original value.
        let bytes = unsafe {
            std::slice::from_raw_parts(self.0 as *const T as *const u8, std::mem::size_of::<T>())
        };
        bytes.serialize(serializer)
    }
}

struct BorrowedDirtyPages<'a>(&'a BTreeMap<usize, Box<[u8; PAGE_SIZE]>>);

impl Serialize for BorrowedDirtyPages<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (index, page) in self.0 {
            map.serialize_entry(index, page.as_slice())?;
        }
        map.end()
    }
}

struct BorrowedSnapshotMemory<'a>(&'a SnapshotMemory);

impl Serialize for BorrowedSnapshotMemory<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStructVariant;
        match self.0 {
            SnapshotMemory::Full(memory) => serializer.serialize_newtype_variant(
                "SnapshotMemorySerde",
                0,
                "Full",
                memory.as_slice(),
            ),
            SnapshotMemory::Overlay { base, dirty_pages } => {
                let mut state =
                    serializer.serialize_struct_variant("SnapshotMemorySerde", 1, "Overlay", 2)?;
                state.serialize_field("base", base.as_slice())?;
                state.serialize_field("dirty_pages", &BorrowedDirtyPages(dirty_pages))?;
                state.end()
            }
        }
    }
}

struct BorrowedVcpuSnapshot<'a> {
    snapshot: &'a VcpuSnapshot,
    projection: SnapshotSafeProjection,
}

impl Serialize for BorrowedVcpuSnapshot<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let VcpuSnapshot {
            regs,
            sregs,
            fpu,
            debug_regs,
            lapic,
            xcrs,
            xsave,
            events,
            msrs,
            cpuid,
            mp_state,
        } = self.snapshot;
        let mut state = serializer.serialize_struct("VcpuSnapshotSerde", 11)?;
        state.serialize_field("regs", &BorrowedPod(regs))?;
        state.serialize_field("sregs", &BorrowedPod(sregs))?;
        state.serialize_field("fpu", &BorrowedPod(fpu))?;
        state.serialize_field("debug_regs", &BorrowedPod(debug_regs))?;
        state.serialize_field("lapic", &BorrowedPod(lapic))?;
        state.serialize_field("xcrs", &BorrowedPod(xcrs))?;
        state.serialize_field("xsave", xsave)?;
        state.serialize_field("events", &BorrowedPod(events))?;
        state.serialize_field("msrs", &BorrowedMsrSamples(msrs, self.projection))?;
        state.serialize_field("cpuid", cpuid)?;
        state.serialize_field("mp_state", &BorrowedPod(mp_state))?;
        state.end()
    }
}

struct BorrowedMsrSamples<'a>(&'a [MsrSnapshot], SnapshotSafeProjection);

impl Serialize for BorrowedMsrSamples<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeSeq, SerializeStruct};
        struct ProjectedSample<'a>(&'a MsrSnapshot);
        impl Serialize for ProjectedSample<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut state = serializer.serialize_struct("MsrSnapshot", 2)?;
                state.serialize_field("index", &self.0.index)?;
                state.serialize_field("data", &None::<u64>)?;
                state.end()
            }
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for sample in self.0 {
            if self.1.msr_data_members().contains(&sample.index) {
                sequence.serialize_element(&ProjectedSample(sample))?;
            } else {
                sequence.serialize_element(sample)?;
            }
        }
        sequence.end()
    }
}

struct BorrowedVcpuSnapshots<'a>(&'a [VcpuSnapshot], SnapshotSafeProjection);

impl Serialize for BorrowedVcpuSnapshots<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for vcpu in self.0 {
            sequence.serialize_element(&BorrowedVcpuSnapshot {
                snapshot: vcpu,
                projection: self.1,
            })?;
        }
        sequence.end()
    }
}

struct BorrowedSerialState<'a>(&'a vm_superio::SerialState);

impl Serialize for BorrowedSerialState<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let vm_superio::SerialState {
            baud_divisor_low,
            baud_divisor_high,
            interrupt_enable,
            interrupt_identification,
            line_control,
            line_status,
            modem_control,
            modem_status,
            scratch,
            in_buffer,
        } = self.0;
        let mut state = serializer.serialize_struct("SerialStateSerde", 10)?;
        state.serialize_field("baud_divisor_low", baud_divisor_low)?;
        state.serialize_field("baud_divisor_high", baud_divisor_high)?;
        state.serialize_field("interrupt_enable", interrupt_enable)?;
        state.serialize_field("interrupt_identification", interrupt_identification)?;
        state.serialize_field("line_control", line_control)?;
        state.serialize_field("line_status", line_status)?;
        state.serialize_field("modem_control", modem_control)?;
        state.serialize_field("modem_status", modem_status)?;
        state.serialize_field("scratch", scratch)?;
        state.serialize_field("in_buffer", in_buffer)?;
        state.end()
    }
}

impl Serialize for VmSnapshotContinuation<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        // Deliberately exhaustive: every future snapshot field must be assigned
        // an explicit continuation identity instead of silently being ignored.
        let VmSnapshot {
            metadata,
            boundary_state,
            vcpu_snapshots,
            pic_master,
            pic_slave,
            ioapic,
            pit,
            clock,
            memory,
            serial_state,
            entropy,
            virtual_tsc,
            exit_count,
            io_exit_count,
            exits_since_last_sdk,
            panic_detected,
            panic_match_state,
            pit_snapshot,
            last_kvm_pit_mode,
            fault_engine_snapshot,
            virtio_snapshots,
            coverage_active,
            scheduler_snapshot,
            hlt_latched_vcpus,
        } = self.snapshot;
        let projection = if self.project_coarse_clock_samples {
            SnapshotSafeProjection::ExcludeGuardedCoarseClockSamplesV2
        } else {
            SnapshotSafeProjection::FullState
        };
        let projection_table_identity = projection.table_identity();
        let mut state = serializer.serialize_struct("VmSnapshotContinuation", 28)?;
        state.serialize_field("continuation_domain", SNAPSHOT_CONTINUATION_DOMAIN)?;
        state.serialize_field(
            "continuation_schema_version",
            &SNAPSHOT_CONTINUATION_SCHEMA_VERSION,
        )?;
        state.serialize_field("projection", &projection)?;
        state.serialize_field("projection_table_identity", &projection_table_identity)?;
        state.serialize_field("metadata", metadata)?;
        state.serialize_field("boundary_state", boundary_state)?;
        state.serialize_field(
            "vcpu_snapshots",
            &BorrowedVcpuSnapshots(vcpu_snapshots, projection),
        )?;
        state.serialize_field("pic_master", &BorrowedPod(pic_master))?;
        state.serialize_field("pic_slave", &BorrowedPod(pic_slave))?;
        state.serialize_field("ioapic", &BorrowedPod(ioapic))?;
        state.serialize_field("pit", pit)?;
        state.serialize_field(
            "clock",
            &(!self.project_coarse_clock_samples).then_some(BorrowedPod(clock)),
        )?;
        state.serialize_field("memory", &BorrowedSnapshotMemory(memory))?;
        state.serialize_field("serial_state", &BorrowedSerialState(serial_state))?;
        state.serialize_field("entropy", entropy)?;
        state.serialize_field("virtual_tsc", virtual_tsc)?;
        state.serialize_field("exit_count", exit_count)?;
        state.serialize_field("io_exit_count", io_exit_count)?;
        state.serialize_field("exits_since_last_sdk", exits_since_last_sdk)?;
        state.serialize_field("panic_detected", panic_detected)?;
        state.serialize_field("panic_match_state", panic_match_state)?;
        state.serialize_field("pit_snapshot", pit_snapshot)?;
        state.serialize_field("last_kvm_pit_mode", last_kvm_pit_mode)?;
        state.serialize_field("fault_engine_snapshot", fault_engine_snapshot)?;
        state.serialize_field("virtio_snapshots", virtio_snapshots)?;
        state.serialize_field("coverage_active", coverage_active)?;
        state.serialize_field("scheduler_snapshot", scheduler_snapshot)?;
        state.serialize_field("hlt_latched_vcpus", hlt_latched_vcpus)?;
        state.end()
    }
}

impl VmSnapshot {
    /// Borrow all continuation state without cloning RAM, registers or devices.
    /// Projection validates retained descriptor bytes only, not execution authority.
    /// Callers compare equal descriptors; runner construction/restore must admit
    /// the actual host and enforced filter before any guest entry.
    pub fn continuation_view(
        &self,
        project_coarse_clock_samples: bool,
    ) -> Result<VmSnapshotContinuation<'_>, SnapshotPreflightError> {
        if project_coarse_clock_samples {
            let descriptor = self.execution_descriptor()?;
            if !descriptor.permits_coarse_clock_projection() {
                return Err(SnapshotPreflightError::ExecutionDescriptorMismatch);
            }
        }
        Ok(VmSnapshotContinuation {
            snapshot: self,
            project_coarse_clock_samples,
        })
    }

    pub fn execution_descriptor(
        &self,
    ) -> Result<&SnapshotExecutionDescriptor, SnapshotPreflightError> {
        self.metadata
            .as_ref()
            .map(|metadata| &metadata.execution_descriptor)
            .ok_or(SnapshotPreflightError::LegacyOrIncomplete)
    }

    /// Validate the full retained continuation without host access or mutation.
    pub fn validate_retained_boundary(&self) -> Result<(), SnapshotError> {
        let metadata = self
            .metadata
            .as_ref()
            .ok_or_else(|| SnapshotError::Topology {
                message: "snapshot lacks completeness metadata".to_string(),
            })?;
        validate_snapshot_metadata(
            Some(metadata),
            &metadata.topology,
            &metadata.execution_descriptor,
        )
        .and_then(|()| self.boundary_state.validate())
        .map_err(|error| SnapshotError::Topology {
            message: format!("snapshot binding preflight failed: {error:?}"),
        })?;
        self.scheduler_snapshot
            .validate()
            .map_err(|error| SnapshotError::Topology {
                message: format!("snapshot scheduler preflight failed: {error:?}"),
            })?;
        let vcpu_count = self.vcpu_snapshots.len();
        if metadata.topology.vcpu_count as usize != vcpu_count
            || self.scheduler_snapshot.state.num_vcpus != vcpu_count
            || self.scheduler_snapshot.state.progress_mode
                != metadata.execution_descriptor.progress_mode
            || !self
                .scheduler_snapshot
                .state
                .progress_budget()
                .same_unit(metadata.execution_descriptor.initial_budget)
            || self.hlt_latched_vcpus.len() != vcpu_count
        {
            return Err(SnapshotError::Topology {
                message: "snapshot scheduler/settled state mismatch".to_string(),
            });
        }
        if self.virtio_snapshots.len() != metadata.topology.virtio_devices.len()
            || metadata
                .topology
                .virtio_devices
                .iter()
                .any(|(identity, queues)| {
                    self.virtio_snapshots
                        .iter()
                        .filter(|device| {
                            device.identity() == *identity
                                && device.transport.queues.len() == *queues as usize
                                && device.backend.device_id() == identity.device_id
                        })
                        .count()
                        != 1
                })
        {
            return Err(SnapshotError::Topology {
                message: "snapshot delivery device inventory mismatch".to_string(),
            });
        }
        for (vcpu_id, vcpu) in self.vcpu_snapshots.iter().enumerate() {
            vcpu.validate_msr_inventory(&metadata.topology.msr_indices)?;
            vcpu.validate_guest_cpuid(
                &metadata.execution_descriptor,
                vcpu_id as u32,
                metadata.topology.vcpu_count,
            )?;
        }
        if metadata.execution_descriptor.kernel_pit_present()
            != matches!(self.pit, SnapshotKernelPitState::Present(_))
        {
            return Err(SnapshotError::Topology {
                message: "snapshot kernel PIT presence differs from admitted clock policy"
                    .to_string(),
            });
        }
        if let SnapshotKernelPitState::Present(pit) = &self.pit {
            if pit.flags & kvm_bindings::KVM_PIT_FLAGS_HPET_LEGACY == 0 {
                return Err(SnapshotError::Topology {
                    message: "snapshot PIT is not inhibited by the admitted clock policy"
                        .to_string(),
                });
            }
        }
        if metadata.execution_descriptor.progress_mode == ProgressMode::KvmDebugBoundary {
            for vcpu in &self.vcpu_snapshots {
                let lapic_word = |offset: usize| {
                    u32::from_le_bytes([
                        vcpu.lapic.regs[offset] as u8,
                        vcpu.lapic.regs[offset + 1] as u8,
                        vcpu.lapic.regs[offset + 2] as u8,
                        vcpu.lapic.regs[offset + 3] as u8,
                    ])
                };
                let timer_lvt = lapic_word(0x320);
                let timer_initial_count = lapic_word(0x380);
                if timer_lvt & (1 << 16) == 0
                    && (timer_initial_count != 0 || timer_lvt & (3 << 17) != 0)
                    || vcpu
                        .msrs
                        .iter()
                        .any(|msr| msr.index == 0x6e0 && msr.data != 0)
                {
                    return Err(SnapshotError::Topology {
                        message: "coarse snapshot contains an unsupported hardware timer"
                            .to_string(),
                    });
                }
            }
        }
        self.validate_kernel_state()
    }

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
        if let SnapshotKernelPitState::Present(pit) = &self.pit {
            for channel in &pit.channels {
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
            if pit.flags
                & !(kvm_bindings::KVM_PIT_FLAGS_HPET_LEGACY
                    | kvm_bindings::KVM_PIT_FLAGS_SPEAKER_DATA_ON)
                != 0
                || pit.reserved.iter().any(|value| *value != 0)
            {
                return ArchitectureFieldSnafu { field: "pit.flags" }.fail();
            }
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
        params
            .boundary_state
            .validate()
            .map_err(|error| SnapshotError::Topology {
                message: format!("snapshot boundary preflight failed: {error:?}"),
            })?;
        params
            .execution_descriptor
            .validate()
            .map_err(|error| SnapshotError::Topology {
                message: format!("snapshot execution preflight failed: {error:?}"),
            })?;
        params
            .scheduler_snapshot
            .validate()
            .map_err(|error| SnapshotError::Topology {
                message: format!("snapshot scheduler preflight failed: {error:?}"),
            })?;
        if params.scheduler_snapshot.state.progress_mode
            != params.execution_descriptor.progress_mode
            || !params
                .scheduler_snapshot
                .state
                .progress_budget()
                .same_unit(params.execution_descriptor.initial_budget)
        {
            return Err(SnapshotError::Topology {
                message: "snapshot scheduler execution descriptor mismatch".to_string(),
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

        // Coarse topology has no in-kernel PIT; do not query a nonexistent device.
        let pit = if params.execution_descriptor.kernel_pit_present() {
            SnapshotKernelPitState::Present(vm.get_pit2().context(GetPitSnafu)?)
        } else {
            SnapshotKernelPitState::Absent
        };
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
            completeness_profile: SNAPSHOT_PROFILE_EXACT_X86_KVM_V2.to_string(),
            inventory: build_snapshot_inventory(&params.topology),
            topology: params.topology,
            execution_descriptor: params.execution_descriptor,
        };

        let snapshot = Self {
            metadata: Some(metadata),
            boundary_state: params.boundary_state,
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
        };
        snapshot.validate_retained_boundary()?;
        Ok(snapshot)
    }

    /// Validate retained exact architecture/completeness without allocating KVM
    /// state. The file-session shell uses this same restore preflight.
    pub fn validate_kernel_restore(&self, vcpu_count: usize) -> Result<(), SnapshotError> {
        if vcpu_count != self.vcpu_snapshots.len() {
            return VcpuCountMismatchSnafu {
                snapshot: self.vcpu_snapshots.len(),
                current: vcpu_count,
            }
            .fail();
        }
        self.validate_retained_boundary()?;
        self.execution_descriptor()
            .and_then(SnapshotExecutionDescriptor::validate_current_host)
            .map_err(|error| SnapshotError::Topology {
                message: format!("snapshot host preflight failed: {error:?}"),
            })
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
        self.restore_cpuid(vcpus)?;
        self.restore_after_cpuid(vcpus, vm, guest_memory)
    }

    /// Caller has completed pure preflight and installed CPUID as its first
    /// admitted mutation. A later failure must poison/discard that runner.
    pub(crate) fn restore_after_cpuid(
        &self,
        vcpus: &[VcpuFd],
        vm: &VmFd,
        guest_memory: &GuestMemoryMmap,
    ) -> Result<(), SnapshotError> {
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
        self.restore_cpuid(vcpus)?;
        self.restore_devices_only_after_cpuid(vcpus, vm)
    }

    pub(crate) fn restore_cpuid(&self, vcpus: &[VcpuFd]) -> Result<(), SnapshotError> {
        if vcpus.len() != self.vcpu_snapshots.len() {
            return VcpuCountMismatchSnafu {
                snapshot: self.vcpu_snapshots.len(),
                current: vcpus.len(),
            }
            .fail();
        }
        for (snapshot, vcpu) in self.vcpu_snapshots.iter().zip(vcpus) {
            snapshot.restore_cpuid(vcpu)?;
        }
        Ok(())
    }

    pub(crate) fn restore_devices_only_after_cpuid(
        &self,
        vcpus: &[VcpuFd],
        vm: &VmFd,
    ) -> Result<(), SnapshotError> {
        self.restore_kernel_state_validated(vcpus, vm)
    }

    fn restore_kernel_state_validated(
        &self,
        vcpus: &[VcpuFd],
        vm: &VmFd,
    ) -> Result<(), SnapshotError> {
        // Restore in-kernel devices BEFORE vCPU state
        // The admitted clock/device policy is retained exactly; no restore-time
        // normalization can silently alter a snapshot's device identity.
        if let SnapshotKernelPitState::Present(pit) = &self.pit {
            vm.set_pit2(pit).context(SetPitSnafu)?;
        }
        vm.set_clock(&self.clock).context(SetClockSnafu)?;
        vm.set_irqchip(&self.pic_master).context(SetIrqchipSnafu)?;
        vm.set_irqchip(&self.pic_slave).context(SetIrqchipSnafu)?;
        vm.set_irqchip(&self.ioapic).context(SetIrqchipSnafu)?;

        // Restore per-vCPU state
        for (vcpu_snap, vcpu) in self.vcpu_snapshots.iter().zip(vcpus.iter()) {
            vcpu_snap.restore_after_cpuid(vcpu)?;
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
    #[snafu(display("Failed to get guest CPUID"))]
    GetVcpuCpuid { source: kvm_ioctls::Error },
    #[snafu(display("Failed to restore guest CPUID"))]
    SetVcpuCpuid { source: kvm_ioctls::Error },
    #[snafu(display("Failed to allocate guest CPUID buffer"))]
    CpuidBuffer,
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

    fn coarse_descriptor() -> SnapshotExecutionDescriptor {
        SnapshotExecutionDescriptor::portable_fixture(&crate::vm::VmConfig {
            num_vcpus: 2,
            smp_progress_mode: ProgressMode::KvmDebugBoundary,
            smp_progress_budget: ProgressBudget::KvmBoundaries {
                profile_id: BoundaryProfileId::KvmDebugExitBoundaryV1,
                count: 7,
            },
            ..crate::vm::VmConfig::default()
        })
        .unwrap()
    }

    fn settled_boundary() -> SnapshotBoundaryState {
        SnapshotBoundaryState {
            pending_io: SnapshotPendingIoState::Settled,
            delivery_state: SnapshotDeliveryState::CapturedKernelAndUserspace,
            injection_ledger: InjectionLedger::new(),
        }
    }

    fn injection_fact() -> InjectionRecord {
        InjectionRecord {
            target_vcpu: Some(1),
            kind: InjectionKind::Nmi,
            source: InjectionSource::Shell,
            requested_state_id: [0x42; 32],
            accepted: false,
        }
    }

    #[test]
    fn host_cpuid_identity_ignores_only_architectural_placement_ids() {
        for function in [1, 0xB, 0x1F, 0x8000_001E] {
            let mut first = SnapshotCpuidEntry {
                function,
                index: 0,
                flags: 1,
                eax: 0x1234,
                ebx: 0x0810_0201,
                ecx: 0x0302,
                edx: 0x5678,
            };
            let mut migrated = first;
            match function {
                1 => migrated.ebx ^= 0xff00_0000,
                0xB | 0x1F => migrated.edx ^= 0xffff,
                0x8000_001E => {
                    migrated.eax ^= 0xffff;
                    migrated.ebx ^= 0xff;
                    migrated.ecx ^= 0xff;
                }
                _ => unreachable!(),
            }
            first.canonicalize_host_placement();
            migrated.canonicalize_host_placement();
            assert_eq!(first, migrated, "placement leaf {function:x}");
            let mut incompatible = first;
            match function {
                1 => incompatible.ecx ^= 1,                // actual feature change
                0xB | 0x1F => incompatible.ebx ^= 1,       // topology count
                0x8000_001E => incompatible.ebx ^= 1 << 8, // threads/core
                _ => unreachable!(),
            }
            incompatible.canonicalize_host_placement();
            assert_ne!(first, incompatible, "capability leaf {function:x}");
        }
        let mut feature = SnapshotCpuidEntry {
            function: 7,
            index: 0,
            flags: 0,
            eax: 0,
            ebx: 1,
            ecx: 2,
            edx: 3,
        };
        let original = feature;
        feature.canonicalize_host_placement();
        assert_eq!(feature, original);
        feature.edx ^= 1;
        feature.canonicalize_host_placement();
        assert_ne!(feature, original);
    }

    #[test]
    fn cross_ccd_host_admission_preserves_raw_identity_and_rejects_feature_changes() {
        // Architectural mechanism fixture, deliberately NOT a measured host.
        let mut source = coarse_descriptor();
        source.host.native_cpuid = vec![
            SnapshotCpuidEntry {
                function: 0,
                index: 0,
                flags: 0,
                eax: 0x20,
                ebx: u32::from_le_bytes(*b"Auth"),
                edx: u32::from_le_bytes(*b"enti"),
                ecx: u32::from_le_bytes(*b"cAMD"),
            },
            SnapshotCpuidEntry {
                function: 1,
                index: 0,
                flags: 0,
                eax: 0xB40F40,
                ebx: 0x0108_0000,
                ecx: 1,
                edx: 1,
            },
        ];
        source.host.kvm_supported_cpuid = vec![
            SnapshotCpuidEntry {
                function: 0x8000_0006,
                index: 0,
                flags: 0,
                eax: 0,
                ebx: 0,
                ecx: 0,
                edx: 0x0600_9140,
            },
            SnapshotCpuidEntry {
                function: 0x8000_001D,
                index: 3,
                flags: 1,
                eax: (3 << 5) | 3,
                ebx: 0x03C0_003F,
                ecx: 0x17FFF,
                edx: 0,
            },
        ];
        let mut migrated = source.clone();
        migrated.host.native_cpuid[1].ebx ^= 0xff00_0000;
        migrated.host.kvm_supported_cpuid[0].edx = 0x0200_9140;
        migrated.host.kvm_supported_cpuid[1].ecx = 0x7FFF;
        assert_ne!(source, migrated, "raw provenance remains exact");
        assert!(source.restore_compatible(&migrated));
        assert_eq!(
            migrated.validate_current_host(),
            Err(SnapshotPreflightError::HostMismatch)
        );
        for corrupt in [
            (|host: &mut SnapshotHostIdentity| host.native_cpuid[1].eax ^= 1)
                as fn(&mut SnapshotHostIdentity),
            |host| host.native_cpuid[1].ecx ^= 1,
            |host| host.kernel_release.push('x'),
            |host| host.kvm_capabilities.push((1, 1)),
            |host| host.kvm_supported_cpuid[0].edx ^= 1, // cache line/association, NOT capacity
            |host| host.kvm_supported_cpuid[1].ebx ^= 1,
            |host| host.kvm_supported_cpuid[1].eax ^= 1 << 14, // sharing topology
        ] {
            let mut incompatible = migrated.clone();
            corrupt(&mut incompatible.host);
            assert!(!source.restore_compatible(&incompatible));
        }
        // The deterministic-cache exception is level-qualified, not index-based.
        let mut l2 = source.host.kvm_supported_cpuid[1];
        l2.eax = (2 << 5) | 3;
        let mut different_l2 = l2;
        different_l2.ecx ^= 1;
        assert_ne!(
            l2.execution_capability(true),
            different_l2.execution_capability(true)
        );
    }

    #[test]
    fn execution_descriptor_round_trip_binds_boundary_profile_and_clock_policy() {
        let descriptor = coarse_descriptor();
        assert_eq!(descriptor.validate(), Ok(()));
        let encoded = serde_json::to_vec(&descriptor).unwrap();
        let decoded: SnapshotExecutionDescriptor = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, descriptor);
        assert_eq!(decoded.validate(), Ok(()));
        assert_eq!(
            decoded.validate_current_host(),
            Err(SnapshotPreflightError::HostMismatch)
        );

        let strict =
            SnapshotExecutionDescriptor::portable_fixture(&crate::vm::VmConfig::default()).unwrap();
        assert_eq!(strict.validate(), Ok(()));
        assert_eq!(strict.boundary_profile, None);
        assert!(matches!(
            strict.initial_budget,
            ProgressBudget::Instructions(_)
        ));
    }

    #[test]
    fn execution_descriptor_rejects_mixed_units_profile_tables_and_unsupported_clocks() {
        let corruptions: &[fn(&mut SnapshotExecutionDescriptor)] = &[
            |descriptor| descriptor.schema_version = 0,
            |descriptor| descriptor.progress_mode = ProgressMode::ExactSingleStep,
            |descriptor| descriptor.initial_budget = ProgressBudget::Instructions(7),
            |descriptor| descriptor.boundary_profile = None,
            |descriptor| descriptor.boundary_profile.as_mut().unwrap().schema_version += 1,
            |descriptor| descriptor.boundary_profile.as_mut().unwrap().table_identity[0] ^= 1,
            |descriptor| descriptor.cpu_policy.hide_tsc = false,
            |descriptor| descriptor.cpu_policy.hide_hypervisor = false,
            |descriptor| descriptor.coarse_msr_filter = None,
            |descriptor| descriptor.safe_projection = SnapshotSafeProjection::FullState,
            |descriptor| {
                descriptor.clock_device_policy = SnapshotClockDevicePolicy::VirtualTscPerExit
            },
            |descriptor| {
                descriptor.clock_device_policy = SnapshotClockDevicePolicy::DeterministicJiffies {
                    in_kernel_pit_present: true,
                    lapic_timer_supported: false,
                }
            },
            |descriptor| {
                descriptor.clock_device_policy = SnapshotClockDevicePolicy::DeterministicJiffies {
                    in_kernel_pit_present: false,
                    lapic_timer_supported: true,
                }
            },
            |descriptor| descriptor.host.origin = SnapshotHostOrigin::Measured,
        ];
        for corrupt in corruptions {
            let mut descriptor = coarse_descriptor();
            corrupt(&mut descriptor);
            assert!(descriptor.validate().is_err());
        }
    }

    #[test]
    fn coarse_msr_filter_profile_binds_the_complete_default_allow_deny_table() {
        let profile = SnapshotMsrFilterProfile::KvmCoarseClockDenyV1;
        assert_eq!(profile.schema_version(), 1);
        assert_eq!(
            profile.ranges(),
            &[
                (0x10, 1, 3),
                (0x11, 2, 3),
                (0x3b, 1, 3),
                (0xe7, 2, 3),
                (0x6e0, 1, 3),
                (0x832, 1, 2),
                (0x838, 1, 2),
                (0x4000_0000, 0x1000, 3),
                (0x4b56_4d00, 0x100, 3),
            ]
        );
        assert_ne!(profile.table_identity(), [0; 32]);
        let descriptor = coarse_descriptor();
        assert_eq!(descriptor.coarse_msr_filter, Some(profile));
        assert_eq!(
            descriptor.safe_projection,
            SnapshotSafeProjection::ExcludeGuardedCoarseClockSamplesV2
        );
        assert!(!descriptor.permits_coarse_clock_projection());
        let mut tampered = serde_json::to_value(&descriptor).unwrap();
        tampered["coarse_msr_filter"] = serde_json::json!("KvmCoarseClockDenyV2");
        assert!(serde_json::from_value::<SnapshotExecutionDescriptor>(tampered).is_err());
        let mut missing = serde_json::to_value(&descriptor).unwrap();
        missing.as_object_mut().unwrap().remove("coarse_msr_filter");
        let missing: SnapshotExecutionDescriptor = serde_json::from_value(missing).unwrap();
        assert_eq!(
            missing.validate(),
            Err(SnapshotPreflightError::ExecutionDescriptorMismatch)
        );
        let mut missing_projection = serde_json::to_value(descriptor).unwrap();
        missing_projection
            .as_object_mut()
            .unwrap()
            .remove("safe_projection");
        assert!(serde_json::from_value::<SnapshotExecutionDescriptor>(missing_projection).is_err());
    }

    #[test]
    fn metadata_rejects_copied_execution_descriptors_and_missing_required_binding() {
        let topology = test_topology();
        let metadata = exact_metadata(&topology);
        let mut different = metadata.execution_descriptor.clone();
        different.cpu_policy.tsc_khz += 1;
        assert_eq!(
            validate_snapshot_metadata(Some(&metadata), &topology, &different),
            Err(SnapshotPreflightError::ExecutionDescriptorMismatch)
        );
        let mut encoded = serde_json::to_value(metadata).unwrap();
        encoded
            .as_object_mut()
            .unwrap()
            .remove("execution_descriptor");
        assert!(serde_json::from_value::<SnapshotMetadata>(encoded).is_err());
    }

    #[test]
    fn boundary_state_requires_settled_io_available_delivery_and_valid_prefix() {
        let boundary = settled_boundary();
        assert_eq!(boundary.validate(), Ok(()));
        let encoded = serde_json::to_vec(&boundary).unwrap();
        let decoded: SnapshotBoundaryState = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, boundary);
        let mut pending = boundary.clone();
        pending.pending_io = SnapshotPendingIoState::Pending { vcpu: 1 };
        assert_eq!(
            pending.validate(),
            Err(SnapshotPreflightError::PendingIoIncomplete)
        );
        let mut unavailable = boundary.clone();
        unavailable.delivery_state = SnapshotDeliveryState::Incomplete;
        assert_eq!(
            unavailable.validate(),
            Err(SnapshotPreflightError::DeliveryStateIncomplete)
        );
        let mut forged = boundary.clone();
        forged.injection_ledger.prefix_digest = [0; 32];
        assert_eq!(
            forged.validate(),
            Err(SnapshotPreflightError::InjectionLedgerInvalid)
        );
        for field in ["pending_io", "delivery_state", "injection_ledger"] {
            let mut value = serde_json::to_value(&boundary).unwrap();
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<SnapshotBoundaryState>(value).is_err());
        }
    }

    #[test]
    fn injection_prefix_binds_request_acceptance_target_source_and_order_without_delivery_claims() {
        let request = injection_fact();
        let mut prefix = InjectionLedger::new();
        prefix.record(&request).unwrap();
        assert_eq!(prefix.sequence, 1);
        let changes: &[fn(&mut InjectionRecord)] = &[
            |fact| fact.target_vcpu = Some(0),
            |fact| {
                fact.kind = InjectionKind::Irq {
                    irq: 5,
                    level: true,
                }
            },
            |fact| fact.source = InjectionSource::Pit,
            |fact| fact.requested_state_id[0] ^= 1,
            |fact| fact.accepted = true,
        ];
        for change in changes {
            let mut changed = request.clone();
            change(&mut changed);
            let mut different = InjectionLedger::new();
            different.record(&changed).unwrap();
            assert_ne!(different.prefix_digest, prefix.prefix_digest);
            let mut forward = prefix.clone();
            forward.record(&changed).unwrap();
            different.record(&request).unwrap();
            assert_ne!(forward.prefix_digest, different.prefix_digest);
        }
        let mut exhausted = prefix;
        exhausted.sequence = u64::MAX;
        let before = exhausted.clone();
        assert_eq!(
            exhausted.record(&request),
            Err(SnapshotPreflightError::InjectionLedgerInvalid)
        );
        assert_eq!(exhausted, before);
    }

    fn complete_boundary_snapshot() -> VmSnapshot {
        let descriptor = coarse_descriptor();
        let topology = SnapshotTopology {
            vcpu_count: 2,
            msr_indices: TEST_MSR_INDICES.to_vec(),
            virtio_devices: Vec::new(),
        };
        let scheduler = crate::scheduler::VcpuScheduler::try_new(
            &crate::scheduler::SchedulerConfig {
                num_vcpus: 2,
                progress_budget: descriptor.initial_budget,
                strategy: crate::scheduler::SchedulingStrategy::RoundRobin,
                seed: 0,
            },
            descriptor.progress_mode,
            vec![true; 2],
        )
        .unwrap();
        VmSnapshot {
            metadata: Some(SnapshotMetadata {
                state_schema_version: SNAPSHOT_STATE_SCHEMA_VERSION,
                completeness_profile: SNAPSHOT_PROFILE_EXACT_X86_KVM_V2.to_string(),
                inventory: build_snapshot_inventory(&topology),
                topology,
                execution_descriptor: descriptor,
            }),
            boundary_state: settled_boundary(),
            vcpu_snapshots: vec![complete_vcpu_snapshot(); 2],
            pic_master: kvm_irqchip {
                chip_id: KVM_IRQCHIP_PIC_MASTER,
                ..Default::default()
            },
            pic_slave: kvm_irqchip {
                chip_id: KVM_IRQCHIP_PIC_SLAVE,
                ..Default::default()
            },
            ioapic: kvm_irqchip {
                chip_id: KVM_IRQCHIP_IOAPIC,
                ..Default::default()
            },
            pit: SnapshotKernelPitState::Absent,
            clock: Default::default(),
            memory: SnapshotMemory::Full(vec![0; PAGE_SIZE]),
            serial_state: Default::default(),
            entropy: crate::devices::entropy::DeterministicEntropy::new(0).snapshot(),
            virtual_tsc: 0,
            exit_count: 0,
            io_exit_count: 0,
            exits_since_last_sdk: 0,
            panic_detected: false,
            panic_match_state: 0,
            pit_snapshot: crate::devices::pit::DeterministicPit::new(crate::cpu::DEFAULT_TSC_KHZ)
                .snapshot(),
            last_kvm_pit_mode: 0,
            fault_engine_snapshot: chaoscontrol_fault::engine::FaultEngine::new(Default::default())
                .snapshot(),
            virtio_snapshots: Vec::new(),
            coverage_active: false,
            scheduler_snapshot: scheduler.snapshot(),
            hlt_latched_vcpus: vec![false; 2],
        }
    }

    #[test]
    fn kernel_pit_presence_and_inhibition_are_exact_retained_state() {
        let coarse = complete_boundary_snapshot();
        assert!(matches!(coarse.pit, SnapshotKernelPitState::Absent));
        coarse.validate_retained_boundary().unwrap();
        assert_eq!(serde_json::to_value(&coarse).unwrap()["pit"], "Absent");

        let mut strict = coarse.clone();
        let config = crate::vm::VmConfig {
            num_vcpus: 2,
            ..Default::default()
        };
        let descriptor = SnapshotExecutionDescriptor::portable_fixture(&config).unwrap();
        strict.scheduler_snapshot = crate::scheduler::VcpuScheduler::try_new(
            &crate::scheduler::SchedulerConfig {
                num_vcpus: 2,
                progress_budget: descriptor.initial_budget,
                strategy: crate::scheduler::SchedulingStrategy::RoundRobin,
                seed: 0,
            },
            descriptor.progress_mode,
            vec![true; 2],
        )
        .unwrap()
        .snapshot();
        strict.metadata.as_mut().unwrap().execution_descriptor = descriptor;
        assert!(strict.validate_retained_boundary().is_err());
        strict.pit = SnapshotKernelPitState::Present(kvm_pit_state2 {
            flags: kvm_bindings::KVM_PIT_FLAGS_HPET_LEGACY,
            ..Default::default()
        });
        strict.validate_retained_boundary().unwrap();
        let encoded = serde_json::to_value(&strict).unwrap();
        let decoded: VmSnapshot = serde_json::from_value(encoded.clone()).unwrap();
        decoded.validate_retained_boundary().unwrap();
        assert_eq!(
            serde_json::to_value(decoded.continuation_view(false).unwrap()).unwrap()["pit"],
            encoded["pit"]
        );
        let SnapshotKernelPitState::Present(pit) = &mut strict.pit else {
            unreachable!()
        };
        pit.flags = 0;
        assert!(strict.validate_retained_boundary().is_err());
    }

    #[test]
    fn borrowed_continuation_view_preserves_every_retained_field_without_projection() {
        let mut snapshot = complete_boundary_snapshot();
        for overlay in [false, true] {
            if overlay {
                snapshot.memory = SnapshotMemory::Overlay {
                    base: Arc::new(vec![0; PAGE_SIZE]),
                    dirty_pages: BTreeMap::from([(0, Box::new([0x5a; PAGE_SIZE]))]),
                };
            }
            let raw = serde_json::to_value(&snapshot).unwrap();
            let borrowed =
                serde_json::to_value(snapshot.continuation_view(false).unwrap()).unwrap();
            for (name, value) in raw.as_object().unwrap() {
                assert_eq!(borrowed.get(name), Some(value), "borrowed field {name}");
            }
            assert_eq!(
                borrowed.as_object().unwrap().len(),
                raw.as_object().unwrap().len() + 4
            );
            assert_eq!(
                borrowed["continuation_domain"],
                SNAPSHOT_CONTINUATION_DOMAIN
            );
            assert_eq!(
                borrowed["continuation_schema_version"],
                SNAPSHOT_CONTINUATION_SCHEMA_VERSION
            );
            assert_eq!(borrowed["projection"], "FullState");
        }
        assert!(matches!(
            snapshot.continuation_view(true),
            Err(SnapshotPreflightError::ExecutionDescriptorMismatch)
        ));
    }

    fn private_coarse_projection(snapshot: &VmSnapshot) -> serde_json::Value {
        // Serializer mechanics only: this portable fixture has no runtime authority.
        serde_json::to_value(VmSnapshotContinuation {
            snapshot,
            project_coarse_clock_samples: true,
        })
        .unwrap()
    }

    fn clock_sample_snapshot() -> VmSnapshot {
        let mut snapshot = complete_boundary_snapshot();
        for vcpu in &mut snapshot.vcpu_snapshots {
            vcpu.msrs.extend([
                MsrSnapshot {
                    index: 0x4000_0010,
                    data: 11,
                },
                MsrSnapshot {
                    index: 0x4000_0020,
                    data: 22,
                },
                MsrSnapshot {
                    index: 0x4000_0021,
                    data: 33,
                },
            ]);
            vcpu.msrs.extend(
                [
                    0x11,
                    0x12,
                    0x3b,
                    0xe7,
                    0xe8,
                    0x6e0,
                    0x832,
                    0x838,
                    0x4000_0000,
                    0x4b56_4d00,
                ]
                .into_iter()
                .map(|index| MsrSnapshot { index, data: 0 }),
            );
            vcpu.msrs.sort_unstable_by_key(|msr| msr.index);
        }
        let metadata = snapshot.metadata.as_mut().unwrap();
        metadata.topology.msr_indices = snapshot.vcpu_snapshots[0]
            .msrs
            .iter()
            .map(|msr| msr.index)
            .collect();
        metadata.inventory = build_snapshot_inventory(&metadata.topology);
        snapshot
    }

    #[test]
    fn approved_clock_sample_differences_compare_equal_without_changing_raw_state() {
        let original = clock_sample_snapshot();
        let projected = private_coarse_projection(&original);
        let projection = SnapshotSafeProjection::ExcludeGuardedCoarseClockSamplesV2;
        assert_eq!(
            projection.msr_data_members(),
            &[0x10, 0x4000_0010, 0x4000_0020]
        );
        assert_eq!(
            projected["projection"],
            serde_json::to_value(projection).unwrap()
        );
        assert_eq!(
            projected["projection_table_identity"],
            serde_json::to_value(projection.table_identity()).unwrap()
        );
        assert_ne!(
            projection.table_identity(),
            Some(SnapshotMsrFilterProfile::KvmCoarseClockDenyV1.table_identity())
        );
        assert_eq!(SnapshotSafeProjection::FullState.table_identity(), None);
        for overlay in [false, true] {
            let mut control = original.clone();
            if overlay {
                control.memory = SnapshotMemory::Overlay {
                    base: Arc::new(vec![0; PAGE_SIZE]),
                    dirty_pages: BTreeMap::from([(0, Box::new([0x5a; PAGE_SIZE]))]),
                };
            }
            let control_raw = serde_json::to_vec(&control).unwrap();
            let mut changed = control.clone();
            changed.clock.clock = 42;
            for (vcpu, state) in changed.vcpu_snapshots.iter_mut().enumerate() {
                for msr in &mut state.msrs {
                    if projection.msr_data_members().contains(&msr.index) {
                        msr.data += 100 + vcpu as u64;
                    }
                }
            }
            let changed_raw = serde_json::to_vec(&changed).unwrap();
            assert_ne!(changed_raw, control_raw);
            assert_ne!(
                serde_json::to_value(control.continuation_view(false).unwrap()).unwrap(),
                serde_json::to_value(changed.continuation_view(false).unwrap()).unwrap()
            );
            assert_eq!(
                private_coarse_projection(&changed),
                private_coarse_projection(&control)
            );
            assert_eq!(serde_json::to_vec(&control).unwrap(), control_raw);
            assert_eq!(serde_json::to_vec(&changed).unwrap(), changed_raw);
            let restored: VmSnapshot = serde_json::from_slice(&changed_raw).unwrap();
            assert_eq!(serde_json::to_vec(&restored).unwrap(), changed_raw);
            let raw = serde_json::to_value(&changed).unwrap();
            let view = private_coarse_projection(&changed);
            for (name, value) in raw.as_object().unwrap() {
                if !["clock", "vcpu_snapshots"].contains(&name.as_str()) {
                    assert_eq!(view.get(name), Some(value), "retained field {name}");
                }
            }
            assert!(view["clock"].is_null());
            for (vcpu, state) in changed.vcpu_snapshots.iter().enumerate() {
                let samples = view["vcpu_snapshots"][vcpu]["msrs"].as_array().unwrap();
                assert_eq!(samples.len(), state.msrs.len());
                for (sample, msr) in samples.iter().zip(&state.msrs) {
                    assert_eq!(sample["index"], msr.index);
                    assert_eq!(
                        sample["data"].is_null(),
                        projection.msr_data_members().contains(&msr.index)
                    );
                }
                for (name, value) in raw["vcpu_snapshots"][vcpu].as_object().unwrap() {
                    if name != "msrs" {
                        assert_eq!(view["vcpu_snapshots"][vcpu].get(name), Some(value));
                    }
                }
            }
        }
    }

    #[test]
    fn coarse_projection_retains_other_msrs_indices_order_inventory_and_continuation_state() {
        let original = clock_sample_snapshot();
        let expected = private_coarse_projection(&original);
        for (vcpu, state) in original.vcpu_snapshots.iter().enumerate() {
            for (slot, msr) in state.msrs.iter().enumerate() {
                if !SnapshotSafeProjection::ExcludeGuardedCoarseClockSamplesV2
                    .msr_data_members()
                    .contains(&msr.index)
                {
                    let mut changed = original.clone();
                    changed.vcpu_snapshots[vcpu].msrs[slot].data ^= 1;
                    assert_ne!(private_coarse_projection(&changed), expected);
                }
            }
        }
        let changes: &[fn(&mut VmSnapshot)] = &[
            |state| state.vcpu_snapshots[0].msrs[1].data ^= 1,
            |state| state.vcpu_snapshots[1].msrs[4].data ^= 1,
            |state| state.vcpu_snapshots[0].msrs[0].index = 0x11,
            |state| state.vcpu_snapshots[0].msrs.swap(0, 2),
            |state| {
                state.vcpu_snapshots[0].msrs.pop();
            },
            |state| {
                state.metadata.as_mut().unwrap().topology.msr_indices.pop();
            },
            |state| state.vcpu_snapshots[0].regs.rax ^= 1,
            |state| state.vcpu_snapshots[1].events.nmi.pending = 1,
            |state| state.vcpu_snapshots[1].lapic.regs[0x200] = 1,
            |state| state.virtual_tsc += 1,
            |state| state.exit_count += 1,
            |state| state.hlt_latched_vcpus[1] = true,
            |state| state.memory = SnapshotMemory::Full(vec![1; PAGE_SIZE]),
        ];
        for change in changes {
            let mut changed = original.clone();
            change(&mut changed);
            assert_ne!(private_coarse_projection(&changed), expected);
        }
    }

    #[test]
    fn superseded_projection_names_are_not_deserialized_as_current_authority() {
        for stale in [
            "ExcludeInaccessibleKernelClockV1",
            "ExcludeGuardedCoarseClockSamplesV1",
        ] {
            let mut descriptor = serde_json::to_value(coarse_descriptor()).unwrap();
            descriptor["safe_projection"] = serde_json::json!(stale);
            assert!(serde_json::from_value::<SnapshotExecutionDescriptor>(descriptor).is_err());
        }
    }

    #[test]
    fn complete_boundary_snapshot_round_trips_available_pending_architecture() {
        let mut snapshot = complete_boundary_snapshot();
        snapshot.vcpu_snapshots[1].events.flags = kvm_bindings::KVM_VCPUEVENT_VALID_NMI_PENDING;
        snapshot.vcpu_snapshots[1].events.nmi.pending = 1;
        snapshot.vcpu_snapshots[1].lapic.regs[0x200] = 1;
        snapshot
            .boundary_state
            .injection_ledger
            .record(&injection_fact())
            .unwrap();
        snapshot.validate_retained_boundary().unwrap();
        let encoded = serde_json::to_vec(&snapshot).unwrap();
        let decoded: VmSnapshot = serde_json::from_slice(&encoded).unwrap();
        decoded.validate_retained_boundary().unwrap();
        assert_eq!(decoded.vcpu_snapshots[1].events.nmi.pending, 1);
        assert_eq!(decoded.vcpu_snapshots[1].lapic.regs[0x200], 1);
        assert_eq!(decoded.boundary_state, snapshot.boundary_state);
        // Mechanism-only portable state cannot masquerade as measured host admission.
        assert!(decoded.validate_kernel_restore(2).is_err());
    }

    #[test]
    fn retained_boundary_rejects_incomplete_and_unsupported_state_before_host_access() {
        let corruptions: &[fn(&mut VmSnapshot)] = &[
            |snapshot| {
                snapshot.boundary_state.pending_io = SnapshotPendingIoState::Pending { vcpu: 0 }
            },
            |snapshot| snapshot.boundary_state.delivery_state = SnapshotDeliveryState::Incomplete,
            |snapshot| {
                snapshot.hlt_latched_vcpus.pop().unwrap();
            },
            |snapshot| {
                snapshot.vcpu_snapshots[0].msrs.pop().unwrap();
            },
            |snapshot| snapshot.vcpu_snapshots[0].events.interrupt.shadow = 4,
            |snapshot| snapshot.pit = SnapshotKernelPitState::Present(Default::default()),
            |snapshot| snapshot.vcpu_snapshots[0].lapic.regs[0x380] = 1,
            |snapshot| {
                snapshot
                    .metadata
                    .as_mut()
                    .unwrap()
                    .execution_descriptor
                    .boundary_profile
                    .as_mut()
                    .unwrap()
                    .table_identity[0] ^= 1
            },
            |snapshot| {
                snapshot.scheduler_snapshot.state_id =
                    crate::scheduler::core::ScheduleStateId([0; 32])
            },
        ];
        for corrupt in corruptions {
            let mut snapshot = complete_boundary_snapshot();
            corrupt(&mut snapshot);
            assert!(snapshot.validate_retained_boundary().is_err());
        }
        let snapshot = complete_boundary_snapshot();
        for field in ["boundary_state", "hlt_latched_vcpus"] {
            let mut value = serde_json::to_value(&snapshot).unwrap();
            value.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<VmSnapshot>(value).is_err());
        }
    }

    #[test]
    fn injection_suffix_validation_rejects_missing_reordered_and_forged_records() {
        let first = injection_fact();
        let mut second = first.clone();
        second.accepted = true;
        let prior = InjectionLedger::new();
        let mut expected = prior.clone();
        expected.record(&first).unwrap();
        expected.record(&second).unwrap();
        assert_eq!(
            validate_injection_records(&prior, &[first.clone(), second.clone()], &expected, 2),
            Ok(())
        );
        assert!(
            validate_injection_records(&prior, std::slice::from_ref(&first), &expected, 2).is_err()
        );
        assert!(
            validate_injection_records(&prior, &[second.clone(), first.clone()], &expected, 2)
                .is_err()
        );
        let mut out_of_range = first;
        out_of_range.target_vcpu = Some(2);
        assert!(
            validate_injection_records(&prior, &[out_of_range, second.clone()], &expected, 2)
                .is_err()
        );
        let mut unknown_delivery_claim = serde_json::to_value(second).unwrap();
        unknown_delivery_claim["delivered_at_instruction"] = serde_json::json!(42);
        assert!(serde_json::from_value::<InjectionRecord>(unknown_delivery_claim).is_err());
    }

    fn exact_metadata(topology: &SnapshotTopology) -> SnapshotMetadata {
        SnapshotMetadata {
            state_schema_version: SNAPSHOT_STATE_SCHEMA_VERSION,
            completeness_profile: SNAPSHOT_PROFILE_EXACT_X86_KVM_V2.to_string(),
            topology: topology.clone(),
            inventory: build_snapshot_inventory(topology),
            execution_descriptor: SnapshotExecutionDescriptor::portable_fixture(
                &crate::vm::VmConfig::default(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn exact_metadata_accepts_complete_matching_inventory() {
        let topology = test_topology();
        let metadata = exact_metadata(&topology);
        assert_eq!(
            validate_snapshot_metadata(Some(&metadata), &topology, &metadata.execution_descriptor),
            Ok(())
        );
    }

    #[test]
    fn exact_metadata_rejects_legacy_and_missing_components() {
        let topology = test_topology();
        assert_eq!(
            validate_snapshot_metadata(
                None,
                &topology,
                &exact_metadata(&topology).execution_descriptor
            ),
            Err(SnapshotPreflightError::LegacyOrIncomplete)
        );

        let mut metadata = exact_metadata(&topology);
        metadata.inventory.pop();
        assert_eq!(
            validate_snapshot_metadata(Some(&metadata), &topology, &metadata.execution_descriptor),
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
            validate_snapshot_metadata(Some(&metadata), &topology, &metadata.execution_descriptor),
            Err(SnapshotPreflightError::TopologyMismatch)
        );

        let mut duplicate_topology = topology.clone();
        duplicate_topology
            .virtio_devices
            .push(duplicate_topology.virtio_devices[0].clone());
        let metadata = exact_metadata(&duplicate_topology);
        assert!(matches!(
            validate_snapshot_metadata(
                Some(&metadata),
                &duplicate_topology,
                &metadata.execution_descriptor
            ),
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
            cpuid: vec![SnapshotCpuidEntry {
                function: 0,
                index: 0,
                flags: 0,
                eax: 0,
                ebx: 0,
                ecx: 0,
                edx: 0,
            }],
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
    fn vcpu_cpuid_is_required_bounded_and_fully_compared() {
        let snapshot = complete_vcpu_snapshot();
        let encoded = serde_json::to_value(&snapshot).unwrap();
        let mut missing = encoded.clone();
        missing.as_object_mut().unwrap().remove("cpuid");
        assert!(serde_json::from_value::<VcpuSnapshot>(missing).is_err());
        for table in [
            serde_json::json!([]),
            serde_json::json!([snapshot.cpuid[0], snapshot.cpuid[0]]),
        ] {
            let mut invalid = encoded.clone();
            invalid["cpuid"] = table;
            assert!(serde_json::from_value::<VcpuSnapshot>(invalid).is_err());
        }
        let mut invalid = encoded.clone();
        invalid["cpuid"][0]["flags"] = serde_json::json!(0x8000_0000u32);
        assert!(serde_json::from_value::<VcpuSnapshot>(invalid).is_err());
        let mut invalid = encoded;
        invalid["cpuid"] = serde_json::to_value(vec![
            snapshot.cpuid[0];
            kvm_bindings::KVM_MAX_CPUID_ENTRIES + 1
        ])
        .unwrap();
        assert!(serde_json::from_value::<VcpuSnapshot>(invalid).is_err());
        let mut full = complete_boundary_snapshot();
        let before = serde_json::to_value(full.continuation_view(false).unwrap()).unwrap();
        full.vcpu_snapshots[0].cpuid[0].ecx ^= 1;
        assert_ne!(
            serde_json::to_value(full.continuation_view(false).unwrap()).unwrap(),
            before
        );
    }

    #[test]
    fn saved_cpuid_policy_accepts_only_runtime_bits_derived_from_captured_state() {
        // Pure policy mechanism: no fixture claims measured execution origin.
        let entry = |function, index, eax, ebx, ecx, edx| kvm_bindings::kvm_cpuid_entry2 {
            function,
            index,
            flags: u32::from(index != 0),
            eax,
            ebx,
            ecx,
            edx,
            padding: [0; 3],
        };
        let entries = [
            entry(1, 0, 0, 0, 1 << 26, 0),
            entry(7, 0, 0, 0, 1 << 3, 0),
            entry(0xD, 0, 7, 832, 832, 0),
            entry(0xD, 1, 2, 832, 0, 0),
            entry(0xD, 2, 256, 576, 2, 0),
        ];
        let mut state = complete_vcpu_snapshot();
        state.sregs.cr4 = (1 << 18) | (1 << 22);
        state.sregs.apic_base = 1 << 11;
        state.xcrs.nr_xcrs = 1;
        state.xcrs.xcrs[0].xcr = 0;
        state.xcrs.xcrs[0].value = 7;
        let mut actual = kvm_bindings::CpuId::from_entries(&entries).unwrap();
        crate::cpu::patch_cpuid_apic_id_in_place(&mut actual, 1, 2);
        crate::cpu::apply_cpuid_runtime_state(
            &mut actual,
            state.sregs.cr4,
            state.sregs.apic_base,
            7,
            0,
        )
        .unwrap();
        state.cpuid = actual
            .as_slice()
            .iter()
            .map(SnapshotCpuidEntry::from_kvm_entry)
            .collect();
        let template = || kvm_bindings::CpuId::from_entries(&entries).unwrap();
        state
            .validate_guest_cpuid_template(template(), 1, 2)
            .unwrap();
        let corruptions: &[fn(&mut VcpuSnapshot)] = &[
            |state| state.cpuid[0].ecx ^= 1,       // unsupported feature change
            |state| state.cpuid[0].ecx ^= 1 << 27, // OSXSAVE must match CR4
            |state| state.cpuid[0].edx ^= 1 << 9,  // APIC must match APICBASE
            |state| state.cpuid[1].ecx ^= 1 << 4,  // OSPKE must match CR4
            |state| state.cpuid[2].ebx ^= 1,       // XSTATE bytes must match XCR0
            |state| state.xcrs.xcrs[0].value = 3,
            |state| state.cpuid[0].ebx ^= 1 << 24, // vCPU identity stays bound
        ];
        for corrupt in corruptions {
            let mut invalid = state.clone();
            corrupt(&mut invalid);
            assert!(invalid
                .validate_guest_cpuid_template(template(), 1, 2)
                .is_err());
        }
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
