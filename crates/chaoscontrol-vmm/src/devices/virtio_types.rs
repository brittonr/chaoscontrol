//! Shared typed limits, failures, and live-state views for virtio devices.

pub const MAX_QUEUE_SIZE: u16 = 256;
pub const MAX_QUEUE_SIZE_USIZE: usize = MAX_QUEUE_SIZE as usize;
pub const MAX_GUEST_MEMORY_REGIONS: usize = 16;
pub const DEFAULT_MAX_AGGREGATE_BYTES: u64 = 2 * 1024 * 1024;
pub const DEFAULT_MAX_BLOCK_TRANSFER_BYTES: u64 = 1024 * 1024;
pub const DEFAULT_MAX_NET_FRAME_BYTES: u64 = 64 * 1024;
pub const DEFAULT_MAX_NET_TX_PACKETS: usize = 256;
pub const DEFAULT_MAX_NET_TX_BYTES: u64 = 4 * 1024 * 1024;
pub const DEFAULT_MAX_ENTROPY_TRANSFER_BYTES: u64 = 64 * 1024;
pub const DEFAULT_SCRATCH_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtioLimits {
    pub max_queue_size: u16,
    pub max_chain_descriptors: u16,
    pub max_aggregate_bytes: u64,
    pub max_block_transfer_bytes: u64,
    pub max_net_frame_bytes: u64,
    pub max_net_tx_packets: usize,
    pub max_net_tx_bytes: u64,
    pub max_entropy_transfer_bytes: u64,
    pub scratch_bytes: usize,
}

impl Default for VirtioLimits {
    fn default() -> Self {
        Self {
            max_queue_size: MAX_QUEUE_SIZE,
            max_chain_descriptors: MAX_QUEUE_SIZE,
            max_aggregate_bytes: DEFAULT_MAX_AGGREGATE_BYTES,
            max_block_transfer_bytes: DEFAULT_MAX_BLOCK_TRANSFER_BYTES,
            max_net_frame_bytes: DEFAULT_MAX_NET_FRAME_BYTES,
            max_net_tx_packets: DEFAULT_MAX_NET_TX_PACKETS,
            max_net_tx_bytes: DEFAULT_MAX_NET_TX_BYTES,
            max_entropy_transfer_bytes: DEFAULT_MAX_ENTROPY_TRANSFER_BYTES,
            scratch_bytes: DEFAULT_SCRATCH_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportViolation {
    MmioWidth { actual: usize },
    MmioAddress { address: u64 },
    QueueSelection { selected: u32, available: usize },
    FeatureSelector { selected: u32 },
    FeaturesAfterAcceptance,
    QueueSizeWhileReady,
    QueueAddressWhileReady,
    ReadyValue { value: u32 },
    ReadyState,
    StatusTransition { current: u32, next: u32 },
    UnsupportedFeatures { requested: u64, offered: u64 },
    ModernFeatureMissing,
    NotifyQueueNotReady { selected: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueViolation {
    ZeroSize,
    SizeWidth { value: u32 },
    SizeNotPowerOfTwo { value: u16 },
    SizeAboveMaximum { value: u16, maximum: u16 },
    AddressMisaligned { address: u64, alignment: u64 },
    AddressOverflow,
    RingOutsideMemory { address: u64, length: u64 },
    RingOverlap,
    AvailableDelta { delta: u16, capacity: u16 },
    AvailableHead { head: u16, capacity: u16 },
    SnapshotCursorWithoutReady { last_avail: u16, next_used: u16 },
    SnapshotUsedIndexMismatch { snapshot: u16, guest: u16 },
    GuestMemoryRegions { actual: usize, maximum: usize },
    NotValidated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DescriptorViolation {
    HeadIndex { index: u16, capacity: u16 },
    NextIndex { index: u16, capacity: u16 },
    Cycle { index: u16 },
    CountLimit { count: u16, maximum: u16 },
    UnsupportedFlags { flags: u16 },
    AddressOverflow { address: u64, length: u32 },
    OutsideMemory { address: u64, length: u32 },
    AggregateOverflow,
    AggregateLimit { length: u64, maximum: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestViolation {
    DescriptorShape,
    HeaderDirection,
    HeaderLength { actual: u32 },
    HeaderReserved { value: u32 },
    StatusDirection,
    StatusLength { actual: u32 },
    DataDirection,
    EmptyTransfer,
    UnsupportedOperation { operation: u32 },
    TransferAlignment { length: u64, alignment: u64 },
    TransferLimit { length: u64, maximum: u64 },
    StorageOverflow,
    StorageOutsideDevice { end: u64, device_size: u64 },
    NetHeaderLength { actual: u32 },
    NetCapacity { available: u64, required: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceViolation {
    ScratchLimit { requested: usize, maximum: usize },
    ScratchExhausted,
    ScratchLease { slot: usize },
    ScratchLeak { count: usize },
    Allocation { requested: usize },
    RetainedPacketLimit { requested: usize, maximum: usize },
    RetainedPacketSlotsExhausted,
    RetainedByteLimit { requested: u64, maximum: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtioFailure {
    Transport(TransportViolation),
    Queue(QueueViolation),
    Descriptor(DescriptorViolation),
    Request(RequestViolation),
    Resource(ResourceViolation),
    GuestMemoryRead,
    GuestMemoryWrite,
    BackendRead,
    BackendWrite,
    UsedRingWrite,
    InterruptDelivery { irq: u32, asserted: bool },
    CompletionState,
    BackendQueue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsedWriteFailurePoint {
    BeforeIndex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LastRequestLiveOutcome {
    Completed {
        head_index: u16,
        written_length: u32,
    },
    Rejected {
        head_index: u16,
        written_length: u32,
        failure: VirtioFailure,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedQueueLiveConfig {
    pub size: u16,
    pub descriptor_address: u64,
    pub driver_address: u64,
    pub device_address: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingCompletionLiveState {
    pub head_index: u16,
    pub written_length: u32,
    pub backend_started: bool,
    pub effects_started: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueLiveState {
    pub config: Option<ValidatedQueueLiveConfig>,
    pub ready: bool,
    pub last_avail_idx: u16,
    pub next_used_idx: u16,
    pub failure: Option<VirtioFailure>,
    pub pending_completion: Option<PendingCompletionLiveState>,
    pub last_request_outcome: Option<LastRequestLiveOutcome>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtioLiveState {
    pub status: u32,
    pub interrupt_status: u32,
    pub failure: Option<VirtioFailure>,
    pub queues: Vec<QueueLiveState>,
}
