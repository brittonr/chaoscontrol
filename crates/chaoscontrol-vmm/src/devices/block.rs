//! Deterministic in-memory block device with fault injection.
//!
//! Replaces a real `virtio-blk` backend with a simple byte-vector store that
//! supports snapshot/restore and programmable fault injection for testing
//! storage error paths (torn writes, corruption, I/O errors).

use std::collections::VecDeque;

/// Errors returned by block device operations.
#[derive(Clone, Debug, thiserror::Error)]
pub enum BlockError {
    /// The requested range falls outside the device.
    #[error("out of bounds: offset {offset}, len {len}, device size {device_size}")]
    OutOfBounds {
        offset: u64,
        len: u64,
        device_size: u64,
    },

    /// An injected read error.
    #[error("injected read error at offset {offset}")]
    InjectedReadError { offset: u64 },

    /// An injected write error.
    #[error("injected write error at offset {offset}")]
    InjectedWriteError { offset: u64 },

    /// An injected torn write – only `bytes_written` of the payload landed.
    #[error("injected torn write at offset {offset}: only {bytes_written} bytes written")]
    InjectedTornWrite { offset: u64, bytes_written: usize },
}

/// A fault that can be injected into the block device.
///
/// Faults are consumed in FIFO order: the next matching I/O operation
/// triggers the oldest queued fault whose variant applies (read vs write)
/// and whose `offset` matches the request offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockFault {
    /// Fail the next read that touches `offset`.
    ReadError { offset: u64 },
    /// Fail the next write that touches `offset`.
    WriteError { offset: u64 },
    /// Simulate a torn write: only `bytes_written` bytes are persisted.
    TornWrite { offset: u64, bytes_written: usize },
    /// Silently corrupt `len` bytes starting at `offset` (writes garbage).
    Corruption { offset: u64, len: usize },
}

/// Read/write statistics for a [`DeterministicBlock`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockStats {
    /// Number of successful read operations.
    pub reads: u64,
    /// Number of successful write operations (including torn writes).
    pub writes: u64,
    /// Total bytes read.
    pub bytes_read: u64,
    /// Total bytes written (for torn writes, only the partial amount).
    pub bytes_written: u64,
}

/// Snapshot of a [`DeterministicBlock`], capturing the full backing store,
/// pending faults, and statistics.
#[derive(Clone, Debug)]
pub struct BlockSnapshot {
    data: Vec<u8>,
    faults: VecDeque<BlockFault>,
    stats: BlockStats,
}

/// An in-memory block device with deterministic fault injection.
///
/// # Examples
///
/// ```
/// use chaoscontrol_vmm::devices::block::DeterministicBlock;
///
/// let mut blk = DeterministicBlock::new(4096);
/// blk.write(0, &[0xAA; 512]).unwrap();
///
/// let mut buf = [0u8; 512];
/// blk.read(0, &mut buf).unwrap();
/// assert_eq!(buf, [0xAA; 512]);
/// ```
#[derive(Clone, Debug)]
pub struct DeterministicBlock {
    data: Vec<u8>,
    faults: VecDeque<BlockFault>,
    stats: BlockStats,
}

impl DeterministicBlock {
    /// Create an empty (zero-filled) block device of `size_bytes`.
    pub fn new(size_bytes: usize) -> Self {
        Self {
            data: vec![0u8; size_bytes],
            faults: VecDeque::new(),
            stats: BlockStats::default(),
        }
    }

    /// Create a block device pre-loaded with `data` (e.g. a disk image).
    pub fn from_image(data: Vec<u8>) -> Self {
        Self {
            data,
            faults: VecDeque::new(),
            stats: BlockStats::default(),
        }
    }

    /// Size of the backing store in bytes.
    pub fn size(&self) -> u64 {
        self.data.len() as u64
    }

    /// Read `buf.len()` bytes starting at `offset`.
    pub fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        let len = buf.len() as u64;
        self.check_bounds(offset, len)?;

        // Check for an injected read fault at this offset.
        if let Some(idx) =
            self.find_fault(|f| matches!(f, BlockFault::ReadError { offset: o } if *o == offset))
        {
            let fault = self.faults.remove(idx).unwrap();
            if let BlockFault::ReadError { offset } = fault {
                return Err(BlockError::InjectedReadError { offset });
            }
        }

        let start = offset as usize;
        let end = start + buf.len();
        buf.copy_from_slice(&self.data[start..end]);

