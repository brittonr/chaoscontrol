//! Bounded virtio-rng shell with transactional deterministic entropy state.

use super::entropy::DeterministicEntropy;
use super::virtio_buffer::{BoundedBufferAllocator, HostBufferAllocator};
use super::virtio_chain::DescriptorBuffer;
use super::virtio_mmio::{VirtQueue, VirtioBackend};
use super::virtio_request::plan_entropy_request;
use super::virtio_types::{ResourceViolation, VirtioFailure};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const VIRTIO_ENTROPY_DEVICE_ID: u32 = 4;
const VIRTIO_ENTROPY_QUEUE_COUNT: usize = 1;
const MINIMUM_SCRATCH_BYTES: usize = 1;

pub struct VirtioEntropy {
    entropy: DeterministicEntropy,
    allocator: Box<dyn BoundedBufferAllocator>,
}

impl VirtioEntropy {
    pub fn new(entropy: DeterministicEntropy) -> Self {
        Self::with_allocator(entropy, Box::<HostBufferAllocator>::default())
    }

    pub fn with_allocator(
        entropy: DeterministicEntropy,
        allocator: Box<dyn BoundedBufferAllocator>,
    ) -> Self {
        Self { entropy, allocator }
    }

    pub fn entropy(&self) -> &DeterministicEntropy {
        &self.entropy
    }

    pub fn entropy_mut(&mut self) -> &mut DeterministicEntropy {
        &mut self.entropy
    }

    fn process_one(
        &mut self,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> Result<bool, VirtioFailure> {
        let Some(available) = queue.plan_next(mem)? else {
            return Ok(false);
        };
        let plan = plan_entropy_request(&available.chain, queue.limits())
            .map_err(VirtioFailure::Request)?;
        let mut scratch = self.allocate_scratch(plan.transfer_bytes, queue)?;
        let mut candidate_entropy = self.entropy.clone();
        fill_guest_transactionally(
            mem,
            available.chain.buffers(),
            &mut scratch,
            &mut candidate_entropy,
        )?;
        self.entropy = candidate_entropy;
        let used_length =
            u32::try_from(plan.transfer_bytes).map_err(|_| VirtioFailure::BackendWrite)?;
        queue.complete(mem, available.head_index, used_length)?;
        Ok(true)
    }

    fn allocate_scratch(
        &mut self,
        transfer_bytes: u64,
        queue: &VirtQueue,
    ) -> Result<Vec<u8>, VirtioFailure> {
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
        self.allocator
            .zeroed(requested, maximum)
            .map_err(VirtioFailure::Resource)
    }
}

impl VirtioBackend for VirtioEntropy {
    fn device_id(&self) -> u32 {
        VIRTIO_ENTROPY_DEVICE_ID
    }

    fn device_features(&self) -> u64 {
        0
    }

    fn num_queues(&self) -> usize {
        VIRTIO_ENTROPY_QUEUE_COUNT
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

    fn read_config(&self, _offset: u64, data: &mut [u8]) {
        data.fill(0);
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn fill_guest_transactionally(
    mem: &GuestMemoryMmap,
    buffers: &[DescriptorBuffer],
    scratch: &mut [u8],
    entropy: &mut DeterministicEntropy,
) -> Result<(), VirtioFailure> {
    for buffer in buffers {
        let mut remaining =
            usize::try_from(buffer.len).map_err(|_| VirtioFailure::GuestMemoryWrite)?;
        let mut address = buffer.addr;
        while remaining > 0 {
            let chunk_length = remaining.min(scratch.len());
            let chunk = &mut scratch[..chunk_length];
            entropy.fill_bytes(chunk);
            mem.write_slice(chunk, GuestAddress(address))
                .map_err(|_| VirtioFailure::GuestMemoryWrite)?;
            address = address
                .checked_add(
                    u64::try_from(chunk_length).map_err(|_| VirtioFailure::GuestMemoryWrite)?,
                )
                .ok_or(VirtioFailure::GuestMemoryWrite)?;
            remaining -= chunk_length;
        }
    }
    Ok(())
}
