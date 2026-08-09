//! Bounded virtio-net RX and TX shells over deterministic queues.

use super::net::{DeterministicNet, NetQueueError};
use super::virtio_buffer::{
    with_zeroed_scratch, BoundedBufferAllocator, HostBufferAllocator, ScratchPoolObservations,
};
use super::virtio_chain::DescriptorChainPlan;
use super::virtio_mmio::{VirtQueue, VirtioBackend};
use super::virtio_request::{plan_net_request, NetDirection, NET_HEADER_BYTES};
use super::virtio_types::{ResourceViolation, VirtioFailure};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const VIRTIO_NET_DEVICE_ID: u32 = 1;
const VIRTIO_NET_F_MAC: u64 = 1 << 5;
const RX_QUEUE_INDEX: usize = 0;
const TX_QUEUE_INDEX: usize = 1;
const VIRTIO_NET_QUEUE_COUNT: usize = 2;
const MAC_BYTES: usize = 6;
const EMPTY_USED_BYTES: u32 = 0;

pub struct VirtioNet {
    net: DeterministicNet,
    allocator: Box<dyn BoundedBufferAllocator>,
}

impl VirtioNet {
    pub fn new(net: DeterministicNet) -> Self {
        Self::try_new(net)
            .expect("default network scratch capacity must allocate before activation")
    }

    pub fn try_new(net: DeterministicNet) -> Result<Self, ResourceViolation> {
        Ok(Self::with_allocator(
            net,
            Box::new(HostBufferAllocator::try_default()?),
        ))
    }

    pub fn with_allocator(
        net: DeterministicNet,
        allocator: Box<dyn BoundedBufferAllocator>,
    ) -> Self {
        Self { net, allocator }
    }

    pub fn net(&self) -> &DeterministicNet {
        &self.net
    }

    pub fn net_mut(&mut self) -> &mut DeterministicNet {
        &mut self.net
    }

    pub fn scratch_capacity_observations(&self) -> ScratchPoolObservations {
        self.allocator.observations()
    }

