//! Bounded virtio-blk shell over deterministic copy-on-write storage.

use super::block::DeterministicBlock;
use super::virtio_block_io::{
    block_data_buffers, preflight_guest_reads, read_header, transfer_disk_to_guest,
    transfer_guest_to_disk,
};
use super::virtio_buffer::{
    with_zeroed_scratch, BoundedBufferAllocator, HostBufferAllocator, ScratchPoolObservations,
};
use super::virtio_mmio::{VirtQueue, VirtioBackend};
use super::virtio_request::{plan_block_request, validated_block_status, BlockOperation};
use super::virtio_types::{ResourceViolation, VirtioFailure};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const VIRTIO_BLK_DEVICE_ID: u32 = 2;
const VIRTIO_BLK_F_SIZE_MAX: u64 = 1 << 1;
const VIRTIO_BLK_F_SEG_MAX: u64 = 1 << 2;
const BLOCK_SECTOR_BYTES: u64 = 512;
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const STATUS_USED_BYTES: u32 = 1;
const MINIMUM_SCRATCH_BYTES: usize = 1;
const CAPACITY_FIELD_BYTES: usize = 8;

pub struct VirtioBlock {
    disk: DeterministicBlock,
    num_sectors: u64,
    allocator: Box<dyn BoundedBufferAllocator>,
}

impl VirtioBlock {
    pub fn new(disk: DeterministicBlock) -> Self {
        Self::try_new(disk).expect("default block scratch capacity must allocate before activation")
    }

    pub fn try_new(disk: DeterministicBlock) -> Result<Self, ResourceViolation> {
        Ok(Self::with_allocator(
            disk,
            Box::new(HostBufferAllocator::try_default()?),
        ))
    }

    pub fn with_allocator(
        disk: DeterministicBlock,
        allocator: Box<dyn BoundedBufferAllocator>,
    ) -> Self {
        let num_sectors = disk.size() / BLOCK_SECTOR_BYTES;
        Self {
            disk,
            num_sectors,
            allocator,
        }
    }

    pub fn disk(&self) -> &DeterministicBlock {
        &self.disk
    }

    pub fn disk_mut(&mut self) -> &mut DeterministicBlock {
        &mut self.disk
    }

    pub fn scratch_capacity_observations(&self) -> ScratchPoolObservations {
        self.allocator.observations()
    }

    fn process_one(
        &mut self,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> Result<bool, VirtioFailure> {
        let Some(available) = queue.plan_next(mem)? else {
            return Ok(false);
        };
        let header = match read_header(mem, &available.chain) {
            Ok(header) => header,
            Err(failure) => return self.complete_request_error(queue, mem, available, failure),
        };
        let plan =
            match plan_block_request(&available.chain, header, self.disk.size(), queue.limits()) {
                Ok(plan) => plan,
                Err(violation) => {
                    return self.complete_request_error(
                        queue,
                        mem,
                        available,
                        VirtioFailure::Request(violation),
                    );
                }
            };
        let data = block_data_buffers(&available.chain)?;
        let (requested, maximum) = Self::scratch_request(plan.transfer_bytes, queue)?;
        let allocator = &mut *self.allocator;
        let disk = &mut self.disk;
        with_zeroed_scratch(allocator, requested, maximum, |scratch| {
            if plan.operation == BlockOperation::Write {
                preflight_guest_reads(mem, data, scratch)?;
            }
            let used_length = match plan.operation {
                BlockOperation::Read => u32::try_from(
                    plan.transfer_bytes
                        .checked_add(u64::from(STATUS_USED_BYTES))
                        .ok_or(VirtioFailure::BackendRead)?,
                )
                .map_err(|_| VirtioFailure::BackendRead)?,
                BlockOperation::Write => STATUS_USED_BYTES,
            };
            queue.stage_completion(available.head_index, used_length)?;
            queue.mark_backend_started()?;
            match plan.operation {
                BlockOperation::Write => {
                    transfer_guest_to_disk(mem, disk, data, plan.disk_offset, scratch)?;
                }
                BlockOperation::Read => {
                    transfer_disk_to_guest(mem, disk, data, plan.disk_offset, scratch)?
                }
            }
            mem.write_obj(VIRTIO_BLK_S_OK, GuestAddress(plan.status_address))
                .map_err(|_| VirtioFailure::GuestMemoryWrite)?;
            queue.complete(mem, available.head_index, used_length)?;
            Ok(true)
        })
    }

    fn complete_request_error(
        &mut self,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
        available: super::virtio_mmio::PlannedAvail,
        failure: VirtioFailure,
    ) -> Result<bool, VirtioFailure> {
        let Some(status_address) = validated_block_status(&available.chain) else {
            return Err(failure);
        };
        queue.stage_completion(available.head_index, STATUS_USED_BYTES)?;
        queue.mark_effects_started()?;
        mem.write_obj(VIRTIO_BLK_S_IOERR, GuestAddress(status_address))
            .map_err(|_| VirtioFailure::GuestMemoryWrite)?;
        queue.complete_rejected(mem, available.head_index, STATUS_USED_BYTES, failure)?;
        Ok(true)
    }

    fn scratch_request(
        transfer_bytes: u64,
        queue: &VirtQueue,
    ) -> Result<(usize, usize), VirtioFailure> {
        let maximum = queue.limits().scratch_bytes;
        if maximum < MINIMUM_SCRATCH_BYTES {
            return Err(VirtioFailure::Resource(ResourceViolation::ScratchLimit {
                requested: MINIMUM_SCRATCH_BYTES,
                maximum,
            }));
        }
        let maximum_u64 = u64::try_from(maximum).map_err(|_| {
            VirtioFailure::Resource(ResourceViolation::ScratchLimit {
                requested: maximum,
                maximum,
            })
        })?;
        let requested = usize::try_from(transfer_bytes.min(maximum_u64)).map_err(|_| {
            VirtioFailure::Resource(ResourceViolation::ScratchLimit {
                requested: maximum,
                maximum,
            })
        })?;
        Ok((requested, maximum))
    }
}

impl VirtioBackend for VirtioBlock {
    fn device_id(&self) -> u32 {
        VIRTIO_BLK_DEVICE_ID
    }

    fn device_features(&self) -> u64 {
        VIRTIO_BLK_F_SIZE_MAX | VIRTIO_BLK_F_SEG_MAX
    }

    fn num_queues(&self) -> usize {
        1
    }

    fn process_queue(
        &mut self,
        _queue_idx: usize,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> Result<bool, VirtioFailure> {
        let mut completed = false;
        while self.process_one(queue, mem)? {
            completed = true;
        }
        Ok(completed)
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        data.fill(0);
        if offset < CAPACITY_FIELD_BYTES as u64 {
            let bytes = self.num_sectors.to_le_bytes();
            let start = usize::try_from(offset).unwrap_or(CAPACITY_FIELD_BYTES);
            let end = start.saturating_add(data.len()).min(CAPACITY_FIELD_BYTES);
            let copy_length = end.saturating_sub(start);
            data[..copy_length].copy_from_slice(&bytes[start..end]);
        }
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
