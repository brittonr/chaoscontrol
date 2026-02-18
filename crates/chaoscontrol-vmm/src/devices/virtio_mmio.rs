//! Virtio MMIO transport layer for deterministic device I/O.
//!
//! Implements the virtio 1.2 MMIO register interface (version 2, legacy-free)
//! so guest virtio drivers can discover devices, negotiate features, set up
//! virtqueues, and perform I/O through descriptor chains.
//!
//! This module provides the transport layer only — actual device logic lives
//! in separate backend implementations ([`VirtioBackend`] trait).

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

// ═══════════════════════════════════════════════════════════════════════
//  Virtio MMIO Register Offsets (virtio 1.2 spec §4.2.2)
// ═══════════════════════════════════════════════════════════════════════

const VIRTIO_MMIO_MAGIC_VALUE: u64 = 0x000;
const VIRTIO_MMIO_VERSION: u64 = 0x004;
const VIRTIO_MMIO_DEVICE_ID: u64 = 0x008;
const VIRTIO_MMIO_VENDOR_ID: u64 = 0x00C;
const VIRTIO_MMIO_DEVICE_FEATURES: u64 = 0x010;
const VIRTIO_MMIO_DEVICE_FEATURES_SEL: u64 = 0x014;
const VIRTIO_MMIO_DRIVER_FEATURES: u64 = 0x020;
const VIRTIO_MMIO_DRIVER_FEATURES_SEL: u64 = 0x024;
const VIRTIO_MMIO_QUEUE_SEL: u64 = 0x030;
const VIRTIO_MMIO_QUEUE_NUM_MAX: u64 = 0x034;
const VIRTIO_MMIO_QUEUE_NUM: u64 = 0x038;
const VIRTIO_MMIO_QUEUE_READY: u64 = 0x044;
const VIRTIO_MMIO_QUEUE_NOTIFY: u64 = 0x050;
const VIRTIO_MMIO_INTERRUPT_STATUS: u64 = 0x060;
const VIRTIO_MMIO_INTERRUPT_ACK: u64 = 0x064;
const VIRTIO_MMIO_STATUS: u64 = 0x070;
const VIRTIO_MMIO_QUEUE_DESC_LOW: u64 = 0x080;
const VIRTIO_MMIO_QUEUE_DESC_HIGH: u64 = 0x084;
const VIRTIO_MMIO_QUEUE_DRIVER_LOW: u64 = 0x090;
const VIRTIO_MMIO_QUEUE_DRIVER_HIGH: u64 = 0x094;
const VIRTIO_MMIO_QUEUE_DEVICE_LOW: u64 = 0x0A0;
const VIRTIO_MMIO_QUEUE_DEVICE_HIGH: u64 = 0x0A4;
const VIRTIO_MMIO_CONFIG_GENERATION: u64 = 0x0FC;
const VIRTIO_MMIO_CONFIG: u64 = 0x100;

// ═══════════════════════════════════════════════════════════════════════
//  Constants
// ═══════════════════════════════════════════════════════════════════════

/// Magic value returned by VIRTIO_MMIO_MAGIC_VALUE register ("virt" in LE).
const MAGIC: u32 = 0x74726976;

/// Version number for modern (non-legacy) virtio MMIO.
const VERSION: u32 = 2;

/// Vendor ID (we use QEMU's vendor ID for compatibility).
const VENDOR_ID: u32 = 0x554D4551;

/// Maximum virtqueue size we support (power of 2).
const QUEUE_SIZE_MAX: u16 = 256;

/// Size of a virtio MMIO device region (4 KB).
pub const VIRTIO_MMIO_DEVICE_SIZE: u64 = 0x1000;

/// Virtqueue descriptor flags (virtio spec §2.7.5).
const VIRTQ_DESC_F_NEXT: u16 = 1; // Descriptor continues via `next` field
const VIRTQ_DESC_F_WRITE: u16 = 2; // Buffer is write-only (device writes)
                                   // const VIRTQ_DESC_F_INDIRECT: u16 = 4;  // Reserved for future use

/// Device status bits (virtio spec §2.1).
#[allow(dead_code)]
const VIRTIO_STATUS_ACKNOWLEDGE: u32 = 1;
#[allow(dead_code)]
const VIRTIO_STATUS_DRIVER: u32 = 2;
#[allow(dead_code)]
const VIRTIO_STATUS_FEATURES_OK: u32 = 8;
#[allow(dead_code)]
const VIRTIO_STATUS_DRIVER_OK: u32 = 4;
#[allow(dead_code)]
const VIRTIO_STATUS_FAILED: u32 = 128;

