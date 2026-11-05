//! Deterministic fuzz input parsing and planning invariants.
//!
//! Inputs are parsed from raw bytes so corpus seeds are stable and hand-craftable.
//! The shapes are deliberately small: queue sizes 1-16, chains 1-8, and tiny
//! memory regions, so fit/overflow decisions are reached constantly.

use chaoscontrol_vmm::devices::virtio_chain::{DescriptorChainPlan, VirtqDesc};
use chaoscontrol_vmm::devices::virtio_types::VirtioLimits;
use chaoscontrol_vmm::devices::virtio_validation::MemoryRegion;

pub struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub fn u8(&mut self) -> u8 {
        let value = self.bytes.get(self.offset).copied().unwrap_or(0);
        self.offset += 1;
        value
    }

    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.u8(), self.u8()])
    }

    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes([self.u8(), self.u8(), self.u8(), self.u8()])
    }

    pub fn u64(&mut self) -> u64 {
        let mut bytes = [0u8; 8];
        for byte in &mut bytes {
            *byte = self.u8();
        }
        u64::from_le_bytes(bytes)
    }
}

pub struct ParsedInput {
    pub descriptors: Vec<VirtqDesc>,
    pub head_index: u16,
    pub queue_size: u16,
    pub memory: Vec<MemoryRegion>,
    pub limits: VirtioLimits,
}

pub fn parse_input(data: &[u8]) -> ParsedInput {
    let mut reader = Reader::new(data);
    let descriptor_count = usize::from(reader.u8() % 33);
    let descriptors = (0..descriptor_count)
        .map(|_| VirtqDesc {
            addr: reader.u64(),
            len: reader.u32(),
            flags: reader.u16(),
            next: reader.u16(),
        })
        .collect::<Vec<_>>();
    let queue_size = 1 + u16::from(reader.u8() % 16);
    let head_index = reader.u16() % queue_size;
    let region_count = usize::from(reader.u8() % 5);
    let memory = (0..region_count)
        .map(|_| MemoryRegion {
            start: reader.u64(),
            length: reader.u64(),
        })
        .collect::<Vec<_>>();
    let limits = VirtioLimits {
        max_queue_size: queue_size,
        max_chain_descriptors: 1 + u16::from(reader.u8() % 8),
        max_aggregate_bytes: u64::from(reader.u16()),
        max_block_transfer_bytes: u64::from(reader.u16()),
        max_net_frame_bytes: u64::from(reader.u16()),
        max_net_tx_packets: usize::from(reader.u8()),
        max_net_tx_bytes: u64::from(reader.u16()),
        max_entropy_transfer_bytes: u64::from(reader.u16()),
        scratch_bytes: usize::from(reader.u16()),
    };
    ParsedInput {
        descriptors,
        head_index,
        queue_size,
        memory,
        limits,
    }
}

pub fn plan(input: &ParsedInput) -> Option<DescriptorChainPlan> {
    chaoscontrol_vmm::devices::virtio_chain::plan_descriptor_chain(
        &input.descriptors,
        input.head_index,
        input.queue_size,
        &input.memory,
        input.limits,
    )
    .ok()
}

fn contained(memory: &[MemoryRegion], addr: u64, len: u64) -> bool {
    if len == 0 {
        // A zero-length range is trivially contained, matching checked_range semantics.
        return true;
    }
    let Some(end) = addr.checked_add(len) else {
        return false;
    };
    memory
        .iter()
        .any(|region| addr >= region.start && end <= region.start.saturating_add(region.length))
}

pub fn check_plan(plan: &DescriptorChainPlan, input: &ParsedInput) {
    let buffers = plan.buffers();
    assert_eq!(usize::from(plan.count()), buffers.len());
    assert!(plan.count() <= input.limits.max_chain_descriptors.min(input.queue_size));
    let mut aggregate = 0u64;
    let mut seen = std::collections::BTreeSet::new();
    for buffer in buffers {
        assert!(
            seen.insert(buffer.index),
            "descriptor index visited twice: {}",
            buffer.index
        );
        assert!(u64::from(buffer.len) <= u64::MAX - buffer.addr);
        assert!(
            contained(&input.memory, buffer.addr, u64::from(buffer.len)),
            "buffer outside admitted memory: {buffer:?}"
        );
        aggregate = aggregate
            .checked_add(u64::from(buffer.len))
            .expect("aggregate overflow");
    }
    assert_eq!(aggregate, plan.aggregate_length());
    assert!(aggregate <= input.limits.max_aggregate_bytes);
}
