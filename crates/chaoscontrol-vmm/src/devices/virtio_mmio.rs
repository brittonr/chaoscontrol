//! Virtio 1.2 MMIO transport with validated split-ring queue state.

use super::virtio_chain::{plan_descriptor_chain, DescriptorChainPlan, VirtqDesc};
use super::virtio_types::{
    LastRequestLiveOutcome, PendingCompletionLiveState, QueueLiveState, QueueViolation,
    TransportViolation, UsedWriteFailurePoint, ValidatedQueueLiveConfig, VirtioFailure,
    VirtioLimits, VirtioLiveState, MAX_GUEST_MEMORY_REGIONS, MAX_QUEUE_SIZE, MAX_QUEUE_SIZE_USIZE,
};
use super::virtio_validation::{
    available_element_address, descriptor_address, used_element_address, validate_available_delta,
    validate_queue_config, validate_queue_size, validate_restored_status,
    validate_status_transition, MemoryRegion, RawQueueConfig, ValidatedQueueConfig,
    VIRTIO_F_VERSION_1, VIRTIO_STATUS_DEVICE_NEEDS_RESET, VIRTIO_STATUS_DRIVER_OK,
    VIRTIO_STATUS_FEATURES_OK,
};
use vm_memory::{Address, Bytes, GuestAddress, GuestMemory, GuestMemoryMmap, GuestMemoryRegion};

pub const VIRTIO_MMIO_MAGIC_VALUE: u64 = 0x000;
pub const VIRTIO_MMIO_VERSION: u64 = 0x004;
pub const VIRTIO_MMIO_DEVICE_ID: u64 = 0x008;
pub const VIRTIO_MMIO_VENDOR_ID: u64 = 0x00C;
pub const VIRTIO_MMIO_DEVICE_FEATURES: u64 = 0x010;
pub const VIRTIO_MMIO_DEVICE_FEATURES_SEL: u64 = 0x014;
pub const VIRTIO_MMIO_DRIVER_FEATURES: u64 = 0x020;
pub const VIRTIO_MMIO_DRIVER_FEATURES_SEL: u64 = 0x024;
pub const VIRTIO_MMIO_QUEUE_SEL: u64 = 0x030;
pub const VIRTIO_MMIO_QUEUE_NUM_MAX: u64 = 0x034;
pub const VIRTIO_MMIO_QUEUE_NUM: u64 = 0x038;
pub const VIRTIO_MMIO_QUEUE_READY: u64 = 0x044;
pub const VIRTIO_MMIO_QUEUE_NOTIFY: u64 = 0x050;
pub const VIRTIO_MMIO_INTERRUPT_STATUS: u64 = 0x060;
pub const VIRTIO_MMIO_INTERRUPT_ACK: u64 = 0x064;
pub const VIRTIO_MMIO_STATUS: u64 = 0x070;
pub const VIRTIO_MMIO_QUEUE_DESC_LOW: u64 = 0x080;
pub const VIRTIO_MMIO_QUEUE_DESC_HIGH: u64 = 0x084;
pub const VIRTIO_MMIO_QUEUE_DRIVER_LOW: u64 = 0x090;
pub const VIRTIO_MMIO_QUEUE_DRIVER_HIGH: u64 = 0x094;
pub const VIRTIO_MMIO_QUEUE_DEVICE_LOW: u64 = 0x0A0;
pub const VIRTIO_MMIO_QUEUE_DEVICE_HIGH: u64 = 0x0A4;
pub const VIRTIO_MMIO_CONFIG_GENERATION: u64 = 0x0FC;
pub const VIRTIO_MMIO_CONFIG: u64 = 0x100;

const MAGIC: u32 = 0x7472_6976;
const VERSION: u32 = 2;
const VENDOR_ID: u32 = 0x554D_4551;
const MMIO_REGISTER_BYTES: usize = 4;
const FEATURE_WORD_BITS: u32 = 32;
const LOW_FEATURE_MASK: u64 = 0x0000_0000_FFFF_FFFF;
const HIGH_FEATURE_MASK: u64 = 0xFFFF_FFFF_0000_0000;
const AVAILABLE_INDEX_OFFSET: u64 = 2;
const USED_INDEX_OFFSET: u64 = 2;
const DESCRIPTOR_LENGTH_OFFSET: u64 = 8;
const DESCRIPTOR_FLAGS_OFFSET: u64 = 12;
const DESCRIPTOR_NEXT_OFFSET: u64 = 14;
const USED_LENGTH_OFFSET: u64 = 4;
const VIRTIO_MMIO_INT_VRING: u32 = 1;
pub const VIRTIO_MMIO_DEVICE_SIZE: u64 = 0x1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmioWriteEffect {
    None,
    NotifyQueue(usize),
}