/// Interrupt status bits.
const VIRTIO_MMIO_INT_VRING: u32 = 1; // Used buffer notification

// ═══════════════════════════════════════════════════════════════════════
//  Virtio Split Virtqueue Descriptor
// ═══════════════════════════════════════════════════════════════════════

/// A single virtqueue descriptor (16 bytes, virtio spec §2.7.5).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VirtqDesc {
    /// Guest physical address of the buffer.
    pub addr: u64,
    /// Length of the buffer in bytes.
    pub len: u32,
    /// Descriptor flags (NEXT, WRITE, INDIRECT).
    pub flags: u16,
    /// Index of the next descriptor (if NEXT flag is set).
    pub next: u16,
}

// ═══════════════════════════════════════════════════════════════════════
//  VirtQueue
// ═══════════════════════════════════════════════════════════════════════

/// A single split virtqueue.
///
/// The guest driver sets up three memory regions:
/// - Descriptor table (array of [`VirtqDesc`])
/// - Available ring (driver → device notifications)
/// - Used ring (device → driver completions)
#[derive(Clone, Debug)]
pub struct VirtQueue {
    /// Maximum queue size offered by the device.
    max_size: u16,
    /// Actual queue size set by the driver.
    size: u16,
    /// Is the queue activated (ready for I/O)?
    ready: bool,
    /// Guest physical address of the descriptor table.
    desc_addr: u64,
    /// Guest physical address of the available ring.
    driver_addr: u64,
    /// Guest physical address of the used ring.
    device_addr: u64,
    /// Last `avail.idx` we've seen (for tracking new buffers).
    last_avail_idx: u16,
}

impl VirtQueue {
    /// Create a new uninitialized virtqueue.
    pub fn new(max_size: u16) -> Self {
        Self {
            max_size,
            size: 0,
            ready: false,
            desc_addr: 0,
            driver_addr: 0,
            device_addr: 0,
            last_avail_idx: 0,
        }
    }

    /// Get the maximum queue size.
    pub fn max_size(&self) -> u16 {
        self.max_size
    }

    /// Set the queue size (driver configuration).
    pub fn set_size(&mut self, size: u16) {
        self.size = size.min(self.max_size);
    }

    /// Get the current queue size.
    pub fn size(&self) -> u16 {
        self.size
    }

