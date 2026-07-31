//! Pure bounded split-ring descriptor-chain planning.

use super::virtio_types::{DescriptorViolation, VirtioLimits, MAX_QUEUE_SIZE_USIZE};
use super::virtio_validation::{checked_range, range_is_contained, MemoryRegion};

pub const VIRTQ_DESC_F_NEXT: u16 = 1;
pub const VIRTQ_DESC_F_WRITE: u16 = 2;
pub const VIRTQ_DESC_F_INDIRECT: u16 = 4;
const SUPPORTED_DESCRIPTOR_FLAGS: u16 = VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DescriptorBuffer {
    pub index: u16,
    pub addr: u64,
    pub len: u32,
    pub write: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptorChainPlan {
    buffers: [DescriptorBuffer; MAX_QUEUE_SIZE_USIZE],
    count: u16,
    aggregate_length: u64,
}

impl DescriptorChainPlan {
    pub fn buffers(&self) -> &[DescriptorBuffer] {
        &self.buffers[..usize::from(self.count)]
    }

    pub fn count(&self) -> u16 {
        self.count
    }

    pub fn aggregate_length(&self) -> u64 {
        self.aggregate_length
    }
}

pub fn plan_descriptor_chain(
    descriptors: &[VirtqDesc],
    head_index: u16,
    queue_size: u16,
    memory: &[MemoryRegion],
    limits: VirtioLimits,
) -> Result<DescriptorChainPlan, DescriptorViolation> {
    if usize::from(queue_size) > MAX_QUEUE_SIZE_USIZE {
        return Err(DescriptorViolation::CountLimit {
            count: queue_size,
            maximum: limits
                .max_chain_descriptors
                .min(MAX_QUEUE_SIZE_USIZE as u16),
        });
    }
    if head_index >= queue_size || usize::from(head_index) >= descriptors.len() {
        return Err(DescriptorViolation::HeadIndex {
            index: head_index,
            capacity: queue_size,
        });
    }
    let chain_limit = queue_size.min(limits.max_chain_descriptors);
    let mut visited = [false; MAX_QUEUE_SIZE_USIZE];
    let mut buffers = [DescriptorBuffer::default(); MAX_QUEUE_SIZE_USIZE];
    let mut count = 0u16;
    let mut aggregate_length = 0u64;
    let mut index = head_index;

    loop {
        if index >= queue_size || usize::from(index) >= descriptors.len() {
            return Err(DescriptorViolation::NextIndex {
                index,
                capacity: queue_size,
            });
        }
        if visited[usize::from(index)] {
            return Err(DescriptorViolation::Cycle { index });
        }
        if count >= chain_limit {
            return Err(DescriptorViolation::CountLimit {
                count: count.saturating_add(1),
                maximum: chain_limit,
            });
        }
        visited[usize::from(index)] = true;
        let descriptor = descriptors[usize::from(index)];
        if descriptor.flags & !SUPPORTED_DESCRIPTOR_FLAGS != 0 {
            return Err(DescriptorViolation::UnsupportedFlags {
                flags: descriptor.flags,
            });
        }
        let range = checked_range(descriptor.addr, u64::from(descriptor.len)).map_err(|_| {
            DescriptorViolation::AddressOverflow {
                address: descriptor.addr,
                length: descriptor.len,
            }
        })?;
        if !range_is_contained(memory, range) {
            return Err(DescriptorViolation::OutsideMemory {
                address: descriptor.addr,
                length: descriptor.len,
            });
        }
        aggregate_length = aggregate_length
            .checked_add(u64::from(descriptor.len))
            .ok_or(DescriptorViolation::AggregateOverflow)?;
        if aggregate_length > limits.max_aggregate_bytes {
            return Err(DescriptorViolation::AggregateLimit {
                length: aggregate_length,
                maximum: limits.max_aggregate_bytes,
            });
        }
        buffers[usize::from(count)] = DescriptorBuffer {
            index,
            addr: descriptor.addr,
            len: descriptor.len,
            write: descriptor.flags & VIRTQ_DESC_F_WRITE != 0,
        };
        count = count.saturating_add(1);
        if descriptor.flags & VIRTQ_DESC_F_NEXT == 0 {
            break;
        }
        index = descriptor.next;
    }

    debug_assert!(count > 0);
    debug_assert!(count <= chain_limit);
    Ok(DescriptorChainPlan {
        buffers,
        count,
        aggregate_length,
    })
}