#[derive(Clone, Debug)]
pub struct PlannedAvail {
    pub head_index: u16,
    pub chain: DescriptorChainPlan,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VirtQueueSnapshot {
    pub raw: RawQueueConfig,
    pub ready: bool,
    pub last_avail_idx: u16,
    pub next_used_idx: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VirtioMmioSnapshot {
    pub base_addr: u64,
    pub irq: u32,
    pub device_id: u32,
    pub driver_features: u64,
    pub device_features_sel: u32,
    pub driver_features_sel: u32,
    pub status: u32,
    pub interrupt_status: u32,
    pub config_generation: u32,
    pub queue_sel: u32,
    pub queues: Vec<VirtQueueSnapshot>,
}

#[derive(Clone, Debug)]
pub struct VirtQueue {
    max_size: u16,
    limits: VirtioLimits,
    raw: RawQueueConfig,
    validated: Option<ValidatedQueueConfig>,
    ready: bool,
    last_avail_idx: u16,
    next_used_idx: u16,
    failure: Option<VirtioFailure>,
    pending_completion: Option<PendingCompletionLiveState>,
    last_request_outcome: Option<LastRequestLiveOutcome>,
    used_write_failure: Option<UsedWriteFailurePoint>,
}

impl VirtQueue {
    pub fn new(max_size: u16) -> Self {
        Self::with_limits(max_size, VirtioLimits::default())
    }

    pub fn with_limits(max_size: u16, limits: VirtioLimits) -> Self {
        Self {
            max_size,
            limits,
            raw: RawQueueConfig::default(),
            validated: None,
            ready: false,
            last_avail_idx: 0,
            next_used_idx: 0,
            failure: None,
            pending_completion: None,
            last_request_outcome: None,
            used_write_failure: None,
        }
    }

    pub fn max_size(&self) -> u16 {
        self.max_size
    }

    pub fn size(&self) -> u16 {
        self.validated.map_or_else(
            || u16::try_from(self.raw.size).unwrap_or(0),
            ValidatedQueueConfig::size,
        )
    }

    pub fn limits(&self) -> VirtioLimits {
        self.limits
    }

    pub fn is_ready(&self) -> bool {
        self.ready && self.failure.is_none() && self.validated.is_some()
    }

    pub fn set_size(&mut self, value: u32) -> Result<(), VirtioFailure> {
        if self.ready {
            return Err(VirtioFailure::Transport(
                TransportViolation::QueueSizeWhileReady,
            ));
        }
        validate_queue_size(value, self.max_size, self.limits).map_err(VirtioFailure::Queue)?;
        self.raw.size = value;
        Ok(())
    }

    pub fn set_descriptor_address(&mut self, value: u64) -> Result<(), VirtioFailure> {
        self.require_address_mutable()?;
        self.raw.descriptor_address = value;
        Ok(())
    }

    pub fn set_driver_address(&mut self, value: u64) -> Result<(), VirtioFailure> {
        self.require_address_mutable()?;
        self.raw.driver_address = value;
        Ok(())
    }

    pub fn set_device_address(&mut self, value: u64) -> Result<(), VirtioFailure> {
        self.require_address_mutable()?;
        self.raw.device_address = value;
        Ok(())
    }

    pub fn raw_config(&self) -> RawQueueConfig {
        self.raw
    }

    pub fn activate(&mut self, mem: &GuestMemoryMmap) -> Result<(), VirtioFailure> {
        if self.ready {
            return Err(VirtioFailure::Transport(TransportViolation::ReadyState));
        }
        let (regions, count) = guest_memory_regions(mem)?;
        let validated =
            validate_queue_config(self.raw, self.max_size, &regions[..count], self.limits)
                .map_err(VirtioFailure::Queue)?;
        self.validated = Some(validated);
        self.ready = true;
        self.last_avail_idx = 0;
        self.next_used_idx = 0;
        self.failure = None;
        self.pending_completion = None;
        self.last_request_outcome = None;
        self.used_write_failure = None;
        Ok(())
    }

    pub fn plan_next(&self, mem: &GuestMemoryMmap) -> Result<Option<PlannedAvail>, VirtioFailure> {
        let config = self.config()?;
        let queue_size = config.size();
        let available_index_address = config
            .available_range()
            .start()
            .checked_add(AVAILABLE_INDEX_OFFSET)
            .ok_or(VirtioFailure::Queue(QueueViolation::AddressOverflow))?;
        let available_index = mem
            .read_obj(GuestAddress(available_index_address))
            .map_err(|_| VirtioFailure::GuestMemoryRead)?;
        let delta = validate_available_delta(self.last_avail_idx, available_index, queue_size)
            .map_err(VirtioFailure::Queue)?;
        if delta == 0 {
            return Ok(None);
        }
        let head_address = available_element_address(config, self.last_avail_idx)
            .ok_or(VirtioFailure::Queue(QueueViolation::AddressOverflow))?;
        let head_index: u16 = mem
            .read_obj(GuestAddress(head_address))
            .map_err(|_| VirtioFailure::GuestMemoryRead)?;
        if head_index >= queue_size {
            return Err(VirtioFailure::Queue(QueueViolation::AvailableHead {
                head: head_index,
                capacity: queue_size,
            }));
        }
        let descriptors = self.read_descriptor_table(mem, config)?;
        let (regions, count) = guest_memory_regions(mem)?;
        let chain = plan_descriptor_chain(
            &descriptors[..usize::from(queue_size)],
            head_index,
            queue_size,
            &regions[..count],
            self.limits,
        )
        .map_err(VirtioFailure::Descriptor)?;
        Ok(Some(PlannedAvail { head_index, chain }))
    }

    pub fn stage_completion(
        &mut self,
        head_index: u16,
        written_length: u32,
    ) -> Result<(), VirtioFailure> {
        if self.pending_completion.is_some() {
            return Err(VirtioFailure::CompletionState);
        }
        self.pending_completion = Some(PendingCompletionLiveState {
            head_index,
            written_length,
            backend_started: false,
            effects_started: false,
        });
        Ok(())
    }

    pub fn mark_backend_started(&mut self) -> Result<(), VirtioFailure> {
        let pending = self
            .pending_completion
            .as_mut()
            .ok_or(VirtioFailure::CompletionState)?;
        pending.backend_started = true;
        pending.effects_started = true;
        Ok(())
    }

    pub fn mark_effects_started(&mut self) -> Result<(), VirtioFailure> {
        let pending = self
            .pending_completion
            .as_mut()
            .ok_or(VirtioFailure::CompletionState)?;
        pending.effects_started = true;
        Ok(())
    }

    pub fn complete(
        &mut self,
        mem: &GuestMemoryMmap,
        head_index: u16,
        written_length: u32,
    ) -> Result<(), VirtioFailure> {
        self.publish_completion(mem, head_index, written_length)?;
        self.last_request_outcome = Some(LastRequestLiveOutcome::Completed {
            head_index,
            written_length,
        });
        Ok(())
    }

    pub fn complete_rejected(
        &mut self,
        mem: &GuestMemoryMmap,
        head_index: u16,
        written_length: u32,
        failure: VirtioFailure,
    ) -> Result<(), VirtioFailure> {
        self.last_request_outcome = Some(LastRequestLiveOutcome::Rejected {
            head_index,
            written_length,
            failure,
        });
        self.publish_completion(mem, head_index, written_length)
    }

    pub fn inject_used_write_failure(&mut self, point: UsedWriteFailurePoint) {
        self.used_write_failure = Some(point);
    }

    fn publish_completion(
        &mut self,
        mem: &GuestMemoryMmap,
        head_index: u16,
        written_length: u32,
    ) -> Result<(), VirtioFailure> {
        let pending = self
            .pending_completion
            .ok_or(VirtioFailure::CompletionState)?;
        if pending.head_index != head_index || pending.written_length != written_length {
            return Err(VirtioFailure::CompletionState);
        }
        let config = self.config()?;
        let element_address =
            used_element_address(config, self.next_used_idx).ok_or(VirtioFailure::UsedRingWrite)?;
        let length_address = element_address
            .checked_add(USED_LENGTH_OFFSET)
            .ok_or(VirtioFailure::UsedRingWrite)?;
        let used_index_address = config
            .used_range()
            .start()
            .checked_add(USED_INDEX_OFFSET)
            .ok_or(VirtioFailure::UsedRingWrite)?;
        let next_used_index = self.next_used_idx.wrapping_add(1);
        self.mark_effects_started()?;
        mem.write_obj(u32::from(head_index), GuestAddress(element_address))
            .map_err(|_| VirtioFailure::UsedRingWrite)?;
        mem.write_obj(written_length, GuestAddress(length_address))
            .map_err(|_| VirtioFailure::UsedRingWrite)?;
        if self.used_write_failure.take() == Some(UsedWriteFailurePoint::BeforeIndex) {
            return Err(VirtioFailure::UsedRingWrite);
        }
        mem.write_obj(next_used_index, GuestAddress(used_index_address))
            .map_err(|_| VirtioFailure::UsedRingWrite)?;
        self.last_avail_idx = self.last_avail_idx.wrapping_add(1);
        self.next_used_idx = next_used_index;
        self.pending_completion = None;
        Ok(())
    }

    pub fn mark_failed(&mut self, failure: VirtioFailure) {
        self.failure = Some(failure);
    }

    pub fn snapshot(&self) -> Result<VirtQueueSnapshot, VirtioFailure> {
        if self.failure.is_some()
            || self.pending_completion.is_some()
            || self.used_write_failure.is_some()
        {
            return Err(VirtioFailure::CompletionState);
        }
        Ok(VirtQueueSnapshot {
            raw: self.raw,
            ready: self.ready,
            last_avail_idx: self.last_avail_idx,
            next_used_idx: self.next_used_idx,
        })
    }

    fn restored(
        snapshot: &VirtQueueSnapshot,
        max_size: u16,
        limits: VirtioLimits,
        regions: &[MemoryRegion],
        mem: &GuestMemoryMmap,
    ) -> Result<Self, VirtioFailure> {
        let validated = if snapshot.ready {
            let config = validate_queue_config(snapshot.raw, max_size, regions, limits)
                .map_err(VirtioFailure::Queue)?;
            let available_index_address = config
                .available_range()
                .start()
                .checked_add(AVAILABLE_INDEX_OFFSET)
                .ok_or(VirtioFailure::Queue(QueueViolation::AddressOverflow))?;
            let available_index: u16 = mem
                .read_obj(GuestAddress(available_index_address))
                .map_err(|_| VirtioFailure::GuestMemoryRead)?;
            validate_available_delta(snapshot.last_avail_idx, available_index, config.size())
                .map_err(VirtioFailure::Queue)?;
            let used_index_address = config
                .used_range()
                .start()
                .checked_add(USED_INDEX_OFFSET)
                .ok_or(VirtioFailure::Queue(QueueViolation::AddressOverflow))?;
            let used_index: u16 = mem
                .read_obj(GuestAddress(used_index_address))
                .map_err(|_| VirtioFailure::GuestMemoryRead)?;
            if used_index != snapshot.next_used_idx {
                return Err(VirtioFailure::Queue(
                    QueueViolation::SnapshotUsedIndexMismatch {
                        snapshot: snapshot.next_used_idx,
                        guest: used_index,
                    },
                ));
            }
            Some(config)
        } else {
            if snapshot.last_avail_idx != 0 || snapshot.next_used_idx != 0 {
                return Err(VirtioFailure::Queue(
                    QueueViolation::SnapshotCursorWithoutReady {
                        last_avail: snapshot.last_avail_idx,
                        next_used: snapshot.next_used_idx,
                    },
                ));
            }
            if snapshot.raw.size != 0 {
                validate_queue_size(snapshot.raw.size, max_size, limits)
                    .map_err(VirtioFailure::Queue)?;
            }
            None
        };
        Ok(Self {
            max_size,
            limits,
            raw: snapshot.raw,
            validated,
            ready: snapshot.ready,
            last_avail_idx: snapshot.last_avail_idx,
            next_used_idx: snapshot.next_used_idx,
            failure: None,
            pending_completion: None,
            last_request_outcome: None,
            used_write_failure: None,
        })
    }

    pub fn live_state(&self) -> QueueLiveState {
        let config = self.validated.map(|config| ValidatedQueueLiveConfig {
            size: config.size(),
            descriptor_address: config.descriptor_range().start(),
            driver_address: config.available_range().start(),
            device_address: config.used_range().start(),
        });
        QueueLiveState {
            config,
            ready: self.is_ready(),
            last_avail_idx: self.last_avail_idx,
            next_used_idx: self.next_used_idx,
            failure: self.failure.clone(),
            pending_completion: self.pending_completion,
            last_request_outcome: self.last_request_outcome.clone(),
        }
    }

    fn config(&self) -> Result<ValidatedQueueConfig, VirtioFailure> {
        if !self.is_ready() {
            return Err(VirtioFailure::Queue(QueueViolation::NotValidated));
        }
        self.validated
            .ok_or(VirtioFailure::Queue(QueueViolation::NotValidated))
    }

    fn require_address_mutable(&self) -> Result<(), VirtioFailure> {
        if self.ready {
            return Err(VirtioFailure::Transport(
                TransportViolation::QueueAddressWhileReady,
            ));
        }
        Ok(())
    }

    fn read_descriptor_table(
        &self,
        mem: &GuestMemoryMmap,
        config: ValidatedQueueConfig,
    ) -> Result<[VirtqDesc; MAX_QUEUE_SIZE_USIZE], VirtioFailure> {
        let mut descriptors = [VirtqDesc::default(); MAX_QUEUE_SIZE_USIZE];
        for index in 0..config.size() {
            let address = descriptor_address(config, index)
                .ok_or(VirtioFailure::Queue(QueueViolation::AddressOverflow))?;
            descriptors[usize::from(index)] = VirtqDesc {
                addr: read_at(mem, address)?,
                len: read_at_offset(mem, address, DESCRIPTOR_LENGTH_OFFSET)?,
                flags: read_at_offset(mem, address, DESCRIPTOR_FLAGS_OFFSET)?,
                next: read_at_offset(mem, address, DESCRIPTOR_NEXT_OFFSET)?,
            };
        }
        Ok(descriptors)
    }
}

pub trait VirtioBackend: Send {
    fn device_id(&self) -> u32;
    fn device_features(&self) -> u64;
    fn num_queues(&self) -> usize;
    fn process_queue(
        &mut self,
        queue_idx: usize,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> Result<bool, VirtioFailure>;
    fn read_config(&self, offset: u64, data: &mut [u8]);
    fn write_config(&mut self, offset: u64, data: &[u8]);
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

pub struct VirtioMmioDevice {
    base_addr: u64,
    irq: u32,
    device_features: u64,
    driver_features: u64,
    device_features_sel: u32,
    driver_features_sel: u32,
    status: u32,
    interrupt_status: u32,
    config_generation: u32,
    queues: Vec<VirtQueue>,
    queue_sel: u32,
    backend: Box<dyn VirtioBackend>,
    failure: Option<VirtioFailure>,
    limits: VirtioLimits,
}

impl VirtioMmioDevice {
    pub fn new(base_addr: u64, irq: u32, backend: Box<dyn VirtioBackend>) -> Self {
        Self::with_limits(base_addr, irq, backend, VirtioLimits::default())
    }

    pub fn with_limits(
        base_addr: u64,
        irq: u32,
        backend: Box<dyn VirtioBackend>,
        limits: VirtioLimits,
    ) -> Self {
        let device_features = backend.device_features() | VIRTIO_F_VERSION_1;
        let queues = (0..backend.num_queues())
            .map(|_| VirtQueue::with_limits(MAX_QUEUE_SIZE, limits))
            .collect();
        Self {
            base_addr,
            irq,
            device_features,
            driver_features: 0,
            device_features_sel: 0,
            driver_features_sel: 0,
            status: 0,
            interrupt_status: 0,
            config_generation: 0,
            queues,
            queue_sel: 0,
            backend,
            failure: None,
            limits,
        }
    }

    pub fn handles(&self, address: u64) -> bool {
        self.base_addr
            .checked_add(VIRTIO_MMIO_DEVICE_SIZE)
            .is_some_and(|end| address >= self.base_addr && address < end)
    }

    pub fn irq(&self) -> u32 {
        self.irq
    }

    pub fn base_addr(&self) -> u64 {
        self.base_addr
    }

    pub fn queue_count(&self) -> usize {
        self.queues.len()
    }

    pub fn backend(&self) -> &dyn VirtioBackend {
        &*self.backend
    }

    pub fn backend_mut(&mut self) -> &mut dyn VirtioBackend {
        &mut *self.backend
    }

    pub fn limits(&self) -> VirtioLimits {
        self.limits
    }

    pub fn inject_used_write_failure(
        &mut self,
        queue_index: usize,
        point: UsedWriteFailurePoint,
    ) -> Result<(), VirtioFailure> {
        let queue = self
            .queues
            .get_mut(queue_index)
            .ok_or(VirtioFailure::BackendQueue)?;
        queue.inject_used_write_failure(point);
        Ok(())
    }

    pub fn record_interrupt_failure(&mut self, queue_index: usize, irq: u32, asserted: bool) {
        let failure = VirtioFailure::InterruptDelivery { irq, asserted };
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.mark_failed(failure.clone());
        }
        self.record_failure(failure);
    }

    pub fn snapshot(&self) -> Result<VirtioMmioSnapshot, VirtioFailure> {
        if self.failure.is_some() {
            return Err(VirtioFailure::CompletionState);
        }
        let queues = self
            .queues
            .iter()
            .map(VirtQueue::snapshot)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(VirtioMmioSnapshot {
            base_addr: self.base_addr,
            irq: self.irq,
            device_id: self.backend.device_id(),
            driver_features: self.driver_features,
            device_features_sel: self.device_features_sel,
            driver_features_sel: self.driver_features_sel,
            status: self.status,
            interrupt_status: self.interrupt_status,
            config_generation: self.config_generation,
            queue_sel: self.queue_sel,
            queues,
        })
    }

    pub fn validate_snapshot(
        &self,
        snapshot: &VirtioMmioSnapshot,
        mem: &GuestMemoryMmap,
    ) -> Result<(), VirtioFailure> {
        if snapshot.base_addr != self.base_addr
            || snapshot.irq != self.irq
            || snapshot.device_id != self.backend.device_id()
            || snapshot.queues.len() != self.queues.len()
        {
            return Err(VirtioFailure::Transport(
                TransportViolation::QueueSelection {
                    selected: snapshot.queue_sel,
                    available: self.queues.len(),
                },
            ));
        }
        if snapshot.driver_features & !self.device_features != 0 {
            return Err(VirtioFailure::Transport(
                TransportViolation::UnsupportedFeatures {
                    requested: snapshot.driver_features,
                    offered: self.device_features,
                },
            ));
        }
        let queue_selected = usize::try_from(snapshot.queue_sel).ok();
        if queue_selected.is_some_and(|selected| selected >= self.queues.len()) {
            return Err(VirtioFailure::Transport(
                TransportViolation::QueueSelection {
                    selected: snapshot.queue_sel,
                    available: self.queues.len(),
                },
            ));
        }
        validate_restored_status(
            snapshot.status,
            self.device_features,
            snapshot.driver_features,
        )
        .map_err(VirtioFailure::Transport)?;
        let (regions, count) = guest_memory_regions(mem)?;
        for (queue_snapshot, queue) in snapshot.queues.iter().zip(&self.queues) {
            VirtQueue::restored(
                queue_snapshot,
                queue.max_size,
                queue.limits,
                &regions[..count],
                mem,
            )?;
        }
        Ok(())
    }

    pub fn restore_snapshot(
        &mut self,
        snapshot: &VirtioMmioSnapshot,
        mem: &GuestMemoryMmap,
    ) -> Result<(), VirtioFailure> {
        self.validate_snapshot(snapshot, mem)?;
        let (regions, count) = guest_memory_regions(mem)?;
        let restored_queues = snapshot
            .queues
            .iter()
            .zip(&self.queues)
            .map(|(queue_snapshot, queue)| {
                VirtQueue::restored(
                    queue_snapshot,
                    queue.max_size,
                    queue.limits,
                    &regions[..count],
                    mem,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.driver_features = snapshot.driver_features;
        self.device_features_sel = snapshot.device_features_sel;
        self.driver_features_sel = snapshot.driver_features_sel;
        self.status = snapshot.status;
        self.interrupt_status = snapshot.interrupt_status;
        self.config_generation = snapshot.config_generation;
        self.queue_sel = snapshot.queue_sel;
        self.queues = restored_queues;
        self.failure = None;
        Ok(())
    }

    pub fn live_state(&self) -> VirtioLiveState {
        VirtioLiveState {
            status: self.status,
            interrupt_status: self.interrupt_status,
            failure: self.failure.clone(),
            queues: self.queues.iter().map(VirtQueue::live_state).collect(),
        }
    }

    pub fn read(&self, offset: u64, data: &mut [u8]) {
        data.fill(0);
        let value = match offset {
            VIRTIO_MMIO_MAGIC_VALUE => MAGIC,
            VIRTIO_MMIO_VERSION => VERSION,
            VIRTIO_MMIO_DEVICE_ID => self.backend.device_id(),
            VIRTIO_MMIO_VENDOR_ID => VENDOR_ID,
            VIRTIO_MMIO_DEVICE_FEATURES => self.selected_device_features(),
            VIRTIO_MMIO_QUEUE_NUM_MAX => self
                .selected_queue()
                .map_or(0, |queue| u32::from(queue.max_size())),
            VIRTIO_MMIO_QUEUE_READY => self
                .selected_queue()
                .map_or(0, |queue| u32::from(queue.is_ready())),
            VIRTIO_MMIO_INTERRUPT_STATUS => self.interrupt_status,
            VIRTIO_MMIO_STATUS => self.status,
            VIRTIO_MMIO_CONFIG_GENERATION => self.config_generation,
            _ if offset >= VIRTIO_MMIO_CONFIG => {
                self.backend.read_config(offset - VIRTIO_MMIO_CONFIG, data);
                return;
            }
            _ => 0,
        };
        let bytes = value.to_le_bytes();
        let copy_length = data.len().min(MMIO_REGISTER_BYTES);
        data[..copy_length].copy_from_slice(&bytes[..copy_length]);
    }

    pub fn write_at(
        &mut self,
        address: u64,
        data: &[u8],
        mem: &GuestMemoryMmap,
    ) -> Result<MmioWriteEffect, VirtioFailure> {
        let offset = address
            .checked_sub(self.base_addr)
            .ok_or(VirtioFailure::Transport(TransportViolation::MmioAddress {
                address,
            }))?;
        if offset >= VIRTIO_MMIO_DEVICE_SIZE {
            return Err(VirtioFailure::Transport(TransportViolation::MmioAddress {
                address,
            }));
        }
        self.write(offset, data, mem)
    }

    pub fn write(
        &mut self,
        offset: u64,
        data: &[u8],
        mem: &GuestMemoryMmap,
    ) -> Result<MmioWriteEffect, VirtioFailure> {
        if offset >= VIRTIO_MMIO_CONFIG {
            self.backend.write_config(offset - VIRTIO_MMIO_CONFIG, data);
            return Ok(MmioWriteEffect::None);
        }
        let value = match parse_register(data) {
            Ok(value) => value,
            Err(failure) => {
                self.record_failure(failure.clone());
                return Err(failure);
            }
        };
        let result = self.write_register(offset, value, mem);
        if let Err(failure) = &result {
            self.record_failure(failure.clone());
        }
        result
    }

    pub fn process_queue(&mut self, queue_index: usize, mem: &GuestMemoryMmap) -> bool {
        if self.status & VIRTIO_STATUS_DRIVER_OK == 0
            || self.status & VIRTIO_STATUS_DEVICE_NEEDS_RESET != 0
            || queue_index >= self.queues.len()
            || !self.queues[queue_index].is_ready()
        {
            return false;
        }
        match self
            .backend
            .process_queue(queue_index, &mut self.queues[queue_index], mem)
        {
            Ok(completed) => {
                if completed {
                    self.interrupt_status |= VIRTIO_MMIO_INT_VRING;
                }
                completed
            }
            Err(failure) => {
                self.queues[queue_index].mark_failed(failure.clone());
                self.record_failure(failure);
                false
            }
        }
    }

    pub fn process_queues(&mut self, mem: &GuestMemoryMmap) -> bool {
        let mut completed = false;
        for queue_index in 0..self.queues.len() {
            completed |= self.process_queue(queue_index, mem);
        }
        completed
    }

    pub fn interrupt_pending(&self) -> bool {
        self.interrupt_status != 0
    }

    fn write_register(
        &mut self,
        offset: u64,
        value: u32,
        mem: &GuestMemoryMmap,
    ) -> Result<MmioWriteEffect, VirtioFailure> {
        match offset {
            VIRTIO_MMIO_DEVICE_FEATURES_SEL => self.device_features_sel = value,
            VIRTIO_MMIO_DRIVER_FEATURES_SEL => self.driver_features_sel = value,
            VIRTIO_MMIO_DRIVER_FEATURES => self.write_driver_features(value)?,
            VIRTIO_MMIO_QUEUE_SEL => self.queue_sel = value,
            VIRTIO_MMIO_QUEUE_NUM => self.selected_queue_mut()?.set_size(value)?,
            VIRTIO_MMIO_QUEUE_READY => self.write_queue_ready(value, mem)?,
            VIRTIO_MMIO_QUEUE_DESC_LOW => {
                self.write_queue_address(value, AddressPart::DescriptorLow)?
            }
            VIRTIO_MMIO_QUEUE_DESC_HIGH => {
                self.write_queue_address(value, AddressPart::DescriptorHigh)?
            }
            VIRTIO_MMIO_QUEUE_DRIVER_LOW => {
                self.write_queue_address(value, AddressPart::DriverLow)?
            }
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH => {
                self.write_queue_address(value, AddressPart::DriverHigh)?
            }
            VIRTIO_MMIO_QUEUE_DEVICE_LOW => {
                self.write_queue_address(value, AddressPart::DeviceLow)?
            }
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH => {
                self.write_queue_address(value, AddressPart::DeviceHigh)?
            }
            VIRTIO_MMIO_QUEUE_NOTIFY => return self.notify_effect(value),
            VIRTIO_MMIO_INTERRUPT_ACK => self.interrupt_status &= !value,
            VIRTIO_MMIO_STATUS => self.write_status(value)?,
            _ => {}
        }
        Ok(MmioWriteEffect::None)
    }

    fn write_driver_features(&mut self, value: u32) -> Result<(), VirtioFailure> {
        if self.status & VIRTIO_STATUS_FEATURES_OK != 0 {
            return Err(VirtioFailure::Transport(
                TransportViolation::FeaturesAfterAcceptance,
            ));
        }
        match self.driver_features_sel {
            0 => {
                self.driver_features =
                    (self.driver_features & HIGH_FEATURE_MASK) | u64::from(value);
            }
            1 => {
                self.driver_features = (self.driver_features & LOW_FEATURE_MASK)
                    | (u64::from(value) << FEATURE_WORD_BITS);
            }
            selected => {
                return Err(VirtioFailure::Transport(
                    TransportViolation::FeatureSelector { selected },
                ));
            }
        }
        Ok(())
    }

    fn write_queue_ready(
        &mut self,
        value: u32,
        mem: &GuestMemoryMmap,
    ) -> Result<(), VirtioFailure> {
        if value != 1 {
            return Err(VirtioFailure::Transport(TransportViolation::ReadyValue {
                value,
            }));
        }
        if self.status & VIRTIO_STATUS_FEATURES_OK == 0 {
            return Err(VirtioFailure::Transport(TransportViolation::ReadyState));
        }
        self.selected_queue_mut()?.activate(mem)
    }

    fn write_status(&mut self, value: u32) -> Result<(), VirtioFailure> {
        validate_status_transition(
            self.status,
            value,
            self.device_features,
            self.driver_features,
        )
        .map_err(VirtioFailure::Transport)?;
        if value == 0 {
            self.reset();
        } else {
            self.status = value;
        }
        Ok(())
    }

    fn write_queue_address(&mut self, value: u32, part: AddressPart) -> Result<(), VirtioFailure> {
        let queue = self.selected_queue_mut()?;
        let raw = queue.raw_config();
        match part {
            AddressPart::DescriptorLow => {
                queue.set_descriptor_address(join_low(raw.descriptor_address, value))
            }
            AddressPart::DescriptorHigh => {
                queue.set_descriptor_address(join_high(raw.descriptor_address, value))
            }
            AddressPart::DriverLow => queue.set_driver_address(join_low(raw.driver_address, value)),
            AddressPart::DriverHigh => {
                queue.set_driver_address(join_high(raw.driver_address, value))
            }
            AddressPart::DeviceLow => queue.set_device_address(join_low(raw.device_address, value)),
            AddressPart::DeviceHigh => {
                queue.set_device_address(join_high(raw.device_address, value))
            }
        }
    }

    fn notify_effect(&self, selected: u32) -> Result<MmioWriteEffect, VirtioFailure> {
        let index = usize::try_from(selected).map_err(|_| {
            VirtioFailure::Transport(TransportViolation::QueueSelection {
                selected,
                available: self.queues.len(),
            })
        })?;
        let queue = self.queues.get(index).ok_or(VirtioFailure::Transport(
            TransportViolation::QueueSelection {
                selected,
                available: self.queues.len(),
            },
        ))?;
        if self.status & VIRTIO_STATUS_DRIVER_OK == 0 || !queue.is_ready() {
            return Err(VirtioFailure::Transport(
                TransportViolation::NotifyQueueNotReady { selected },
            ));
        }
        Ok(MmioWriteEffect::NotifyQueue(index))
    }

    fn selected_device_features(&self) -> u32 {
        match self.device_features_sel {
            0 => self.device_features as u32,
            1 => (self.device_features >> FEATURE_WORD_BITS) as u32,
            _ => 0,
        }
    }

    fn selected_queue(&self) -> Option<&VirtQueue> {
        usize::try_from(self.queue_sel)
            .ok()
            .and_then(|index| self.queues.get(index))
    }

    fn selected_queue_mut(&mut self) -> Result<&mut VirtQueue, VirtioFailure> {
        let available = self.queues.len();
        let selected = self.queue_sel;
        let index = usize::try_from(selected).map_err(|_| {
            VirtioFailure::Transport(TransportViolation::QueueSelection {
                selected,
                available,
            })
        })?;
        self.queues.get_mut(index).ok_or(VirtioFailure::Transport(
            TransportViolation::QueueSelection {
                selected,
                available,
            },
        ))
    }

    fn record_failure(&mut self, failure: VirtioFailure) {
        self.failure = Some(failure);
        self.status |= VIRTIO_STATUS_DEVICE_NEEDS_RESET;
    }

    fn reset(&mut self) {
        self.driver_features = 0;
        self.device_features_sel = 0;
        self.driver_features_sel = 0;
        self.status = 0;
        self.interrupt_status = 0;
        self.queue_sel = 0;
        self.failure = None;
        for queue in &mut self.queues {
            *queue = VirtQueue::with_limits(MAX_QUEUE_SIZE, self.limits);
        }
    }
}

#[derive(Clone, Copy)]
enum AddressPart {
    DescriptorLow,
    DescriptorHigh,
    DriverLow,
    DriverHigh,
    DeviceLow,
    DeviceHigh,
}

fn parse_register(data: &[u8]) -> Result<u32, VirtioFailure> {
    let bytes: [u8; MMIO_REGISTER_BYTES] = data.try_into().map_err(|_| {
        VirtioFailure::Transport(TransportViolation::MmioWidth { actual: data.len() })
    })?;
    Ok(u32::from_le_bytes(bytes))
}

fn join_low(current: u64, value: u32) -> u64 {
    (current & HIGH_FEATURE_MASK) | u64::from(value)
}

fn join_high(current: u64, value: u32) -> u64 {
    (current & LOW_FEATURE_MASK) | (u64::from(value) << FEATURE_WORD_BITS)
}

fn guest_memory_regions(
    mem: &GuestMemoryMmap,
) -> Result<([MemoryRegion; MAX_GUEST_MEMORY_REGIONS], usize), VirtioFailure> {
    let count = mem.num_regions();
    if count > MAX_GUEST_MEMORY_REGIONS {
        return Err(VirtioFailure::Queue(QueueViolation::GuestMemoryRegions {
            actual: count,
            maximum: MAX_GUEST_MEMORY_REGIONS,
        }));
    }
    let mut regions = [MemoryRegion::default(); MAX_GUEST_MEMORY_REGIONS];
    for (index, region) in mem.iter().enumerate() {
        regions[index] = MemoryRegion {
            start: region.start_addr().raw_value(),
            length: region.len(),
        };
    }
    Ok((regions, count))
}

fn read_at<T: vm_memory::ByteValued>(
    mem: &GuestMemoryMmap,
    address: u64,
) -> Result<T, VirtioFailure> {
    mem.read_obj(GuestAddress(address))
        .map_err(|_| VirtioFailure::GuestMemoryRead)
}

fn read_at_offset<T: vm_memory::ByteValued>(
    mem: &GuestMemoryMmap,
    address: u64,
    offset: u64,
) -> Result<T, VirtioFailure> {
    let field_address = address
        .checked_add(offset)
        .ok_or(VirtioFailure::GuestMemoryRead)?;
    read_at(mem, field_address)
}
