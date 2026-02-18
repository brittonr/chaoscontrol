//! Virtio entropy device backend for deterministic random number generation.
//!
//! Implements the virtio-rng device specification on top of the
//! deterministic [`DeterministicEntropy`] source.

use super::entropy::DeterministicEntropy;
use super::virtio_mmio::{walk_descriptor_chain, VirtQueue, VirtioBackend};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

// ═══════════════════════════════════════════════════════════════════════
//  Virtio Entropy Constants
// ═══════════════════════════════════════════════════════════════════════

/// Virtio device type ID for entropy/RNG devices.
const VIRTIO_ENTROPY_DEVICE_ID: u32 = 4;

// ═══════════════════════════════════════════════════════════════════════
//  VirtioEntropy Backend
// ═══════════════════════════════════════════════════════════════════════

/// Virtio entropy device backend.
///
/// Wraps a [`DeterministicEntropy`] source and implements the virtio-rng
/// protocol: the guest submits write-only buffers, and the device fills
/// them with deterministic pseudo-random bytes.
pub struct VirtioEntropy {
    /// Underlying deterministic entropy source.
    entropy: DeterministicEntropy,
}

impl VirtioEntropy {
    /// Create a new virtio-rng device with the given entropy source.
    pub fn new(entropy: DeterministicEntropy) -> Self {
        Self { entropy }
    }

    /// Get a reference to the underlying entropy source.
    pub fn entropy(&self) -> &DeterministicEntropy {
        &self.entropy
    }

    /// Get a mutable reference to the underlying entropy source.
    pub fn entropy_mut(&mut self) -> &mut DeterministicEntropy {
        &mut self.entropy
    }

    /// Process a single entropy request descriptor chain.
    fn process_request(
        &mut self,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
        head_idx: u16,
    ) -> u32 {
        let buffers = match walk_descriptor_chain(queue, mem, head_idx) {
            Some(b) => b,
            None => return 0,
        };

        let mut total_written = 0u32;

        for buf in &buffers {
            if !buf.write {
                continue; // Skip read-only buffers
            }

            let mut data = vec![0u8; buf.len as usize];
            self.entropy.fill_bytes(&mut data);

            if mem.write_slice(&data, GuestAddress(buf.addr)).is_err() {
                break;
            }

            total_written += buf.len;
        }

        total_written
    }
}

impl VirtioBackend for VirtioEntropy {
    fn device_id(&self) -> u32 {
        VIRTIO_ENTROPY_DEVICE_ID
    }

    fn device_features(&self) -> u64 {
        0 // No optional features
    }

    fn num_queues(&self) -> usize {
        1 // Single request queue
    }

    fn process_queue(
        &mut self,
        _queue_idx: usize,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> bool {
        let mut work_done = false;

        while let Some(head_idx) = queue.pop_avail(mem) {
            let len = self.process_request(queue, mem, head_idx);
            queue.add_used(mem, head_idx, len);
            work_done = true;
        }

        work_done
    }

    fn read_config(&self, _offset: u64, data: &mut [u8]) {
        // Virtio-rng has no device-specific config space
        for byte in data {
            *byte = 0;
        }
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {
        // No writable config space
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtio_entropy_device_id() {
        let ent = DeterministicEntropy::new(42);
        let vent = VirtioEntropy::new(ent);
        assert_eq!(vent.device_id(), VIRTIO_ENTROPY_DEVICE_ID);
    }

    #[test]
    fn virtio_entropy_features() {
        let ent = DeterministicEntropy::new(42);
        let vent = VirtioEntropy::new(ent);
        assert_eq!(vent.device_features(), 0);
    }

    #[test]
    fn virtio_entropy_num_queues() {
        let ent = DeterministicEntropy::new(42);
        let vent = VirtioEntropy::new(ent);
        assert_eq!(vent.num_queues(), 1);
    }

    #[test]
    fn virtio_entropy_deterministic_output() {
        let ent1 = DeterministicEntropy::new(123);
        let ent2 = DeterministicEntropy::new(123);
        let mut vent1 = VirtioEntropy::new(ent1);
        let mut vent2 = VirtioEntropy::new(ent2);

        // Generate some random bytes from both
        let mut buf1 = [0u8; 64];
        let mut buf2 = [0u8; 64];
        vent1.entropy_mut().fill_bytes(&mut buf1);
        vent2.entropy_mut().fill_bytes(&mut buf2);

        // Same seed ⇒ same output
        assert_eq!(buf1, buf2);
    }
}