    /// Activate the queue (make it ready for I/O).
    pub fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
    }

    /// Check if the queue is ready.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Set the descriptor table address.
    pub fn set_desc_addr(&mut self, addr: u64) {
        self.desc_addr = addr;
    }

    /// Set the available ring address.
    pub fn set_driver_addr(&mut self, addr: u64) {
        self.driver_addr = addr;
    }

    /// Set the used ring address.
    pub fn set_device_addr(&mut self, addr: u64) {
        self.device_addr = addr;
    }

    /// Read a descriptor from the guest's descriptor table.
    pub fn read_desc(&self, mem: &GuestMemoryMmap, idx: u16) -> Option<VirtqDesc> {
        if idx >= self.size {
            return None;
        }
        let addr = self.desc_addr + (idx as u64) * 16; // Each descriptor is 16 bytes
        let desc = VirtqDesc {
            addr: mem.read_obj(GuestAddress(addr)).ok()?,
            len: mem.read_obj(GuestAddress(addr + 8)).ok()?,
            flags: mem.read_obj(GuestAddress(addr + 12)).ok()?,
            next: mem.read_obj(GuestAddress(addr + 14)).ok()?,
        };
        Some(desc)
    }

    /// Read the `avail.idx` field from the available ring.
    pub fn read_avail_idx(&self, mem: &GuestMemoryMmap) -> Option<u16> {
        // Available ring layout:
        //   u16 flags
        //   u16 idx
        //   u16 ring[queue_size]
        //   u16 used_event (optional)
        let idx_addr = self.driver_addr + 2;
        mem.read_obj(GuestAddress(idx_addr)).ok()
    }

    /// Read an entry from the available ring at the given index.
    pub fn read_avail_ring(&self, mem: &GuestMemoryMmap, idx: u16) -> Option<u16> {
        let ring_start = self.driver_addr + 4;
        let offset = (idx % self.size) as u64;
        mem.read_obj(GuestAddress(ring_start + offset * 2)).ok()
    }

    /// Add a used buffer to the used ring and increment `used.idx`.
    pub fn add_used(&mut self, mem: &GuestMemoryMmap, desc_idx: u16, len: u32) -> bool {
        if !self.ready {
            return false;
        }

        // Used ring layout:
        //   u16 flags
        //   u16 idx
        //   struct virtq_used_elem ring[queue_size] {
        //     u32 id;   // descriptor chain head
        //     u32 len;  // total bytes written
        //   }
        //   u16 avail_event (optional)

        // Read current used.idx
        let idx_addr = self.device_addr + 2;
        let used_idx: u16 = match mem.read_obj(GuestAddress(idx_addr)) {
            Ok(v) => v,
            Err(_) => return false,
        };

        // Write used ring element
        let ring_start = self.device_addr + 4;
        let ring_offset = ((used_idx % self.size) as u64) * 8;
        let elem_addr = ring_start + ring_offset;

        if mem
            .write_obj(desc_idx as u32, GuestAddress(elem_addr))
            .is_err()
        {
            return false;
        }
        if mem.write_obj(len, GuestAddress(elem_addr + 4)).is_err() {
            return false;
        }

        // Increment used.idx
        let new_idx = used_idx.wrapping_add(1);
        if mem.write_obj(new_idx, GuestAddress(idx_addr)).is_err() {
            return false;
        }

        true
    }

    /// Check if there are new buffers available from the driver.
    pub fn has_new_buffers(&mut self, mem: &GuestMemoryMmap) -> bool {
        if !self.ready {
            return false;
        }
        let avail_idx = match self.read_avail_idx(mem) {
            Some(idx) => idx,
            None => return false,
        };
        avail_idx != self.last_avail_idx
    }

    /// Pop the next available descriptor chain head.
    pub fn pop_avail(&mut self, mem: &GuestMemoryMmap) -> Option<u16> {
        if !self.ready {
            return None;
        }
        let avail_idx = self.read_avail_idx(mem)?;
        if avail_idx == self.last_avail_idx {
            return None; // No new buffers
        }
        let desc_idx = self.read_avail_ring(mem, self.last_avail_idx)?;
        self.last_avail_idx = self.last_avail_idx.wrapping_add(1);
        Some(desc_idx)
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  VirtioBackend Trait
// ═══════════════════════════════════════════════════════════════════════

/// Trait for virtio device backends.
///
/// Each device type (block, net, rng) implements this trait to handle
/// device-specific I/O processing, feature negotiation, and config space.
pub trait VirtioBackend: Send {
    /// Device type ID (1=net, 2=block, 4=rng).
    fn device_id(&self) -> u32;

    /// Feature bits offered by the device.
    fn device_features(&self) -> u64;

    /// Number of virtqueues this device uses.
    fn num_queues(&self) -> usize;

    /// Process available buffers in the given queue.
    ///
    /// Returns `true` if work was done and an interrupt should be raised.
    fn process_queue(
        &mut self,
        queue_idx: usize,
        queue: &mut VirtQueue,
        mem: &GuestMemoryMmap,
    ) -> bool;

    /// Read from device-specific config space (offset relative to 0x100).
    fn read_config(&self, offset: u64, data: &mut [u8]);

    /// Write to device-specific config space (offset relative to 0x100).
    fn write_config(&mut self, offset: u64, data: &[u8]);

    /// Downcast to `Any` for backend-specific operations (e.g., fault injection).
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

// ═══════════════════════════════════════════════════════════════════════
//  VirtioMmioDevice
// ═══════════════════════════════════════════════════════════════════════

/// A virtio MMIO transport device.
///
/// Handles the MMIO register interface and delegates I/O to a backend.
pub struct VirtioMmioDevice {
    /// Base address in guest physical memory.
    base_addr: u64,
    /// IRQ line number.
    irq: u32,
    /// Device features (offered by backend).
    device_features: u64,
    /// Driver features (negotiated).
    driver_features: u64,
    /// Feature selection registers.
    device_features_sel: u32,
    driver_features_sel: u32,
    /// Device status byte.
    status: u32,
    /// Interrupt status.
    interrupt_status: u32,
    /// Config generation counter.
    config_generation: u32,
    /// Virtqueues.
    queues: Vec<VirtQueue>,
    /// Currently selected queue index.
    queue_sel: u32,
    /// Device backend.
    backend: Box<dyn VirtioBackend>,
}

impl VirtioMmioDevice {
    /// Create a new virtio MMIO device.
    pub fn new(base_addr: u64, irq: u32, backend: Box<dyn VirtioBackend>) -> Self {
        let device_features = backend.device_features();
        let num_queues = backend.num_queues();
        let queues = (0..num_queues)
            .map(|_| VirtQueue::new(QUEUE_SIZE_MAX))
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
        }
    }

    /// Check if this device handles the given guest physical address.
    pub fn handles(&self, addr: u64) -> bool {
        addr >= self.base_addr && addr < self.base_addr + VIRTIO_MMIO_DEVICE_SIZE
    }

    /// Get the IRQ line number.
    pub fn irq(&self) -> u32 {
        self.irq
    }

    /// Get the base address.
    pub fn base_addr(&self) -> u64 {
        self.base_addr
    }

    /// Get the backend (for testing and inspection).
    pub fn backend(&self) -> &dyn VirtioBackend {
        &*self.backend
    }

    /// Get a mutable reference to the backend (for testing and inspection).
    pub fn backend_mut(&mut self) -> &mut dyn VirtioBackend {
        &mut *self.backend
    }

    /// Handle an MMIO read from the guest.
    pub fn read(&self, offset: u64, data: &mut [u8]) {
        let val: u32 = match offset {
            VIRTIO_MMIO_MAGIC_VALUE => MAGIC,
            VIRTIO_MMIO_VERSION => VERSION,
            VIRTIO_MMIO_DEVICE_ID => self.backend.device_id(),
            VIRTIO_MMIO_VENDOR_ID => VENDOR_ID,
            VIRTIO_MMIO_DEVICE_FEATURES => {
                let sel = self.device_features_sel;
                if sel == 0 {
                    self.device_features as u32
                } else if sel == 1 {
                    (self.device_features >> 32) as u32
                } else {
                    0
                }
            }
            VIRTIO_MMIO_QUEUE_NUM_MAX => {
                if (self.queue_sel as usize) < self.queues.len() {
                    self.queues[self.queue_sel as usize].max_size() as u32
                } else {
                    0
                }
            }
            VIRTIO_MMIO_QUEUE_READY => {
                if (self.queue_sel as usize) < self.queues.len() {
                    self.queues[self.queue_sel as usize].is_ready() as u32
                } else {
                    0
                }
            }
            VIRTIO_MMIO_INTERRUPT_STATUS => self.interrupt_status,
            VIRTIO_MMIO_STATUS => self.status,
            VIRTIO_MMIO_CONFIG_GENERATION => self.config_generation,
            _ if offset >= VIRTIO_MMIO_CONFIG => {
                let cfg_offset = offset - VIRTIO_MMIO_CONFIG;
                self.backend.read_config(cfg_offset, data);
                return;
            }
            _ => 0,
        };

        // Write the u32 value to data buffer (little-endian)
        let bytes = val.to_le_bytes();
        let len = data.len().min(4);
        data[..len].copy_from_slice(&bytes[..len]);
    }

    /// Handle an MMIO write from the guest.
    pub fn write(&mut self, offset: u64, data: &[u8], _mem: &GuestMemoryMmap) {
        // Parse the write data as a u32 (little-endian)
        let mut buf = [0u8; 4];
        let len = data.len().min(4);
        buf[..len].copy_from_slice(&data[..len]);
        let val = u32::from_le_bytes(buf);

        match offset {
            VIRTIO_MMIO_DEVICE_FEATURES_SEL => {
                self.device_features_sel = val;
            }
            VIRTIO_MMIO_DRIVER_FEATURES => {
                let sel = self.driver_features_sel;
                if sel == 0 {
                    self.driver_features =
                        (self.driver_features & 0xFFFF_FFFF_0000_0000) | (val as u64);
                } else if sel == 1 {
                    self.driver_features =
                        (self.driver_features & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                }
            }
            VIRTIO_MMIO_DRIVER_FEATURES_SEL => {
                self.driver_features_sel = val;
            }
            VIRTIO_MMIO_QUEUE_SEL => {
                self.queue_sel = val;
            }
            VIRTIO_MMIO_QUEUE_NUM => {
                if (self.queue_sel as usize) < self.queues.len() {
                    self.queues[self.queue_sel as usize].set_size(val as u16);
                }
            }
            VIRTIO_MMIO_QUEUE_READY => {
                if (self.queue_sel as usize) < self.queues.len() {
                    self.queues[self.queue_sel as usize].set_ready(val != 0);
                }
            }
            VIRTIO_MMIO_QUEUE_DESC_LOW => {
                if (self.queue_sel as usize) < self.queues.len() {
                    let queue = &mut self.queues[self.queue_sel as usize];
                    let addr = (queue.desc_addr & 0xFFFF_FFFF_0000_0000) | (val as u64);
                    queue.set_desc_addr(addr);
                }
            }
            VIRTIO_MMIO_QUEUE_DESC_HIGH => {
                if (self.queue_sel as usize) < self.queues.len() {
                    let queue = &mut self.queues[self.queue_sel as usize];
                    let addr = (queue.desc_addr & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                    queue.set_desc_addr(addr);
                }
            }
            VIRTIO_MMIO_QUEUE_DRIVER_LOW => {
                if (self.queue_sel as usize) < self.queues.len() {
                    let queue = &mut self.queues[self.queue_sel as usize];
                    let addr = (queue.driver_addr & 0xFFFF_FFFF_0000_0000) | (val as u64);
                    queue.set_driver_addr(addr);
                }
            }
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH => {
                if (self.queue_sel as usize) < self.queues.len() {
                    let queue = &mut self.queues[self.queue_sel as usize];
                    let addr = (queue.driver_addr & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                    queue.set_driver_addr(addr);
                }
            }
            VIRTIO_MMIO_QUEUE_DEVICE_LOW => {
                if (self.queue_sel as usize) < self.queues.len() {
                    let queue = &mut self.queues[self.queue_sel as usize];
                    let addr = (queue.device_addr & 0xFFFF_FFFF_0000_0000) | (val as u64);
                    queue.set_device_addr(addr);
                }
            }
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH => {
                if (self.queue_sel as usize) < self.queues.len() {
                    let queue = &mut self.queues[self.queue_sel as usize];
                    let addr = (queue.device_addr & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                    queue.set_device_addr(addr);
                }
            }
            VIRTIO_MMIO_QUEUE_NOTIFY => {
                // Queue notification — driver has added buffers to the available ring.
                // We don't process inline here; the VM exit handler will call process_queues().
            }
            VIRTIO_MMIO_INTERRUPT_ACK => {
                self.interrupt_status &= !val;
            }
            VIRTIO_MMIO_STATUS => {
                self.status = val;
                // If driver writes 0, it's a device reset.
                if val == 0 {
                    self.reset();
                }
            }
            _ if offset >= VIRTIO_MMIO_CONFIG => {
                let cfg_offset = offset - VIRTIO_MMIO_CONFIG;
                self.backend.write_config(cfg_offset, data);
            }
            _ => {}
        }
    }

    /// Process all queues and return true if an interrupt should be raised.
    pub fn process_queues(&mut self, mem: &GuestMemoryMmap) -> bool {
        let mut work_done = false;
        for idx in 0..self.queues.len() {
            if self.backend.process_queue(idx, &mut self.queues[idx], mem) {
                work_done = true;
            }
        }
        if work_done {
            self.interrupt_status |= VIRTIO_MMIO_INT_VRING;
        }
        work_done
    }

    /// Check if an interrupt is pending.
    pub fn interrupt_pending(&self) -> bool {
        self.interrupt_status != 0
    }

    /// Reset the device to initial state.
    fn reset(&mut self) {
        self.driver_features = 0;
        self.device_features_sel = 0;
        self.driver_features_sel = 0;
        self.status = 0;
        self.interrupt_status = 0;
        self.queue_sel = 0;
        for queue in &mut self.queues {
            *queue = VirtQueue::new(QUEUE_SIZE_MAX);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Descriptor Chain Utilities
// ═══════════════════════════════════════════════════════════════════════

/// Iterate over a descriptor chain and collect read/write buffers.
pub fn walk_descriptor_chain(
    queue: &VirtQueue,
    mem: &GuestMemoryMmap,
    head_idx: u16,
) -> Option<Vec<DescriptorBuffer>> {
    let mut buffers = Vec::new();
    let mut idx = head_idx;
    let mut visited = std::collections::HashSet::new();

    loop {
        if visited.contains(&idx) {
            return None; // Cycle detected
        }
        visited.insert(idx);

        let desc = queue.read_desc(mem, idx)?;
        if desc.len > 0 {
            buffers.push(DescriptorBuffer {
                addr: desc.addr,
                len: desc.len,
                write: (desc.flags & VIRTQ_DESC_F_WRITE) != 0,
            });
        }

        if (desc.flags & VIRTQ_DESC_F_NEXT) == 0 {
            break;
        }
        idx = desc.next;
    }

    Some(buffers)
}

/// A buffer from a descriptor chain.
#[derive(Clone, Debug)]
pub struct DescriptorBuffer {
    pub addr: u64,
    pub len: u32,
    pub write: bool, // true = device writes, false = device reads
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyBackend {
        device_id: u32,
        features: u64,
        num_queues: usize,
    }

    impl VirtioBackend for DummyBackend {
        fn device_id(&self) -> u32 {
            self.device_id
        }
        fn device_features(&self) -> u64 {
            self.features
        }
        fn num_queues(&self) -> usize {
            self.num_queues
        }
        fn process_queue(
            &mut self,
            _queue_idx: usize,
            _queue: &mut VirtQueue,
            _mem: &GuestMemoryMmap,
        ) -> bool {
            false
        }
        fn read_config(&self, _offset: u64, data: &mut [u8]) {
            for byte in data {
                *byte = 0xAB;
            }
        }
        fn write_config(&mut self, _offset: u64, _data: &[u8]) {}
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    #[test]
    fn mmio_device_magic_and_version() {
        let backend = Box::new(DummyBackend {
            device_id: 2,
            features: 0,
            num_queues: 1,
        });
        let dev = VirtioMmioDevice::new(0xD000_0000, 5, backend);

        let mut buf = [0u8; 4];
        dev.read(VIRTIO_MMIO_MAGIC_VALUE, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), MAGIC);

        dev.read(VIRTIO_MMIO_VERSION, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), VERSION);
    }

    #[test]
    fn mmio_device_id_and_vendor() {
        let backend = Box::new(DummyBackend {
            device_id: 4,
            features: 0,
            num_queues: 1,
        });
        let dev = VirtioMmioDevice::new(0xD000_0000, 5, backend);

        let mut buf = [0u8; 4];
        dev.read(VIRTIO_MMIO_DEVICE_ID, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), 4);

        dev.read(VIRTIO_MMIO_VENDOR_ID, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), VENDOR_ID);
    }

    #[test]
    fn mmio_device_features() {
        let backend = Box::new(DummyBackend {
            device_id: 2,
            features: 0x0000_0001_0000_0002,
            num_queues: 1,
        });
        let dev = VirtioMmioDevice::new(0xD000_0000, 5, backend);

        let mut buf = [0u8; 4];

        // Select low 32 bits
        dev.read(VIRTIO_MMIO_DEVICE_FEATURES, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), 0x0000_0002);

        // Select high 32 bits
        let mut dev = dev;
        dev.write(
            VIRTIO_MMIO_DEVICE_FEATURES_SEL,
            &[1, 0, 0, 0],
            &GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 1024)]).unwrap(),
        );
        dev.read(VIRTIO_MMIO_DEVICE_FEATURES, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), 0x0000_0001);
    }

    #[test]
    fn queue_num_max() {
        let backend = Box::new(DummyBackend {
            device_id: 1,
            features: 0,
            num_queues: 2,
        });
        let dev = VirtioMmioDevice::new(0xD000_0000, 5, backend);

        let mut buf = [0u8; 4];
        dev.read(VIRTIO_MMIO_QUEUE_NUM_MAX, &mut buf);
        assert_eq!(u32::from_le_bytes(buf), QUEUE_SIZE_MAX as u32);
    }

    #[test]
    fn handles_address_range() {
        let backend = Box::new(DummyBackend {
            device_id: 2,
            features: 0,
            num_queues: 1,
        });
        let dev = VirtioMmioDevice::new(0xD000_0000, 5, backend);

        assert!(dev.handles(0xD000_0000));
        assert!(dev.handles(0xD000_0FFF));
        assert!(!dev.handles(0xD000_1000));
        assert!(!dev.handles(0xCFFF_FFFF));
    }

    #[test]
    fn virtqueue_new() {
        let vq = VirtQueue::new(256);
        assert_eq!(vq.max_size(), 256);
        assert_eq!(vq.size(), 0);
        assert!(!vq.is_ready());
    }

    #[test]
    fn virtqueue_set_size() {
        let mut vq = VirtQueue::new(256);
        vq.set_size(128);
        assert_eq!(vq.size(), 128);

        // Clamps to max_size
        vq.set_size(512);
        assert_eq!(vq.size(), 256);
    }

    #[test]
    fn virtqueue_activation() {
        let mut vq = VirtQueue::new(256);
        assert!(!vq.is_ready());
        vq.set_ready(true);
        assert!(vq.is_ready());
        vq.set_ready(false);
        assert!(!vq.is_ready());
    }
}
