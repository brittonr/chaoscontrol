//! Deterministic block device with copy-on-write snapshots and fault injection.
//!
//! Replaces a real `virtio-blk` backend with an in-memory store that supports
//! efficient snapshot/restore via copy-on-write (CoW) page tracking, and
//! programmable fault injection for testing storage error paths (torn writes,
//! corruption, I/O errors).
//!
//! # Copy-on-Write Design
//!
//! The block device maintains a shared, immutable **base image** (`Arc<Vec<u8>>`)
//! and a per-instance **dirty page map** (`BTreeMap<usize, Vec<u8>>`). Writes
//! copy the affected 4 KB page into the dirty map on first touch; subsequent
//! writes to the same page modify the dirty copy in place.
//!
//! Snapshots are cheap: clone the dirty map and bump the `Arc` reference count.
//! For a 512 MB disk image where only 1 MB has been modified, a snapshot costs
//! ~256 dirty-page clones (~1 MB) instead of a full 512 MB copy.

use snafu::Snafu;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

/// Dirty + volatile page overlays extracted for restart preservation.
pub type DirtyOverlay = (BTreeMap<usize, Vec<u8>>, BTreeMap<usize, Vec<u8>>);

/// Page size for copy-on-write tracking (4 KB, matching Linux page size).
const PAGE_SIZE: usize = 4096;

/// Errors returned by block device operations.
#[derive(Clone, Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum BlockError {
    /// The requested range falls outside the device.
    #[snafu(display("out of bounds: offset {offset}, len {len}, device size {device_size}"))]
    OutOfBounds {
        offset: u64,
        len: u64,
        device_size: u64,
    },

    /// An injected read error.
    #[snafu(display("injected read error at offset {offset}"))]
    InjectedReadError { offset: u64 },

    /// An injected write error.
    #[snafu(display("injected write error at offset {offset}"))]
    InjectedWriteError { offset: u64 },

    /// An injected torn write – only `bytes_written` of the payload landed.
    #[snafu(display(
        "injected torn write at offset {offset}: only {bytes_written} bytes written"
    ))]
    InjectedTornWrite { offset: u64, bytes_written: usize },

    /// Failed to read a disk image file.
    #[snafu(display("failed to read disk image '{path}': {reason}"))]
    ImageRead { path: String, reason: String },
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
    /// Return fewer bytes than requested on a read (short read).
    /// One-shot: consumed on the next matching read.
    PartialRead { offset: u64, max_bytes: usize },
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

/// Snapshot of a [`DeterministicBlock`], capturing CoW state, pending
/// faults, and statistics.
///
/// Cheap to create: the base image is shared via `Arc`, only dirty pages
/// and metadata are cloned.
#[derive(Clone, Debug)]
pub struct BlockSnapshot {
    base: Arc<Vec<u8>>,
    dirty: BTreeMap<usize, Vec<u8>>,
    volatile: BTreeMap<usize, Vec<u8>>,
    faults: VecDeque<BlockFault>,
    stats: BlockStats,
    slow_delay_ns: u64,
    fsync_lie: bool,
}

/// An in-memory block device with copy-on-write snapshots and deterministic
/// fault injection.
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
    /// Immutable base image, shared across snapshots.
    base: Arc<Vec<u8>>,
    /// Dirty pages: page index → page data (4 KB each, last page may be shorter).
    dirty: BTreeMap<usize, Vec<u8>>,
    /// Volatile pages (fsync-lie mode): discarded on crash, flushed explicitly.
    volatile: BTreeMap<usize, Vec<u8>>,
    /// Pending fault injection queue.
    faults: VecDeque<BlockFault>,
    /// I/O statistics.
    stats: BlockStats,
    /// Per-operation I/O delay in nanoseconds (0 = no delay).
    slow_delay_ns: u64,
    /// When true, writes go to the volatile buffer instead of durable storage.
    fsync_lie: bool,
}

