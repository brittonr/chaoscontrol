//! Virtio network device backend for deterministic networking.
//!
//! Implements the virtio-net device specification on top of the
//! in-memory [`DeterministicNet`] backend.

use super::net::DeterministicNet;
use super::virtio_mmio::{walk_descriptor_chain, VirtQueue, VirtioBackend};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

// ═══════════════════════════════════════════════════════════════════════
//  Virtio Net Constants
// ═══════════════════════════════════════════════════════════════════════

/// Virtio device type ID for network devices.
const VIRTIO_NET_DEVICE_ID: u32 = 1;

/// Feature: Device has a MAC address.
const VIRTIO_NET_F_MAC: u64 = 1 << 5;

/// Queue indices.
const RX_QUEUE_IDX: usize = 0;
const TX_QUEUE_IDX: usize = 1;

// ═══════════════════════════════════════════════════════════════════════
//  VirtioNet Backend
// ═══════════════════════════════════════════════════════════════════════

/// Virtio network device backend.
///
/// Wraps a [`DeterministicNet`] and implements the virtio-net protocol.
/// The device has two queues: receiveq (0) and transmitq (1).
pub struct VirtioNet {
    /// Underlying deterministic network device.
    net: DeterministicNet,
}

impl VirtioNet {
    /// Create a new virtio-net device with the given network backend.
    pub fn new(net: DeterministicNet) -> Self {
        Self { net }
    }

    /// Get a reference to the underlying network device.
    pub fn net(&self) -> &DeterministicNet {
        &self.net
    }

    /// Get a mutable reference to the underlying network device.
    pub fn net_mut(&mut self) -> &mut DeterministicNet {
        &mut self.net
    }

    /// Process RX queue: deliver packets from the RX buffer to guest.
    fn process_rx(&mut self, queue: &mut VirtQueue, mem: &GuestMemoryMmap) -> bool {
        let mut work_done = false;

        while self.net.has_rx_data() {
            let head_idx = match queue.pop_avail(mem) {
                Some(idx) => idx,
                None => break, // No more buffers available
            };

            let packet = match self.net.pop_rx() {
                Some(p) => p,
                None => break,
            };

            let buffers = match walk_descriptor_chain(queue, mem, head_idx) {
                Some(b) => b,
                None => {
                    // Malformed chain, skip
                    queue.add_used(mem, head_idx, 0);
                    work_done = true;
                    continue;
                }
            };

            // Write packet data into device-writable buffers
            let mut bytes_written = 0;
            let mut packet_offset = 0;

            for buf in &buffers {
                if !buf.write {
                    continue; // Skip read-only buffers
                }

                let chunk_len = (buf.len as usize).min(packet.len() - packet_offset);
                if chunk_len == 0 {
                    break;
                }

                let chunk = &packet[packet_offset..packet_offset + chunk_len];
                if mem.write_slice(chunk, GuestAddress(buf.addr)).is_err() {
                    break;
                }

                bytes_written += chunk_len as u32;
                packet_offset += chunk_len;

                if packet_offset >= packet.len() {
                    break;
                }
            }

            queue.add_used(mem, head_idx, bytes_written);
            work_done = true;
        }

        work_done
    }

    /// Process TX queue: collect packets from guest and enqueue them.
    fn process_tx(&mut self, queue: &mut VirtQueue, mem: &GuestMemoryMmap) -> bool {
        let mut work_done = false;

        while let Some(head_idx) = queue.pop_avail(mem) {
            let buffers = match walk_descriptor_chain(queue, mem, head_idx) {
                Some(b) => b,
                None => {
                    queue.add_used(mem, head_idx, 0);
                    work_done = true;
                    continue;
                }
            };

            // Collect packet data from read-only buffers
            let mut packet = Vec::new();
            for buf in &buffers {
                if buf.write {
                    continue; // Skip write-only buffers
                }

                let mut chunk = vec![0u8; buf.len as usize];
                if mem.read_slice(&mut chunk, GuestAddress(buf.addr)).is_err() {
                    continue;
                }
                packet.extend_from_slice(&chunk);
            }

            if !packet.is_empty() {
                self.net.enqueue_tx(packet);
            }

            queue.add_used(mem, head_idx, 0);
            work_done = true;
        }

        work_done
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
        2 // RX and TX
    }

    fn process_queue(
        &mut self,
        queue_idx: usize,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> bool {
        match queue_idx {
            RX_QUEUE_IDX => self.process_rx(queue, mem),
            TX_QUEUE_IDX => self.process_tx(queue, mem),
            _ => false,
        }
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        // Config space layout (spec §5.1.4):
        //   u8[6] mac
        //   u16 status
        //   u16 max_virtqueue_pairs
        //   u16 mtu
        //   ... (more fields we don't implement)

        if offset < 6 {
            // MAC address
            let mac = self.net.mac();
            let start = offset as usize;
            let end = (start + data.len()).min(6);
            let copy_len = end - start;
            data[..copy_len].copy_from_slice(&mac[start..end]);
        } else {
            // Zero out other fields
            for byte in data {
                *byte = 0;
            }
        }
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {
        // Config space is read-only for virtio-net
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];

    #[test]
    fn virtio_net_device_id() {
        let net_dev = DeterministicNet::new(TEST_MAC);
        let vnet = VirtioNet::new(net_dev);
        assert_eq!(vnet.device_id(), VIRTIO_NET_DEVICE_ID);
    }

    #[test]
    fn virtio_net_features() {
        let net_dev = DeterministicNet::new(TEST_MAC);
        let vnet = VirtioNet::new(net_dev);
        let features = vnet.device_features();
        assert_ne!(features & VIRTIO_NET_F_MAC, 0);
    }

    #[test]
    fn virtio_net_num_queues() {
        let net_dev = DeterministicNet::new(TEST_MAC);
        let vnet = VirtioNet::new(net_dev);
        assert_eq!(vnet.num_queues(), 2);
    }

    #[test]
    fn virtio_net_mac_config() {
        let net_dev = DeterministicNet::new(TEST_MAC);
        let vnet = VirtioNet::new(net_dev);

        let mut mac = [0u8; 6];
        vnet.read_config(0, &mut mac);
        assert_eq!(mac, TEST_MAC);
    }

    #[test]
    fn virtio_net_config_partial_read() {
        let net_dev = DeterministicNet::new(TEST_MAC);
        let vnet = VirtioNet::new(net_dev);

        // Read just the first 3 bytes of the MAC
        let mut buf = [0u8; 3];
        vnet.read_config(0, &mut buf);
        assert_eq!(buf, [0x02, 0x00, 0x00]);

        // Read bytes 3-5
        let mut buf2 = [0u8; 3];
        vnet.read_config(3, &mut buf2);
        assert_eq!(buf2, [0x00, 0x00, 0x01]);
    }
}
