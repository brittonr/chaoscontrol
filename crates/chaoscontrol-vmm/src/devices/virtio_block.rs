//! Virtio block device backend for deterministic disk I/O.
//!
//! Implements the virtio-blk device specification on top of the
//! in-memory [`DeterministicBlock`] storage backend.

use super::block::DeterministicBlock;
use super::virtio_mmio::{walk_descriptor_chain, VirtQueue, VirtioBackend};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

// ═══════════════════════════════════════════════════════════════════════
//  Virtio Block Constants
// ═══════════════════════════════════════════════════════════════════════

/// Virtio device type ID for block devices.
const VIRTIO_BLK_DEVICE_ID: u32 = 2;

/// Feature: Maximum segment size (in bytes).
const VIRTIO_BLK_F_SIZE_MAX: u64 = 1 << 1;

/// Feature: Maximum number of segments.
const VIRTIO_BLK_F_SEG_MAX: u64 = 1 << 2;

/// Sector size in bytes (virtio-blk always uses 512-byte sectors).
const SECTOR_SIZE: u64 = 512;

/// Request types (virtio spec §5.2.6).
const VIRTIO_BLK_T_IN: u32 = 0; // Read from disk
const VIRTIO_BLK_T_OUT: u32 = 1; // Write to disk
                                 // const VIRTIO_BLK_T_FLUSH: u32 = 4;   // Flush request (not implemented)

/// Status codes (virtio spec §5.2.6.1).
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
// const VIRTIO_BLK_S_UNSUPP: u8 = 2;

// ═══════════════════════════════════════════════════════════════════════
//  Virtio Block Request Header
// ═══════════════════════════════════════════════════════════════════════

/// The virtio-blk request header (16 bytes, spec §5.2.6).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct VirtioBlkReq {
    /// Request type (IN=read, OUT=write, FLUSH=flush).
    type_: u32,
    /// Reserved (must be zero).
    _reserved: u32,
    /// Sector number (512-byte units).
    sector: u64,
}

// ═══════════════════════════════════════════════════════════════════════
//  VirtioBlock Backend
// ═══════════════════════════════════════════════════════════════════════

/// Virtio block device backend.
///
/// Wraps a [`DeterministicBlock`] and implements the virtio-blk protocol:
/// descriptor chains contain a request header, data buffers, and a status byte.
pub struct VirtioBlock {
    /// Underlying deterministic storage.
    disk: DeterministicBlock,
    /// Number of 512-byte sectors.
    num_sectors: u64,
}

impl VirtioBlock {
    /// Create a new virtio-blk device with the given storage.
    pub fn new(disk: DeterministicBlock) -> Self {
        let num_sectors = disk.size() / SECTOR_SIZE;
        Self { disk, num_sectors }
    }

    /// Get a reference to the underlying disk.
    pub fn disk(&self) -> &DeterministicBlock {
        &self.disk
    }

    /// Get a mutable reference to the underlying disk.
    pub fn disk_mut(&mut self) -> &mut DeterministicBlock {
        &mut self.disk
    }

    /// Process a single virtio-blk request descriptor chain.
    ///
    /// Returns the number of bytes written (for the used ring).
    fn process_request(
        &mut self,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
        head_idx: u16,
    ) -> u32 {
        let buffers = match walk_descriptor_chain(queue, mem, head_idx) {
            Some(b) => b,
            None => {
                // Malformed descriptor chain
                return 0;
            }
        };

        if buffers.len() < 2 {
            // Need at least: header (read) + status (write)
            return 0;
        }

        // First buffer: request header (device reads)
        let hdr_buf = &buffers[0];
        if hdr_buf.write || hdr_buf.len < 16 {
            return 0;
        }

        let req = VirtioBlkReq {
            type_: match mem.read_obj(GuestAddress(hdr_buf.addr)) {
                Ok(v) => v,
                Err(_) => return 0,
            },
            _reserved: match mem.read_obj(GuestAddress(hdr_buf.addr + 4)) {
                Ok(v) => v,
                Err(_) => return 0,
            },
            sector: match mem.read_obj(GuestAddress(hdr_buf.addr + 8)) {
                Ok(v) => v,
                Err(_) => return 0,
            },
        };

        // Last buffer: status byte (device writes)
        let status_buf = &buffers[buffers.len() - 1];
        if !status_buf.write || status_buf.len < 1 {
            return 0;
        }

        // Middle buffers: data
        let data_buffers = &buffers[1..buffers.len() - 1];

        let status = match req.type_ {
            VIRTIO_BLK_T_IN => {
                // Read from disk
                self.handle_read(mem, req.sector, data_buffers)
            }
            VIRTIO_BLK_T_OUT => {
                // Write to disk
                self.handle_write(mem, req.sector, data_buffers)
            }
            _ => VIRTIO_BLK_S_IOERR, // Unsupported request type
        };

        // Write status byte
        let _ = mem.write_obj(status, GuestAddress(status_buf.addr));

        // Return total bytes written (just the status byte for now)
        1
    }