        self.stats.reads += 1;
        self.stats.bytes_read += len;
        Ok(())
    }

    /// Write `data` starting at `offset`.
    pub fn write(&mut self, offset: u64, data: &[u8]) -> Result<(), BlockError> {
        let len = data.len() as u64;
        self.check_bounds(offset, len)?;

        // Check for an injected write fault at this offset.
        if let Some(idx) = self.find_fault(|f| {
            matches!(
                f,
                BlockFault::WriteError { offset: o }
                | BlockFault::TornWrite { offset: o, .. }
                | BlockFault::Corruption { offset: o, .. }
                if *o == offset
            )
        }) {
            let fault = self.faults.remove(idx).unwrap();
            match fault {
                BlockFault::WriteError { offset } => {
                    return Err(BlockError::InjectedWriteError { offset });
                }
                BlockFault::TornWrite {
                    offset,
                    bytes_written,
                } => {
                    // Partial write: only persist the first `bytes_written` bytes.
                    let actual = bytes_written.min(data.len());
                    let start = offset as usize;
                    self.data[start..start + actual].copy_from_slice(&data[..actual]);
                    self.stats.writes += 1;
                    self.stats.bytes_written += actual as u64;
                    return Err(BlockError::InjectedTornWrite {
                        offset,
                        bytes_written: actual,
                    });
                }
                BlockFault::Corruption {
                    offset: _,
                    len: corrupt_len,
                } => {
                    // Write the data normally, then overwrite with garbage.
                    let start = offset as usize;
                    let end = start + data.len();
                    self.data[start..end].copy_from_slice(data);

                    // Corrupt: fill the first `corrupt_len` bytes with 0xFF.
                    let corrupt_end = (start + corrupt_len).min(self.data.len());
                    for byte in &mut self.data[start..corrupt_end] {
                        *byte = 0xFF;
                    }
                    self.stats.writes += 1;
                    self.stats.bytes_written += data.len() as u64;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let start = offset as usize;
        let end = start + data.len();
        self.data[start..end].copy_from_slice(data);

        self.stats.writes += 1;
        self.stats.bytes_written += len;
        Ok(())
    }

    /// Enqueue a fault to be triggered on a future I/O operation.
    ///
    /// Faults are matched and consumed in FIFO order.
    pub fn inject_fault(&mut self, fault: BlockFault) {
        self.faults.push_back(fault);
    }

    /// Capture a full snapshot of the device (backing data + faults + stats).
    pub fn snapshot(&self) -> BlockSnapshot {
        BlockSnapshot {
            data: self.data.clone(),
            faults: self.faults.clone(),
            stats: self.stats.clone(),
        }
    }

    /// Restore a device from a snapshot.
    pub fn restore(snapshot: &BlockSnapshot) -> Self {
        Self {
            data: snapshot.data.clone(),
            faults: snapshot.faults.clone(),
            stats: snapshot.stats.clone(),
        }
    }

    /// Current I/O statistics.
    pub fn stats(&self) -> &BlockStats {
        &self.stats
    }

    // ── internal helpers ──────────────────────────────────────────────

    /// Check whether `[offset, offset+len)` is within the device.
    ///
    /// Delegates to [`crate::verified::block::check_bounds`].
    fn check_bounds(&self, offset: u64, len: u64) -> Result<(), BlockError> {
        crate::verified::block::check_bounds(self.data.len() as u64, offset, len)
    }

    /// Find the first fault in the queue matching `predicate` and return its
    /// index.
    ///
    /// Delegates to [`crate::verified::block::find_matching_fault`].
    fn find_fault(&self, predicate: impl Fn(&BlockFault) -> bool) -> Option<usize> {
        crate::verified::block::find_matching_fault(&self.faults, predicate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_roundtrip() {
        let mut blk = DeterministicBlock::new(1024);
        let payload = b"hello block device";
        blk.write(0, payload).unwrap();

        let mut buf = vec![0u8; payload.len()];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(&buf, payload);
    }

    #[test]
    fn from_image() {
        let image = vec![0xAB; 512];
        let mut blk = DeterministicBlock::from_image(image.clone());
        let mut buf = vec![0u8; 512];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(buf, image);
    }

    #[test]
    fn out_of_bounds_read() {
        let mut blk = DeterministicBlock::new(512);
        let mut buf = [0u8; 8];
        let err = blk.read(510, &mut buf).unwrap_err();
        assert!(matches!(err, BlockError::OutOfBounds { .. }));
    }

    #[test]
    fn out_of_bounds_write() {
        let mut blk = DeterministicBlock::new(512);
        let err = blk.write(510, &[0u8; 8]).unwrap_err();
        assert!(matches!(err, BlockError::OutOfBounds { .. }));
    }

    #[test]
    fn injected_read_error() {
        let mut blk = DeterministicBlock::new(1024);
        blk.inject_fault(BlockFault::ReadError { offset: 0 });

        let mut buf = [0u8; 8];
        let err = blk.read(0, &mut buf).unwrap_err();
        assert!(matches!(err, BlockError::InjectedReadError { offset: 0 }));

        // Fault consumed – second read succeeds.
        blk.read(0, &mut buf).unwrap();
    }

    #[test]
    fn injected_write_error() {
        let mut blk = DeterministicBlock::new(1024);
        blk.inject_fault(BlockFault::WriteError { offset: 0 });

        let err = blk.write(0, &[1u8; 8]).unwrap_err();
        assert!(matches!(err, BlockError::InjectedWriteError { offset: 0 }));

        // Data should be unchanged (write never landed).
        let mut buf = [0u8; 8];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(buf, [0u8; 8]);
    }

    #[test]
    fn injected_torn_write() {
        let mut blk = DeterministicBlock::new(1024);
        blk.inject_fault(BlockFault::TornWrite {
            offset: 0,
            bytes_written: 4,
        });

        let err = blk.write(0, &[0xAA; 8]).unwrap_err();
        assert!(matches!(
            err,
            BlockError::InjectedTornWrite {
                offset: 0,
                bytes_written: 4
            }
        ));

        // Only the first 4 bytes should have been written.
        let mut buf = [0u8; 8];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(&buf[..4], &[0xAA; 4]);
        assert_eq!(&buf[4..], &[0x00; 4]);
    }

    #[test]
    fn injected_corruption() {
        let mut blk = DeterministicBlock::new(1024);
        blk.inject_fault(BlockFault::Corruption { offset: 0, len: 4 });

        // Write succeeds (returns Ok), but the first 4 bytes are corrupted.
        blk.write(0, &[0xAA; 8]).unwrap();

        let mut buf = [0u8; 8];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(&buf[..4], &[0xFF; 4]); // corrupted
        assert_eq!(&buf[4..], &[0xAA; 4]); // rest intact
    }

    #[test]
    fn fault_offset_mismatch_no_trigger() {
        let mut blk = DeterministicBlock::new(1024);
        blk.inject_fault(BlockFault::ReadError { offset: 512 });

        // Read at offset 0 should succeed because the fault targets offset 512.
        let mut buf = [0u8; 8];
        blk.read(0, &mut buf).unwrap();
    }

    #[test]
    fn stats_tracking() {
        let mut blk = DeterministicBlock::new(1024);
        blk.write(0, &[1u8; 100]).unwrap();
        blk.write(100, &[2u8; 50]).unwrap();

        let mut buf = [0u8; 64];
        blk.read(0, &mut buf).unwrap();

        assert_eq!(blk.stats().writes, 2);
        assert_eq!(blk.stats().bytes_written, 150);
        assert_eq!(blk.stats().reads, 1);
        assert_eq!(blk.stats().bytes_read, 64);
    }

    #[test]
    fn snapshot_restore() {
        let mut blk = DeterministicBlock::new(1024);
        blk.write(0, b"snapshot me").unwrap();
        blk.inject_fault(BlockFault::ReadError { offset: 512 });

        let snap = blk.snapshot();

        // Mutate original
        blk.write(0, b"overwritten").unwrap();

        // Restore
        let mut restored = DeterministicBlock::restore(&snap);
        let mut buf = vec![0u8; 11];
        restored.read(0, &mut buf).unwrap();
        assert_eq!(&buf, b"snapshot me");

        // Fault should still be present
        let mut fault_buf = [0u8; 8];
        let err = restored.read(512, &mut fault_buf).unwrap_err();
        assert!(matches!(err, BlockError::InjectedReadError { offset: 512 }));

        // Stats preserved
        assert_eq!(restored.stats().writes, snap.stats.writes);
    }

    #[test]
    fn size_method() {
        let blk = DeterministicBlock::new(4096);
        assert_eq!(blk.size(), 4096);

        let blk2 = DeterministicBlock::from_image(vec![0; 8192]);
        assert_eq!(blk2.size(), 8192);
    }
}