    fn process_tx_one(
        &mut self,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> Result<bool, VirtioFailure> {
        let Some(available) = queue.plan_next(mem)? else {
            return Ok(false);
        };
        let plan = plan_net_request(&available.chain, NetDirection::Transmit, 0, queue.limits())
            .map_err(VirtioFailure::Request)?;
        let packet_bytes = usize::try_from(plan.packet_bytes)
            .map_err(|_| VirtioFailure::Resource(ResourceViolation::Allocation { requested: 0 }))?;
        let maximum = usize::try_from(queue.limits().max_net_frame_bytes).map_err(|_| {
            VirtioFailure::Resource(ResourceViolation::ScratchLimit {
                requested: packet_bytes,
                maximum: usize::MAX,
            })
        })?;
        let limits = queue.limits();
        let allocator = &mut *self.allocator;
        let net = &mut self.net;
        with_zeroed_scratch(allocator, packet_bytes, maximum, |packet| {
            read_tx_packet(mem, &available.chain, packet)?;
            net.validate_tx_retention(
                packet_bytes,
                limits.max_net_tx_packets,
                limits.max_net_tx_bytes,
            )
            .map_err(|error| map_queue_error(error, packet_bytes))?;
            queue.stage_completion(available.head_index, EMPTY_USED_BYTES)?;
            queue.mark_backend_started()?;
            net.try_enqueue_tx_bounded(packet, limits.max_net_tx_packets, limits.max_net_tx_bytes)
                .map_err(|error| map_queue_error(error, packet_bytes))?;
            queue.complete(mem, available.head_index, EMPTY_USED_BYTES)?;
            Ok(true)
        })
    }

    fn process_rx_one(
        &mut self,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> Result<bool, VirtioFailure> {
        let Some(packet) = self.net.peek_rx() else {
            return Ok(false);
        };
        let packet_bytes = u64::try_from(packet.len()).map_err(|_| VirtioFailure::BackendRead)?;
        let Some(available) = queue.plan_next(mem)? else {
            return Ok(false);
        };
        let plan = plan_net_request(
            &available.chain,
            NetDirection::Receive,
            packet_bytes,
            queue.limits(),
        )
        .map_err(VirtioFailure::Request)?;
        queue.stage_completion(available.head_index, plan.used_bytes)?;
        queue.mark_effects_started()?;
        write_rx_packet(mem, &available.chain, packet)?;
        queue.mark_backend_started()?;
        let removed = self.net.pop_rx().ok_or(VirtioFailure::BackendQueue)?;
        debug_assert_eq!(removed.len(), packet_bytes as usize);
        queue.complete(mem, available.head_index, plan.used_bytes)?;
        Ok(true)
    }
}

impl VirtioBackend for VirtioNet {
    fn device_id(&self) -> u32 {
        VIRTIO_NET_DEVICE_ID
    }

    fn device_features(&self) -> u64 {
        VIRTIO_NET_F_MAC
    }

    fn num_queues(&self) -> usize {
        VIRTIO_NET_QUEUE_COUNT
    }

    fn process_queue(
        &mut self,
        queue_index: usize,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> Result<bool, VirtioFailure> {
        let mut completed = false;
        match queue_index {
            RX_QUEUE_INDEX => {
                while self.net.has_rx_data() && self.process_rx_one(queue, mem)? {
                    completed = true;
                }
            }
            TX_QUEUE_INDEX => {
                while self.process_tx_one(queue, mem)? {
                    completed = true;
                }
            }
            _ => return Err(VirtioFailure::BackendQueue),
        }
        Ok(completed)
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        data.fill(0);
        if offset < MAC_BYTES as u64 {
            let start = usize::try_from(offset).unwrap_or(MAC_BYTES);
            let end = start.saturating_add(data.len()).min(MAC_BYTES);
            let copy_length = end.saturating_sub(start);
            data[..copy_length].copy_from_slice(&self.net.mac()[start..end]);
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

fn map_queue_error(error: NetQueueError, packet_bytes: usize) -> VirtioFailure {
    match error {
        NetQueueError::PacketLimit { requested, maximum } => {
            VirtioFailure::Resource(ResourceViolation::RetainedPacketLimit { requested, maximum })
        }
        NetQueueError::ByteLimit { requested, maximum } => {
            VirtioFailure::Resource(ResourceViolation::RetainedByteLimit { requested, maximum })
        }
        NetQueueError::FrameLimit { requested, maximum } => {
            VirtioFailure::Resource(ResourceViolation::ScratchLimit { requested, maximum })
        }
        NetQueueError::SlotExhausted => {
            VirtioFailure::Resource(ResourceViolation::RetainedPacketSlotsExhausted)
        }
        NetQueueError::Allocation => VirtioFailure::Resource(ResourceViolation::Allocation {
            requested: packet_bytes,
        }),
        NetQueueError::Arithmetic | NetQueueError::PostCommit => VirtioFailure::BackendWrite,
    }
}

fn read_tx_packet(
    mem: &GuestMemoryMmap,
    chain: &DescriptorChainPlan,
    packet: &mut [u8],
) -> Result<(), VirtioFailure> {
    let mut packet_offset = 0usize;
    for (buffer_index, buffer) in chain.buffers().iter().enumerate() {
        let skip = if buffer_index == 0 {
            usize::try_from(NET_HEADER_BYTES).map_err(|_| VirtioFailure::GuestMemoryRead)?
        } else {
            0
        };
        let buffer_length =
            usize::try_from(buffer.len).map_err(|_| VirtioFailure::GuestMemoryRead)?;
        let read_length = buffer_length.saturating_sub(skip);
        if read_length == 0 {
            continue;
        }
        let end = packet_offset
            .checked_add(read_length)
            .ok_or(VirtioFailure::GuestMemoryRead)?;
        let address = buffer
            .addr
            .checked_add(u64::try_from(skip).map_err(|_| VirtioFailure::GuestMemoryRead)?)
            .ok_or(VirtioFailure::GuestMemoryRead)?;
        mem.read_slice(&mut packet[packet_offset..end], GuestAddress(address))
            .map_err(|_| VirtioFailure::GuestMemoryRead)?;
        packet_offset = end;
    }
    if packet_offset != packet.len() {
        return Err(VirtioFailure::GuestMemoryRead);
    }
    Ok(())
}

fn write_rx_packet(
    mem: &GuestMemoryMmap,
    chain: &DescriptorChainPlan,
    packet: &[u8],
) -> Result<(), VirtioFailure> {
    let header_bytes =
        usize::try_from(NET_HEADER_BYTES).map_err(|_| VirtioFailure::GuestMemoryWrite)?;
    let first = chain
        .buffers()
        .first()
        .ok_or(VirtioFailure::GuestMemoryWrite)?;
    let header = [0u8; NET_HEADER_BYTES as usize];
    mem.write_slice(&header, GuestAddress(first.addr))
        .map_err(|_| VirtioFailure::GuestMemoryWrite)?;

    let mut packet_offset = 0usize;
    for (buffer_index, buffer) in chain.buffers().iter().enumerate() {
        let skip = if buffer_index == 0 { header_bytes } else { 0 };
        let buffer_length =
            usize::try_from(buffer.len).map_err(|_| VirtioFailure::GuestMemoryWrite)?;
        let capacity = buffer_length.saturating_sub(skip);
        let remaining = packet.len().saturating_sub(packet_offset);
        let write_length = capacity.min(remaining);
        if write_length == 0 {
            continue;
        }
        let end = packet_offset
            .checked_add(write_length)
            .ok_or(VirtioFailure::GuestMemoryWrite)?;
        let address = buffer
            .addr
            .checked_add(u64::try_from(skip).map_err(|_| VirtioFailure::GuestMemoryWrite)?)
            .ok_or(VirtioFailure::GuestMemoryWrite)?;
        mem.write_slice(&packet[packet_offset..end], GuestAddress(address))
            .map_err(|_| VirtioFailure::GuestMemoryWrite)?;
        packet_offset = end;
    }
    if packet_offset != packet.len() {
        return Err(VirtioFailure::GuestMemoryWrite);
    }
    Ok(())
}