impl DeterministicBlock {
    /// Create an empty (zero-filled) block device of `size_bytes`.
    pub fn new(size_bytes: usize) -> Self {
        Self {
            base: Arc::new(vec![0u8; size_bytes]),
            dirty: BTreeMap::new(),
            volatile: BTreeMap::new(),
            faults: VecDeque::new(),
            stats: BlockStats::default(),
            slow_delay_ns: 0,
            fsync_lie: false,
        }
    }

    /// Create a block device pre-loaded with `data` (e.g. a disk image).
    pub fn from_image(data: Vec<u8>) -> Self {
        Self {
            base: Arc::new(data),
            dirty: BTreeMap::new(),
            volatile: BTreeMap::new(),
            faults: VecDeque::new(),
            stats: BlockStats::default(),
            slow_delay_ns: 0,
            fsync_lie: false,
        }
    }

    /// Create a block device from a disk image file.
    ///
    /// Reads the entire file into memory as the base image. The file is
    /// only read once; subsequent snapshots share the data via `Arc`.
    pub fn from_image_file(path: &str) -> Result<Self, BlockError> {
        let data = std::fs::read(path).map_err(|e| BlockError::ImageRead {
            path: path.to_string(),
            reason: e.to_string(),
        })?;
        Ok(Self::from_image(data))
    }

    /// Size of the backing store in bytes.
    pub fn size(&self) -> u64 {
        self.base.len() as u64
    }

    /// Number of dirty (modified) pages.
    ///
    /// Useful for diagnostics: a snapshot's cost is proportional to this.
    pub fn dirty_page_count(&self) -> usize {
        self.dirty.len()
    }

    /// Approximate memory overhead from dirty pages (bytes).
    pub fn dirty_bytes(&self) -> usize {
        self.dirty.values().map(|p| p.len()).sum()
    }

    /// Read `buf.len()` bytes starting at `offset`.
    ///
    /// Returns the I/O delay in nanoseconds (0 unless `DiskSlow` is active).
    pub fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<u64, BlockError> {
        let len = buf.len() as u64;
        self.check_bounds(offset, len)?;

        // Check for an injected read fault at this offset.
        if let Some(idx) =
            self.find_fault(|f| matches!(f, BlockFault::ReadError { offset: o } if *o == offset))
        {
            let fault = self.faults.remove(idx).unwrap();
            if let BlockFault::ReadError { offset } = fault {
                return InjectedReadSnafu { offset }.fail();
            }
        }

        // Check for partial read fault.
        if let Some(idx) = self
            .find_fault(|f| matches!(f, BlockFault::PartialRead { offset: o, .. } if *o == offset))
        {
            let fault = self.faults.remove(idx).unwrap();
            if let BlockFault::PartialRead { max_bytes, .. } = fault {
                let actual = max_bytes.min(buf.len());
                // Read the partial portion.
                self.cow_read_3tier(offset as usize, &mut buf[..actual]);
                // Zero the rest.
                buf[actual..].fill(0);
                self.stats.reads += 1;
                self.stats.bytes_read += actual as u64;
                return Ok(self.slow_delay_ns);
            }
        }

        self.cow_read_3tier(offset as usize, buf);

        self.stats.reads += 1;
        self.stats.bytes_read += len;
        Ok(self.slow_delay_ns)
    }