    fn handle_read(
        &mut self,
        mem: &GuestMemoryMmap,
        sector: u64,
        buffers: &[super::virtio_mmio::DescriptorBuffer],
    ) -> u8 {
        let offset = sector * SECTOR_SIZE;
        let mut current_offset = offset;

        for buf in buffers {
            if !buf.write {
                return VIRTIO_BLK_S_IOERR; // Expected write buffer for read data
            }

            let mut data = vec![0u8; buf.len as usize];
            if self.disk.read(current_offset, &mut data).is_err() {
                return VIRTIO_BLK_S_IOERR;
            }

            if mem.write_slice(&data, GuestAddress(buf.addr)).is_err() {
                return VIRTIO_BLK_S_IOERR;
            }

            current_offset += buf.len as u64;
        }

        VIRTIO_BLK_S_OK
    }

    fn handle_write(
        &mut self,
        mem: &GuestMemoryMmap,
        sector: u64,
        buffers: &[super::virtio_mmio::DescriptorBuffer],
    ) -> u8 {
        let offset = sector * SECTOR_SIZE;
        let mut current_offset = offset;

        for buf in buffers {
            if buf.write {
                return VIRTIO_BLK_S_IOERR; // Expected read buffer for write data
            }

            let mut data = vec![0u8; buf.len as usize];
            if mem.read_slice(&mut data, GuestAddress(buf.addr)).is_err() {
                return VIRTIO_BLK_S_IOERR;
            }

            if self.disk.write(current_offset, &data).is_err() {
                return VIRTIO_BLK_S_IOERR;
            }

            current_offset += buf.len as u64;
        }

        VIRTIO_BLK_S_OK
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
        1 // Single request queue
    }

    fn process_queue(
        &mut self,
        _queue_idx: usize,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> bool {
        let mut work_done = false;

        // Process all available descriptor chains
        while let Some(head_idx) = queue.pop_avail(mem) {
            let len = self.process_request(queue, mem, head_idx);
            queue.add_used(mem, head_idx, len);
            work_done = true;
        }

        work_done
    }

    fn read_config(&self, offset: u64, data: &mut [u8]) {
        // Config space layout (spec §5.2.4):
        //   u64 capacity (in 512-byte sectors)
        //   u32 size_max (optional, depends on VIRTIO_BLK_F_SIZE_MAX)
        //   u32 seg_max  (optional, depends on VIRTIO_BLK_F_SEG_MAX)
        //   ... (more fields we don't implement)

        if offset < 8 {
            // capacity field (u64 at offset 0)
            let capacity_bytes = self.num_sectors.to_le_bytes();
            let start = offset as usize;
            let end = (start + data.len()).min(8);
            let copy_len = end - start;
            data[..copy_len].copy_from_slice(&capacity_bytes[start..end]);
        } else {
            // Zero out other fields
            for byte in data {
                *byte = 0;
            }
        }
    }

    fn write_config(&mut self, _offset: u64, _data: &[u8]) {
        // Config space is read-only for virtio-blk
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtio_block_device_id() {
        let disk = DeterministicBlock::new(4096);
        let blk = VirtioBlock::new(disk);
        assert_eq!(blk.device_id(), VIRTIO_BLK_DEVICE_ID);
    }

    #[test]
    fn virtio_block_features() {
        let disk = DeterministicBlock::new(4096);
        let blk = VirtioBlock::new(disk);
        let features = blk.device_features();
        assert_ne!(features & VIRTIO_BLK_F_SIZE_MAX, 0);
        assert_ne!(features & VIRTIO_BLK_F_SEG_MAX, 0);
    }

    #[test]
    fn virtio_block_num_queues() {
        let disk = DeterministicBlock::new(4096);
        let blk = VirtioBlock::new(disk);
        assert_eq!(blk.num_queues(), 1);
    }

    #[test]
    fn virtio_block_capacity_config() {
        let disk = DeterministicBlock::new(8 * 1024 * 1024); // 8 MB
        let blk = VirtioBlock::new(disk);

        let mut capacity = [0u8; 8];
        blk.read_config(0, &mut capacity);
        let num_sectors = u64::from_le_bytes(capacity);
        assert_eq!(num_sectors, (8 * 1024 * 1024) / 512);
    }

    #[test]
    fn virtio_block_config_partial_read() {
        let disk = DeterministicBlock::new(4096);
        let blk = VirtioBlock::new(disk);

        // Read just the first 4 bytes of the capacity field
        let mut buf = [0u8; 4];
        blk.read_config(0, &mut buf);
        let expected = ((4096u64 / 512) as u32).to_le_bytes();
        assert_eq!(buf, expected);
    }
}
