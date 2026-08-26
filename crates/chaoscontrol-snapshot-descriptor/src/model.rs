use serde::{Deserialize, Serialize};

pub const DESCRIPTOR_SCHEMA: &str = "chaoscontrol-snapshot-descriptor-v1";
pub const DESCRIPTOR_VERSION: u32 = 1;
pub const EXACT_SNAPSHOT_PROFILE: &str = "exact-x86-kvm-v1";
pub const EXACT_STATE_SCHEMA_VERSION: u32 = 2;
pub const EXACT_ARCHITECTURE: &str = "x86_64";
pub const CURRENT_PAYLOAD_CODEC: &str = "simulation-snapshot-cbor-zstd-v2";
pub const CHUNK_MANIFEST_CODEC: &str = "snapshot-chunk-manifest-v1";
pub const SNAPSHOT_CHUNK_CODEC: &str = "raw-snapshot-chunk-v1";
pub const STATE_OWNER_SCHEMA: &str = "chaoscontrol-state-owner-v1";
pub const VIRTIO_NETWORK_DEVICE_ID: u32 = 1;
pub const VIRTIO_BLOCK_DEVICE_ID: u32 = 2;
pub const VIRTIO_ENTROPY_DEVICE_ID: u32 = 4;

pub const MAX_TEXT_BYTES: usize = 256;
pub const MAX_KVM_OPERATIONS: usize = 32;
pub const MAX_MSR_INDICES: usize = 4096;
pub const MAX_VCPUS: u32 = 256;
pub const MAX_DEVICES: usize = 64;
pub const MAX_DEVICE_QUEUES: u32 = 64;
pub const MAX_STATE_OWNERS: usize = 1024;
pub const MAX_GUEST_ARTIFACTS: usize = 16;
pub const MAX_CLOSURE_MEMBERS: usize = 4096;
pub const MAX_LOCATOR_HINTS: usize = 32;
pub const MAX_LOCATOR_BYTES: usize = 2048;
pub const MAX_RESTORE_PHASES: usize = 32;
pub const MAX_CONTINUATION_STEPS: u64 = 1_000_000;
pub const MAX_MEMORY_BYTES: u64 = 1_u64 << 48;

pub const REQUIRED_KVM_OPERATIONS: [&str; 13] = [
    "kvm-clock-v1",
    "kvm-debug-registers-v1",
    "kvm-fpu-v1",
    "kvm-irqchip-v1",
    "kvm-lapic-v1",
    "kvm-mp-state-v1",
    "kvm-msrs-v1",
    "kvm-pit2-v1",
    "kvm-registers-v1",
    "kvm-special-registers-v1",
    "kvm-vcpu-events-v1",
    "kvm-xcrs-v1",
    "kvm-xsave-v1",
];

pub const FIXED_STATE_OWNERS: [&str; 14] = [
    "counters",
    "coverage",
    "deterministic-time",
    "entropy",
    "fault-engine",
    "guest-memory",
    "in-kernel-clock",
    "in-kernel-irqchip",
    "in-kernel-pit",
    "panic-detector",
    "scheduler",
    "serial",
    "timer",
    "vmm-determinism",
];

pub const RESTORE_NON_CLAIMS: [&str; 4] = [
    "descriptor validity does not grant artifact access or retention authority",
    "destination preflight does not prove KVM or guest correctness",
    "restore observations do not prove future replay or cross-host portability",
    "restore receipts do not grant execution, branch, promotion, or release authority",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DigestAlgorithm {
    Blake3,
    Sha256,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaggedDigest {
    pub algorithm: DigestAlgorithm,
    pub hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentIdentity {
    pub digest: TaggedDigest,
    pub length_bytes: u64,
    pub codec: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClosureKind {
    Monolithic,
    Chunked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClosureRole {
    SnapshotPayload,
    SnapshotChunk,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosureMember {
    pub order: u32,
    pub role: ClosureRole,
    pub content: ContentIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadClosure {
    pub kind: ClosureKind,
    pub logical_payload: ContentIdentity,
    pub manifest: Option<ContentIdentity>,
    pub members: Vec<ClosureMember>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceIdentity {
    pub base_address: u64,
    pub irq: u32,
    pub device_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceBackend {
    Network,
    Block,
    Entropy,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceCohort {
    pub identity: DeviceIdentity,
    pub queue_count: u32,
    pub backend: DeviceBackend,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotTopology {
    pub vcpu_count: u32,
    pub memory_bytes: u64,
    pub msr_indices: Vec<u32>,
    pub devices: Vec<DeviceCohort>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateOwner {
    pub owner_id: String,
    pub schema_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GuestArtifactRole {
    Kernel,
    Initrd,
    DiskImage,
    GuestBinary,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestArtifact {
    pub role: GuestArtifactRole,
    pub content: ContentIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCohort {
    pub runtime_build: TaggedDigest,
    pub kvm_operations: Vec<String>,
    pub scheduler_profile: String,
    pub time_profile: String,
    pub entropy_profile: String,
}

// r[impl chaoscontrol.snapshot_descriptor.contract]
// r[impl chaoscontrol.snapshot_descriptor.complete_cohort]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotDescriptor {
    pub schema: String,
    pub descriptor_version: u32,
    pub completeness_profile: String,
    pub state_schema_version: u32,
    pub architecture: String,
    pub runtime: RuntimeCohort,
    pub topology: SnapshotTopology,
    pub state_owners: Vec<StateOwner>,
    pub guest_artifacts: Vec<GuestArtifact>,
    pub payload: PayloadClosure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotDescriptorEnvelope {
    pub descriptor_id: TaggedDigest,
    pub descriptor: SnapshotDescriptor,
}