    /// Write `data` starting at `offset`.
    ///
    /// Returns the I/O delay in nanoseconds (0 unless `DiskSlow` is active).
    pub fn write(&mut self, offset: u64, data: &[u8]) -> Result<u64, BlockError> {
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
                    return InjectedWriteSnafu { offset }.fail();
                }
                BlockFault::TornWrite {
                    offset,
                    bytes_written,
                } => {
                    let actual = bytes_written.min(data.len());
                    self.write_to_layer(offset as usize, &data[..actual]);
                    self.stats.writes += 1;
                    self.stats.bytes_written += actual as u64;
                    return InjectedTornWriteSnafu {
                        offset,
                        bytes_written: actual,
                    }
                    .fail();
                }
                BlockFault::Corruption {
                    offset: _,
                    len: corrupt_len,
                } => {
                    // Write the data normally, then overwrite with garbage.
                    self.write_to_layer(offset as usize, data);
                    let corrupt_end = corrupt_len.min(data.len());
                    let garbage = vec![0xFF; corrupt_end];
                    self.write_to_layer(offset as usize, &garbage);
                    self.stats.writes += 1;
                    self.stats.bytes_written += data.len() as u64;
                    return Ok(self.slow_delay_ns);
                }
                _ => unreachable!(),
            }
        }

        self.write_to_layer(offset as usize, data);

        self.stats.writes += 1;
        self.stats.bytes_written += len;
        Ok(self.slow_delay_ns)
    }

    /// Enqueue a fault to be triggered on a future I/O operation.
    ///
    /// Faults are matched and consumed in FIFO order.
    pub fn inject_fault(&mut self, fault: BlockFault) {
        self.faults.push_back(fault);
    }

    /// Set the per-I/O delay (nanoseconds). 0 = no delay.
    pub fn set_slow_delay_ns(&mut self, delay_ns: u64) {
        self.slow_delay_ns = delay_ns;
    }

    /// Current slow delay setting.
    pub fn slow_delay_ns(&self) -> u64 {
        self.slow_delay_ns
    }

    /// Enable fsync-lie mode: subsequent writes go to a volatile buffer.
    pub fn enable_fsync_lie(&mut self) {
        self.fsync_lie = true;
    }

    /// Disable fsync-lie mode. Does NOT flush pending volatile writes.
    pub fn disable_fsync_lie(&mut self) {
        self.fsync_lie = false;
    }

    /// Whether fsync-lie mode is currently active.
    pub fn fsync_lie_active(&self) -> bool {
        self.fsync_lie
    }

    /// Flush (commit) the volatile buffer into the durable dirty layer.
    /// All volatile pages become durable.
    pub fn flush_volatile(&mut self) {
        for (page_idx, page_data) in std::mem::take(&mut self.volatile) {
            self.dirty.insert(page_idx, page_data);
        }
    }

    /// Discard all volatile writes (simulates power loss).
    pub fn discard_volatile(&mut self) {
        self.volatile.clear();
    }

    /// Number of volatile (unflushed) pages.
    pub fn volatile_page_count(&self) -> usize {
        self.volatile.len()
    }

    /// Capture a snapshot of the device (CoW state + faults + stats).
    ///
    /// Cost is proportional to the number of dirty pages, not the device
    /// size. The base image is shared via `Arc` reference counting.
    pub fn snapshot(&self) -> BlockSnapshot {
        BlockSnapshot {
            base: Arc::clone(&self.base),
            dirty: self.dirty.clone(),
            volatile: self.volatile.clone(),
            faults: self.faults.clone(),
            stats: self.stats.clone(),
            slow_delay_ns: self.slow_delay_ns,
            fsync_lie: self.fsync_lie,
        }
    }

    /// Restore a device from a snapshot.
    ///
    /// Shares the base image with the snapshot via `Arc`.
    pub fn restore(snapshot: &BlockSnapshot) -> Self {
        Self {
            base: Arc::clone(&snapshot.base),
            dirty: snapshot.dirty.clone(),
            volatile: snapshot.volatile.clone(),
            faults: snapshot.faults.clone(),
            stats: snapshot.stats.clone(),
            slow_delay_ns: snapshot.slow_delay_ns,
            fsync_lie: snapshot.fsync_lie,
        }
    }

    /// Extract dirty + volatile page overlays for preservation across restart.
    ///
    /// Returns the dirty and volatile maps without consuming the block device.
    /// Used by the controller to preserve disk state across ProcessRestart.
    pub fn snapshot_dirty(&self) -> DirtyOverlay {
        (self.dirty.clone(), self.volatile.clone())
    }

    /// Apply dirty + volatile page overlays (from a prior `snapshot_dirty`).
    ///
    /// Merges the provided pages into the device's current state. Pages
    /// in `dirty` overwrite any existing dirty pages at the same index.
    pub fn restore_dirty(&mut self, overlay: DirtyOverlay) {
        self.dirty = overlay.0;
        self.volatile = overlay.1;
    }

    /// Current I/O statistics.
    pub fn stats(&self) -> &BlockStats {
        &self.stats
    }

    /// Flatten CoW layers into a contiguous byte vector.
    ///
    /// Useful for inspection, debugging, or writing the final disk state
    /// to a file. Returns a full copy of the device contents.
    pub fn materialize(&self) -> Vec<u8> {
        let mut data = (*self.base).clone();
        for (&page_idx, page_data) in &self.dirty {
            let start = page_idx * PAGE_SIZE;
            let end = start + page_data.len();
            data[start..end].copy_from_slice(page_data);
        }
        data
    }

    // ── CoW internals ─────────────────────────────────────────────

    /// Write to the appropriate layer (volatile if fsync_lie, else dirty).
    fn write_to_layer(&mut self, offset: usize, data: &[u8]) {
        if self.fsync_lie {
            self.volatile_write(offset, data);
        } else {
            self.cow_write(offset, data);
        }
    }

    /// Write bytes into the volatile layer (same page-splitting as cow_write).
    fn volatile_write(&mut self, offset: usize, data: &[u8]) {
        let mut pos = 0;
        while pos < data.len() {
            let abs = offset + pos;
            let page_idx = abs / PAGE_SIZE;
            let in_page = abs % PAGE_SIZE;
            let page_remaining = self.page_len(page_idx) - in_page;
            let chunk = page_remaining.min(data.len() - pos);

            let page = self.volatile.entry(page_idx).or_insert_with(|| {
                // Seed from dirty, then base.
                if let Some(dirty_page) = self.dirty.get(&page_idx) {
                    dirty_page.clone()
                } else {
                    let start = page_idx * PAGE_SIZE;
                    let end = (start + PAGE_SIZE).min(self.base.len());
                    self.base[start..end].to_vec()
                }
            });
            page[in_page..in_page + chunk].copy_from_slice(&data[pos..pos + chunk]);

            pos += chunk;
        }
    }

    /// 3-tier read: volatile → dirty → base.
    fn cow_read_3tier(&self, offset: usize, buf: &mut [u8]) {
        let mut pos = 0;
        while pos < buf.len() {
            let abs = offset + pos;
            let page_idx = abs / PAGE_SIZE;
            let in_page = abs % PAGE_SIZE;
            let page_remaining = self.page_len(page_idx) - in_page;
            let chunk = page_remaining.min(buf.len() - pos);

            if let Some(volatile_page) = self.volatile.get(&page_idx) {
                buf[pos..pos + chunk].copy_from_slice(&volatile_page[in_page..in_page + chunk]);
            } else if let Some(dirty_page) = self.dirty.get(&page_idx) {
                buf[pos..pos + chunk].copy_from_slice(&dirty_page[in_page..in_page + chunk]);
            } else {
                let base_off = page_idx * PAGE_SIZE + in_page;
                buf[pos..pos + chunk].copy_from_slice(&self.base[base_off..base_off + chunk]);
            }

            pos += chunk;
        }
    }

    /// Write bytes through the CoW layer.
    ///
    /// For each 4 KB page touched, ensures a dirty copy exists (copying
    /// from the base image on first write), then modifies the dirty copy.
    fn cow_write(&mut self, offset: usize, data: &[u8]) {
        let mut pos = 0;
        while pos < data.len() {
            let abs = offset + pos;
            let page_idx = abs / PAGE_SIZE;
            let in_page = abs % PAGE_SIZE;
            let page_remaining = self.page_len(page_idx) - in_page;
            let chunk = page_remaining.min(data.len() - pos);

            let dirty_page = self.ensure_dirty(page_idx);
            dirty_page[in_page..in_page + chunk].copy_from_slice(&data[pos..pos + chunk]);

            pos += chunk;
        }
    }

    /// Ensure a dirty copy of the given page exists, creating one from
    /// the base image if needed. Returns a mutable reference to the page.
    fn ensure_dirty(&mut self, page_idx: usize) -> &mut Vec<u8> {
        self.dirty.entry(page_idx).or_insert_with(|| {
            let start = page_idx * PAGE_SIZE;
            let end = (start + PAGE_SIZE).min(self.base.len());
            self.base[start..end].to_vec()
        })
    }

    /// Length of the given page (4096 for all but possibly the last page).
    fn page_len(&self, page_idx: usize) -> usize {
        let start = page_idx * PAGE_SIZE;
        (start + PAGE_SIZE).min(self.base.len()) - start
    }

    // ── bounds / fault helpers ────────────────────────────────────

    /// Check whether `[offset, offset+len)` is within the device.
    fn check_bounds(&self, offset: u64, len: u64) -> Result<(), BlockError> {
        crate::verified::block::check_bounds(self.base.len() as u64, offset, len)
    }

    /// Find the first fault in the queue matching `predicate`.
    fn find_fault(&self, predicate: impl Fn(&BlockFault) -> bool) -> Option<usize> {
        crate::verified::block::find_matching_fault(&self.faults, predicate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── basic read/write ─────────────────────────────────────────

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

    // ── fault injection ──────────────────────────────────────────

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

    // ── stats ────────────────────────────────────────────────────

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

    // ── snapshot / restore ───────────────────────────────────────

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

    // ── CoW-specific tests ───────────────────────────────────────

    #[test]
    fn cow_no_dirty_pages_on_read_only() {
        let image = vec![0xAB; 8192];
        let mut blk = DeterministicBlock::from_image(image);

        let mut buf = [0u8; 512];
        blk.read(0, &mut buf).unwrap();
        blk.read(4096, &mut buf).unwrap();

        assert_eq!(
            blk.dirty_page_count(),
            0,
            "reads must not create dirty pages"
        );
    }

    #[test]
    fn cow_single_page_write() {
        let mut blk = DeterministicBlock::new(8192);
        blk.write(0, &[0xAA; 100]).unwrap();

        assert_eq!(
            blk.dirty_page_count(),
            1,
            "write within one page = 1 dirty page"
        );
    }

    #[test]
    fn cow_cross_page_write() {
        let mut blk = DeterministicBlock::new(8192);
        // Write spanning page boundary (page 0 and page 1)
        blk.write(4090, &[0xBB; 20]).unwrap();

        assert_eq!(
            blk.dirty_page_count(),
            2,
            "cross-page write = 2 dirty pages"
        );

        let mut buf = [0u8; 20];
        blk.read(4090, &mut buf).unwrap();
        assert_eq!(buf, [0xBB; 20]);
    }

    #[test]
    fn cow_snapshot_shares_base() {
        let image = vec![0xCC; 16384]; // 4 pages
        let mut blk = DeterministicBlock::from_image(image);

        // Dirty only page 0
        blk.write(0, &[0xDD; 512]).unwrap();
        assert_eq!(blk.dirty_page_count(), 1);

        let snap = blk.snapshot();

        // After snapshot, original and snapshot share the same base Arc
        assert!(Arc::ptr_eq(&blk.base, &snap.base));

        // Snapshot has the same dirty page count
        let restored = DeterministicBlock::restore(&snap);
        assert_eq!(restored.dirty_page_count(), 1);
    }

    #[test]
    fn cow_snapshot_isolation() {
        let mut blk = DeterministicBlock::new(8192);
        blk.write(0, b"before snap").unwrap();

        let snap = blk.snapshot();

        // Mutate original (different page)
        blk.write(4096, b"after snap").unwrap();

        // Restore from snapshot — should not see the mutation
        let mut restored = DeterministicBlock::restore(&snap);
        let mut buf = [0u8; 10];
        restored.read(4096, &mut buf).unwrap();
        assert_eq!(
            buf, [0u8; 10],
            "restored device must not see post-snapshot writes"
        );
    }

    #[test]
    fn cow_materialize() {
        let mut blk = DeterministicBlock::new(8192);
        blk.write(0, &[0xAA; 100]).unwrap();
        blk.write(4096, &[0xBB; 200]).unwrap();

        let flat = blk.materialize();
        assert_eq!(flat.len(), 8192);
        assert_eq!(&flat[0..100], &[0xAA; 100]);
        assert_eq!(&flat[100..4096], &vec![0u8; 3996]);
        assert_eq!(&flat[4096..4296], &[0xBB; 200]);
    }

    #[test]
    fn cow_multiple_snapshots_share_base() {
        let image = vec![0x11; 65536]; // 16 pages
        let mut blk = DeterministicBlock::from_image(image);

        // Write different pages
        blk.write(0, &[0x22; 512]).unwrap(); // page 0
        let snap1 = blk.snapshot();

        blk.write(4096, &[0x33; 512]).unwrap(); // page 1
        let snap2 = blk.snapshot();

        blk.write(8192, &[0x44; 512]).unwrap(); // page 2
        let snap3 = blk.snapshot();

        // All snapshots share the same base
        assert!(Arc::ptr_eq(&snap1.base, &snap2.base));
        assert!(Arc::ptr_eq(&snap2.base, &snap3.base));

        // But have different dirty page counts
        let r1 = DeterministicBlock::restore(&snap1);
        let r2 = DeterministicBlock::restore(&snap2);
        let r3 = DeterministicBlock::restore(&snap3);
        assert_eq!(r1.dirty_page_count(), 1);
        assert_eq!(r2.dirty_page_count(), 2);
        assert_eq!(r3.dirty_page_count(), 3);
    }

    #[test]
    fn cow_dirty_bytes() {
        let mut blk = DeterministicBlock::new(8192);
        assert_eq!(blk.dirty_bytes(), 0);

        blk.write(0, &[1u8; 10]).unwrap(); // dirties page 0 (4096 bytes)
        assert_eq!(blk.dirty_bytes(), 4096);

        blk.write(4096, &[2u8; 10]).unwrap(); // dirties page 1 (4096 bytes)
        assert_eq!(blk.dirty_bytes(), 8192);
    }

    #[test]
    fn cow_last_page_partial() {
        // Device size not a multiple of PAGE_SIZE
        let mut blk = DeterministicBlock::new(5000);
        assert_eq!(blk.size(), 5000);

        // Write to the last partial page (page 1: bytes 4096..5000 = 904 bytes)
        blk.write(4096, &[0xEE; 904]).unwrap();
        assert_eq!(blk.dirty_page_count(), 1);

        let mut buf = [0u8; 904];
        blk.read(4096, &mut buf).unwrap();
        assert_eq!(buf, [0xEE; 904]);
    }

    #[test]
    fn cow_from_image_file_not_found() {
        let err = DeterministicBlock::from_image_file("/nonexistent/disk.img").unwrap_err();
        assert!(matches!(err, BlockError::ImageRead { .. }));
    }

    #[test]
    fn cow_from_image_file_roundtrip() {
        // Create a temp file with known content
        let dir = std::env::temp_dir().join("chaoscontrol-block-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.img");
        let data = vec![0x42; 8192];
        std::fs::write(&path, &data).unwrap();

        let mut blk = DeterministicBlock::from_image_file(path.to_str().unwrap()).unwrap();
        let mut buf = [0u8; 8192];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(buf.to_vec(), data);
        assert_eq!(blk.dirty_page_count(), 0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cow_write_then_snapshot_then_diverge() {
        // Simulates exploration: base → write → snapshot → two divergent branches
        let mut blk = DeterministicBlock::from_image(vec![0u8; 16384]);

        // Common prefix
        blk.write(0, b"shared state").unwrap();
        let snap = blk.snapshot();

        // Branch A: writes to page 1
        let mut branch_a = DeterministicBlock::restore(&snap);
        branch_a.write(4096, b"branch A data").unwrap();

        // Branch B: writes to page 2
        let mut branch_b = DeterministicBlock::restore(&snap);
        branch_b.write(8192, b"branch B data").unwrap();

        // Verify isolation
        let mut buf_a = [0u8; 13];
        branch_a.read(4096, &mut buf_a).unwrap();
        assert_eq!(&buf_a, b"branch A data");

        let mut buf_b = [0u8; 13];
        branch_b.read(8192, &mut buf_b).unwrap();
        assert_eq!(&buf_b, b"branch B data");

        // Branch A should NOT see branch B's write
        let mut check = [0u8; 13];
        branch_a.read(8192, &mut check).unwrap();
        assert_eq!(check, [0u8; 13]);

        // Branch B should NOT see branch A's write
        branch_b.read(4096, &mut check).unwrap();
        assert_eq!(check, [0u8; 13]);

        // Both share the same base
        assert!(Arc::ptr_eq(&branch_a.base, &branch_b.base));
    }

    #[test]
    fn cow_fault_injection_through_cow() {
        // Verify fault injection works correctly with CoW layer
        let mut blk = DeterministicBlock::from_image(vec![0u8; 8192]);

        // Write, snapshot, inject fault, restore
        blk.write(0, &[0xAA; 512]).unwrap();
        blk.inject_fault(BlockFault::TornWrite {
            offset: 0,
            bytes_written: 4,
        });
        let snap = blk.snapshot();

        let mut restored = DeterministicBlock::restore(&snap);
        let err = restored.write(0, &[0xBB; 512]).unwrap_err();
        assert!(matches!(err, BlockError::InjectedTornWrite { .. }));

        // Only 4 bytes should have been written
        let mut buf = [0u8; 512];
        restored.read(0, &mut buf).unwrap();
        assert_eq!(&buf[..4], &[0xBB; 4]);
        assert_eq!(&buf[4..512], &[0xAA; 508]);
    }

    // ── DiskSlow tests ──────────────────────────────────────────────

    #[test]
    fn slow_read_returns_delay() {
        let mut blk = DeterministicBlock::new(4096);
        blk.set_slow_delay_ns(5_000_000);
        let delay = blk.read(0, &mut [0u8; 8]).unwrap();
        assert_eq!(delay, 5_000_000);
    }

    #[test]
    fn slow_write_returns_delay() {
        let mut blk = DeterministicBlock::new(4096);
        blk.set_slow_delay_ns(10_000_000);
        let delay = blk.write(0, &[1u8; 8]).unwrap();
        assert_eq!(delay, 10_000_000);
    }

    #[test]
    fn slow_clear_removes_delay() {
        let mut blk = DeterministicBlock::new(4096);
        blk.set_slow_delay_ns(5_000_000);
        assert_eq!(blk.slow_delay_ns(), 5_000_000);
        blk.set_slow_delay_ns(0);
        let delay = blk.read(0, &mut [0u8; 8]).unwrap();
        assert_eq!(delay, 0);
    }

    #[test]
    fn slow_snapshot_roundtrip() {
        let mut blk = DeterministicBlock::new(4096);
        blk.set_slow_delay_ns(42_000);
        let snap = blk.snapshot();
        let restored = DeterministicBlock::restore(&snap);
        assert_eq!(restored.slow_delay_ns(), 42_000);
    }

    // ── DiskFsyncLie tests ──────────────────────────────────────────

    #[test]
    fn fsync_lie_writes_to_volatile() {
        let mut blk = DeterministicBlock::new(8192);
        blk.enable_fsync_lie();
        blk.write(0, &[0xAA; 512]).unwrap();

        // Data visible via read (volatile is read-through)
        let mut buf = [0u8; 512];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(buf, [0xAA; 512]);

        // But NOT in the dirty layer
        assert_eq!(blk.dirty_page_count(), 0);
        assert_eq!(blk.volatile_page_count(), 1);
    }

    #[test]
    fn fsync_lie_discard_on_kill() {
        let mut blk = DeterministicBlock::new(8192);
        blk.enable_fsync_lie();
        blk.write(0, &[0xBB; 512]).unwrap();
        assert_eq!(blk.volatile_page_count(), 1);

        // Simulate ProcessKill
        blk.discard_volatile();
        assert_eq!(blk.volatile_page_count(), 0);

        // Data gone
        let mut buf = [0u8; 512];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(buf, [0u8; 512]);
    }

    #[test]
    fn fsync_lie_flush_commits() {
        let mut blk = DeterministicBlock::new(8192);
        blk.enable_fsync_lie();
        blk.write(0, &[0xCC; 512]).unwrap();
        assert_eq!(blk.volatile_page_count(), 1);
        assert_eq!(blk.dirty_page_count(), 0);

        blk.flush_volatile();
        assert_eq!(blk.volatile_page_count(), 0);
        assert_eq!(blk.dirty_page_count(), 1);

        // Data survives discard after flush
        blk.discard_volatile();
        let mut buf = [0u8; 512];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(buf, [0xCC; 512]);
    }

    #[test]
    fn fsync_lie_read_through_layers() {
        let mut blk = DeterministicBlock::from_image(vec![0x11; 8192]);
        // Write to dirty layer
        blk.write(0, &[0x22; 256]).unwrap();
        // Enable fsync-lie, write to volatile
        blk.enable_fsync_lie();
        blk.write(0, &[0x33; 128]).unwrap();

        let mut buf = [0u8; 512];
        blk.read(0, &mut buf).unwrap();
        // First 128 bytes from volatile
        assert_eq!(&buf[..128], &[0x33; 128]);
        // Bytes 128..256 from dirty (volatile page has dirty seed)
        // Actually, volatile page was seeded from dirty, so 128..256 = 0x22
        assert_eq!(&buf[128..256], &[0x22; 128]);
    }

    #[test]
    fn fsync_lie_snapshot_roundtrip() {
        let mut blk = DeterministicBlock::new(8192);
        blk.enable_fsync_lie();
        blk.write(0, &[0xDD; 100]).unwrap();
        let snap = blk.snapshot();

        let mut restored = DeterministicBlock::restore(&snap);
        assert!(restored.fsync_lie_active());
        assert_eq!(restored.volatile_page_count(), 1);

        let mut buf = [0u8; 100];
        restored.read(0, &mut buf).unwrap();
        assert_eq!(buf, [0xDD; 100]);
    }

    // ── DiskPartialRead tests ───────────────────────────────────────

    #[test]
    fn partial_read_short() {
        let mut blk = DeterministicBlock::from_image(vec![0xFF; 4096]);
        blk.inject_fault(BlockFault::PartialRead {
            offset: 0,
            max_bytes: 256,
        });

        let mut buf = [0u8; 512];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(&buf[..256], &[0xFF; 256]);
        assert_eq!(&buf[256..], &[0u8; 256]);
    }

    #[test]
    fn partial_read_one_shot() {
        let mut blk = DeterministicBlock::from_image(vec![0xFF; 4096]);
        blk.inject_fault(BlockFault::PartialRead {
            offset: 0,
            max_bytes: 128,
        });

        // First read: partial
        let mut buf = [0u8; 512];
        blk.read(0, &mut buf).unwrap();
        assert_eq!(&buf[128..], &[0u8; 384]);

        // Second read: full (fault consumed)
        let mut buf2 = [0u8; 512];
        blk.read(0, &mut buf2).unwrap();
        assert_eq!(buf2, [0xFF; 512]);
    }

    // ── dirty page preservation across restart ──────────────────

    #[test]
    fn snapshot_dirty_preserves_writes() {
        let base = vec![0u8; 8192];
        let mut blk = DeterministicBlock::from_image(base);

        // Write data to dirty pages
        blk.write(0, b"persistent").unwrap();
        blk.write(4096, b"also persistent").unwrap();

        // Snapshot just the dirty overlay
        let (dirty, volatile) = blk.snapshot_dirty();
        assert_eq!(dirty.len(), 2); // two dirty pages

        // Create a fresh block device (simulating restart)
        let mut fresh = DeterministicBlock::from_image(vec![0u8; 8192]);

        // Restore dirty pages
        fresh.restore_dirty((dirty, volatile));

        // Verify data survived
        let mut buf = vec![0u8; 10];
        fresh.read(0, &mut buf).unwrap();
        assert_eq!(&buf, b"persistent");

        let mut buf2 = vec![0u8; 15];
        fresh.read(4096, &mut buf2).unwrap();
        assert_eq!(&buf2, b"also persistent");
    }

    #[test]
    fn snapshot_dirty_empty_on_clean_device() {
        let blk = DeterministicBlock::new(4096);
        let (dirty, volatile) = blk.snapshot_dirty();
        assert!(dirty.is_empty());
        assert!(volatile.is_empty());
    }
}
