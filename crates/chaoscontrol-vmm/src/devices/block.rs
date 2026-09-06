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

/// Dirty + volatile page overlays extracted for restart preservation.
pub type DirtyOverlay = (
    std::collections::BTreeMap<usize, Vec<u8>>,
    std::collections::BTreeMap<usize, Vec<u8>>,
);

/// Page size for copy-on-write tracking (4 KB, matching Linux page size).
const PAGE_SIZE: usize = 4096;
const MAX_PENDING_BLOCK_OBSERVATIONS: usize = 4_096;

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

    /// The evidence queue cannot retain all attributable effects.
    #[snafu(display(
        "block observation capacity exhausted: required {required}, available {available}"
    ))]
    ObservationCapacity { required: usize, available: usize },

    /// The deterministic block operation sequence is exhausted.
    #[snafu(display("block operation sequence exhausted"))]
    OperationSequenceExhausted,
}

/// A fault that can be injected into the block device.
///
/// Faults are consumed in FIFO order: the next matching I/O operation
/// triggers the oldest queued fault whose variant applies (read vs write)
/// and whose `offset` matches the request offset.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BlockSnapshot {
    base: std::sync::Arc<Vec<u8>>,
    dirty: std::collections::BTreeMap<usize, Vec<u8>>,
    volatile: std::collections::BTreeMap<usize, Vec<u8>>,
    faults: std::collections::VecDeque<BlockFault>,
    fault_attempt_ids:
        std::collections::VecDeque<Option<::chaoscontrol_fault::outcomes::FaultAttemptId>>,
    stats: BlockStats,
    slow_delay_ns: u64,
    slow_attempt_id: Option<::chaoscontrol_fault::outcomes::FaultAttemptId>,
    fsync_lie: bool,
    fsync_lie_attempt_id: Option<::chaoscontrol_fault::outcomes::FaultAttemptId>,
    full: bool,
    full_attempt_id: Option<::chaoscontrol_fault::outcomes::FaultAttemptId>,
    fault_observations:
        std::collections::VecDeque<::chaoscontrol_fault::outcomes::FaultObservation>,
    operation_sequence: u64,
    observation_overflowed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockSnapshotValidationError {
    DeviceSizeOverflow,
    DeviceSizeMismatch {
        expected: u64,
        actual: u64,
    },
    FaultAttributionCount,
    ObservationCapacity,
    ObservationOverflow,
    PageIndexOverflow {
        layer: &'static str,
        page: usize,
    },
    PageOutOfBounds {
        layer: &'static str,
        page: usize,
        base_bytes: usize,
    },
    PageLength {
        layer: &'static str,
        page: usize,
        expected: usize,
        actual: usize,
    },
}

impl BlockSnapshot {
    pub fn device_size(&self) -> Result<u64, BlockSnapshotValidationError> {
        u64::try_from(self.base.len()).map_err(|_| BlockSnapshotValidationError::DeviceSizeOverflow)
    }

    pub fn validate_device_size(&self, expected: u64) -> Result<(), BlockSnapshotValidationError> {
        let actual = self.device_size()?;
        if actual != expected {
            return Err(BlockSnapshotValidationError::DeviceSizeMismatch { expected, actual });
        }
        Ok(())
    }

    /// Validate snapshot-owned structure without consulting external evidence.
    pub fn validate_structure(&self) -> Result<(), BlockSnapshotValidationError> {
        if self.faults.len() != self.fault_attempt_ids.len() {
            return Err(BlockSnapshotValidationError::FaultAttributionCount);
        }
        if self.fault_observations.len() > MAX_PENDING_BLOCK_OBSERVATIONS {
            return Err(BlockSnapshotValidationError::ObservationCapacity);
        }
        if self.observation_overflowed != 0 {
            return Err(BlockSnapshotValidationError::ObservationOverflow);
        }
        validate_snapshot_overlay("dirty", &self.dirty, self.base.len())?;
        validate_snapshot_overlay("volatile", &self.volatile, self.base.len())
    }

    /// Validate pending block mechanisms against an authoritative ledger.
    pub fn validate_pending_faults(
        &self,
        ledger: &::chaoscontrol_fault::outcomes::FaultOutcomeLedger,
        target: u32,
    ) -> Result<(), ::chaoscontrol_fault::outcomes::FaultTransitionError> {
        if self.faults.len() != self.fault_attempt_ids.len()
            || self.fault_observations.len() > MAX_PENDING_BLOCK_OBSERVATIONS
            || self.observation_overflowed != 0
        {
            return Err(
                ::chaoscontrol_fault::outcomes::FaultTransitionError::SnapshotPendingStateMismatch,
            );
        }
        for (fault, attempt_id) in self.faults.iter().zip(&self.fault_attempt_ids) {
            let attempt_id = attempt_id.ok_or(
                ::chaoscontrol_fault::outcomes::FaultTransitionError::SnapshotPendingStateMismatch,
            )?;
            let effect = block_fault_effect(target, fault)?;
            ::chaoscontrol_fault::outcomes::validate_pending_fault_effect(
                ledger, attempt_id, &effect,
            )?;
        }
        validate_active_block_effect(
            ledger,
            self.slow_delay_ns != 0,
            self.slow_attempt_id,
            ::chaoscontrol_fault::outcomes::FaultPlanEffect::BlockSlow {
                target,
                delay_ns: self.slow_delay_ns,
            },
        )?;
        validate_active_block_effect(
            ledger,
            self.fsync_lie,
            self.fsync_lie_attempt_id,
            ::chaoscontrol_fault::outcomes::FaultPlanEffect::BlockFsyncLie { target },
        )?;
        validate_active_block_effect(
            ledger,
            self.full,
            self.full_attempt_id,
            ::chaoscontrol_fault::outcomes::FaultPlanEffect::BlockFull { target },
        )?;
        if self
            .fault_observations
            .iter()
            .any(|observation| observation.operation_sequence >= self.operation_sequence)
        {
            return Err(
                ::chaoscontrol_fault::outcomes::FaultTransitionError::SnapshotPendingStateMismatch,
            );
        }
        let observations = self.fault_observations.iter().cloned().collect::<Vec<_>>();
        ::chaoscontrol_fault::outcomes::validate_pending_fault_observations(ledger, &observations)
    }
}

fn validate_snapshot_overlay(
    layer: &'static str,
    pages: &std::collections::BTreeMap<usize, Vec<u8>>,
    base_bytes: usize,
) -> Result<(), BlockSnapshotValidationError> {
    for (&page, data) in pages {
        let start = page
            .checked_mul(PAGE_SIZE)
            .ok_or(BlockSnapshotValidationError::PageIndexOverflow { layer, page })?;
        if start >= base_bytes {
            return Err(BlockSnapshotValidationError::PageOutOfBounds {
                layer,
                page,
                base_bytes,
            });
        }
        let expected = PAGE_SIZE.min(base_bytes - start);
        if data.len() != expected {
            return Err(BlockSnapshotValidationError::PageLength {
                layer,
                page,
                expected,
                actual: data.len(),
            });
        }
    }
    Ok(())
}

fn block_fault_effect(
    target: u32,
    fault: &BlockFault,
) -> Result<
    ::chaoscontrol_fault::outcomes::FaultPlanEffect,
    ::chaoscontrol_fault::outcomes::FaultTransitionError,
> {
    let effect = match fault {
        BlockFault::ReadError { offset } => ::chaoscontrol_fault::outcomes::FaultPlanEffect::BlockReadError {
            target,
            offset: *offset,
        },
        BlockFault::WriteError { offset } => ::chaoscontrol_fault::outcomes::FaultPlanEffect::BlockWriteError {
            target,
            offset: *offset,
        },
        BlockFault::TornWrite {
            offset,
            bytes_written,
        } => ::chaoscontrol_fault::outcomes::FaultPlanEffect::BlockTornWrite {
            target,
            offset: *offset,
            bytes_written: u64::try_from(*bytes_written)
                .map_err(|_| ::chaoscontrol_fault::outcomes::FaultTransitionError::SnapshotPendingStateMismatch)?,
        },
        BlockFault::Corruption { offset, len } => ::chaoscontrol_fault::outcomes::FaultPlanEffect::BlockCorruption {
            target,
            offset: *offset,
            len: u64::try_from(*len)
                .map_err(|_| ::chaoscontrol_fault::outcomes::FaultTransitionError::SnapshotPendingStateMismatch)?,
        },
        BlockFault::PartialRead { offset, max_bytes } => ::chaoscontrol_fault::outcomes::FaultPlanEffect::BlockPartialRead {
            target,
            offset: *offset,
            max_bytes: u64::try_from(*max_bytes)
                .map_err(|_| ::chaoscontrol_fault::outcomes::FaultTransitionError::SnapshotPendingStateMismatch)?,
        },
    };
    Ok(effect)
}

fn validate_active_block_effect(
    ledger: &::chaoscontrol_fault::outcomes::FaultOutcomeLedger,
    active: bool,
    attempt_id: Option<::chaoscontrol_fault::outcomes::FaultAttemptId>,
    effect: ::chaoscontrol_fault::outcomes::FaultPlanEffect,
) -> Result<(), ::chaoscontrol_fault::outcomes::FaultTransitionError> {
    match (active, attempt_id) {
        (true, Some(attempt_id)) => ::chaoscontrol_fault::outcomes::validate_pending_fault_effect(
            ledger, attempt_id, &effect,
        ),
        (false, None) => Ok(()),
        _ => {
            Err(::chaoscontrol_fault::outcomes::FaultTransitionError::SnapshotPendingStateMismatch)
        }
    }
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
    base: std::sync::Arc<Vec<u8>>,
    /// Dirty pages: page index → page data (4 KB each, last page may be shorter).
    dirty: std::collections::BTreeMap<usize, Vec<u8>>,
    /// Volatile pages (fsync-lie mode): discarded on crash, flushed explicitly.
    volatile: std::collections::BTreeMap<usize, Vec<u8>>,
    /// Pending fault injection queue.
    faults: std::collections::VecDeque<BlockFault>,
    /// Attempt identities parallel to the pending fault queue.
    fault_attempt_ids:
        std::collections::VecDeque<Option<::chaoscontrol_fault::outcomes::FaultAttemptId>>,
    /// I/O statistics.
    stats: BlockStats,
    /// Per-operation I/O delay in nanoseconds (0 = no delay).
    slow_delay_ns: u64,
    /// Attempt that armed the slow-operation mechanism.
    slow_attempt_id: Option<::chaoscontrol_fault::outcomes::FaultAttemptId>,
    /// When true, writes go to the volatile buffer instead of durable storage.
    fsync_lie: bool,
    /// Attempt that armed fsync-lie behavior.
    fsync_lie_attempt_id: Option<::chaoscontrol_fault::outcomes::FaultAttemptId>,
    /// When true, all writes fail with an injected write error.
    full: bool,
    /// Attempt that armed disk-full behavior.
    full_attempt_id: Option<::chaoscontrol_fault::outcomes::FaultAttemptId>,
    /// Bounded observations waiting for the controller ledger.
    fault_observations:
        std::collections::VecDeque<::chaoscontrol_fault::outcomes::FaultObservation>,
    /// Sequence for deterministic block-operation identities.
    operation_sequence: u64,
    /// Observations rejected because the bounded queue was full.
    observation_overflowed: u64,
}

impl DeterministicBlock {
    /// Create an empty (zero-filled) block device of `size_bytes`.
    pub fn new(size_bytes: usize) -> Self {
        Self {
            base: std::sync::Arc::new(vec![0u8; size_bytes]),
            dirty: std::collections::BTreeMap::new(),
            volatile: std::collections::BTreeMap::new(),
            faults: std::collections::VecDeque::new(),
            fault_attempt_ids: std::collections::VecDeque::new(),
            stats: BlockStats::default(),
            slow_delay_ns: 0,
            slow_attempt_id: None,
            fsync_lie: false,
            fsync_lie_attempt_id: None,
            full: false,
            full_attempt_id: None,
            fault_observations: std::collections::VecDeque::with_capacity(
                MAX_PENDING_BLOCK_OBSERVATIONS,
            ),
            operation_sequence: 0,
            observation_overflowed: 0,
        }
    }

    /// Create a block device pre-loaded with `data` (e.g. a disk image).
    pub fn from_image(data: Vec<u8>) -> Self {
        Self {
            base: std::sync::Arc::new(data),
            dirty: std::collections::BTreeMap::new(),
            volatile: std::collections::BTreeMap::new(),
            faults: std::collections::VecDeque::new(),
            fault_attempt_ids: std::collections::VecDeque::new(),
            stats: BlockStats::default(),
            slow_delay_ns: 0,
            slow_attempt_id: None,
            fsync_lie: false,
            fsync_lie_attempt_id: None,
            full: false,
            full_attempt_id: None,
            fault_observations: std::collections::VecDeque::with_capacity(
                MAX_PENDING_BLOCK_OBSERVATIONS,
            ),
            operation_sequence: 0,
            observation_overflowed: 0,
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
        let observation_count = self.read_observation_count(offset, buf.len());
        let operation_sequence = self.begin_operation(observation_count)?;

        // Check for an injected read fault at this offset.
        if let Some(idx) =
            self.find_fault(|f| matches!(f, BlockFault::ReadError { offset: o } if *o == offset))
        {
            let attempt_id = self.remove_fault_attempt(idx);
            let fault = self.faults.remove(idx).unwrap();
            if let BlockFault::ReadError { offset } = fault {
                self.record_observation(
                    attempt_id,
                    operation_sequence,
                    ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockReadFailed,
                );
                return InjectedReadSnafu { offset }.fail();
            }
        }

        // Check for partial read fault.
        if let Some(idx) = self
            .find_fault(|f| matches!(f, BlockFault::PartialRead { offset: o, .. } if *o == offset))
        {
            let max_bytes = match self.faults.get(idx) {
                Some(BlockFault::PartialRead { max_bytes, .. }) => *max_bytes,
                _ => unreachable!(),
            };
            if max_bytes < buf.len() {
                let attempt_id = self.remove_fault_attempt(idx);
                self.faults.remove(idx).unwrap();
                // Read the partial portion.
                self.cow_read_3tier(offset as usize, &mut buf[..max_bytes]);
                // Zero the rest.
                buf[max_bytes..].fill(0);
                self.stats.reads += 1;
                self.stats.bytes_read += max_bytes as u64;
                self.record_observation(
                    attempt_id,
                    operation_sequence,
                    ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockReadShortened,
                );
                self.record_slow_observation(operation_sequence);
                return Ok(self.slow_delay_ns);
            }
        }

        self.cow_read_3tier(offset as usize, buf);

        self.stats.reads += 1;
        self.stats.bytes_read += len;
        self.record_slow_observation(operation_sequence);
        Ok(self.slow_delay_ns)
    }

    /// Write `data` starting at `offset`.
    ///
    /// Returns the I/O delay in nanoseconds (0 unless `DiskSlow` is active).
    pub fn write(&mut self, offset: u64, data: &[u8]) -> Result<u64, BlockError> {
        let len = data.len() as u64;
        self.check_bounds(offset, len)?;
        let observation_count = self.write_observation_count(offset, data.len());
        let operation_sequence = self.begin_operation(observation_count)?;
        if self.full {
            self.record_observation(
                self.full_attempt_id,
                operation_sequence,
                ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockWriteRejectedFull,
            );
            return InjectedWriteSnafu { offset }.fail();
        }

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
            let fault = self.faults.get(idx).cloned().unwrap();
            match fault {
                BlockFault::WriteError { offset } => {
                    let attempt_id = self.remove_fault_attempt(idx);
                    self.faults.remove(idx).unwrap();
                    self.record_observation(
                        attempt_id,
                        operation_sequence,
                        ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockWriteFailed,
                    );
                    return InjectedWriteSnafu { offset }.fail();
                }
                BlockFault::TornWrite {
                    offset,
                    bytes_written,
                } if bytes_written < data.len() => {
                    let attempt_id = self.remove_fault_attempt(idx);
                    self.faults.remove(idx).unwrap();
                    self.write_to_layer(offset as usize, &data[..bytes_written]);
                    self.stats.writes += 1;
                    self.stats.bytes_written += bytes_written as u64;
                    self.record_observation(
                        attempt_id,
                        operation_sequence,
                        ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockWriteTorn,
                    );
                    self.record_fsync_lie_observation(operation_sequence, bytes_written > 0);
                    return InjectedTornWriteSnafu {
                        offset,
                        bytes_written,
                    }
                    .fail();
                }
                BlockFault::Corruption {
                    offset: _,
                    len: corrupt_len,
                } if corrupt_len > 0 && !data.is_empty() => {
                    let attempt_id = self.remove_fault_attempt(idx);
                    self.faults.remove(idx).unwrap();
                    // Write the data normally, then replace a prefix with different bytes.
                    self.write_to_layer(offset as usize, data);
                    let corrupt_end = corrupt_len.min(data.len());
                    let corrupted = data[..corrupt_end]
                        .iter()
                        .map(|byte| byte ^ u8::MAX)
                        .collect::<Vec<_>>();
                    self.write_to_layer(offset as usize, &corrupted);
                    self.stats.writes += 1;
                    self.stats.bytes_written += data.len() as u64;
                    self.record_observation(
                        attempt_id,
                        operation_sequence,
                        ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockBytesCorrupted,
                    );
                    self.record_fsync_lie_observation(operation_sequence, true);
                    self.record_slow_observation(operation_sequence);
                    return Ok(self.slow_delay_ns);
                }
                BlockFault::TornWrite { .. } | BlockFault::Corruption { .. } => {}
                _ => unreachable!(),
            }
        }

        self.write_to_layer(offset as usize, data);

        self.stats.writes += 1;
        self.stats.bytes_written += len;
        self.record_fsync_lie_observation(operation_sequence, !data.is_empty());
        self.record_slow_observation(operation_sequence);
        Ok(self.slow_delay_ns)
    }

    /// Enqueue a fault without evidence attribution.
    pub fn inject_fault(&mut self, fault: BlockFault) {
        self.faults.push_back(fault);
        self.fault_attempt_ids.push_back(None);
    }

    /// Enqueue a fault bound to one selected attempt.
    pub fn inject_fault_with_attempt(
        &mut self,
        fault: BlockFault,
        attempt_id: ::chaoscontrol_fault::outcomes::FaultAttemptId,
    ) {
        self.faults.push_back(fault);
        self.fault_attempt_ids.push_back(Some(attempt_id));
    }

    /// Set persistent disk-full behavior without evidence attribution.
    pub fn set_full(&mut self, is_full: bool) {
        self.full = is_full;
        self.full_attempt_id = None;
    }

    /// Set persistent disk-full behavior for one selected attempt.
    pub fn set_full_with_attempt(
        &mut self,
        is_full: bool,
        attempt_id: ::chaoscontrol_fault::outcomes::FaultAttemptId,
    ) {
        self.full = is_full;
        self.full_attempt_id = is_full.then_some(attempt_id);
    }

    /// Return whether persistent disk-full behavior is active.
    pub fn is_full(&self) -> bool {
        self.full
    }

    /// Set the per-I/O delay without evidence attribution.
    pub fn set_slow_delay_ns(&mut self, delay_ns: u64) {
        self.slow_delay_ns = delay_ns;
        self.slow_attempt_id = None;
    }

    /// Set the per-I/O delay for one selected attempt.
    pub fn set_slow_delay_with_attempt(
        &mut self,
        delay_ns: u64,
        attempt_id: ::chaoscontrol_fault::outcomes::FaultAttemptId,
    ) {
        self.slow_delay_ns = delay_ns;
        self.slow_attempt_id = (delay_ns > 0).then_some(attempt_id);
    }

    /// Current slow delay setting.
    pub fn slow_delay_ns(&self) -> u64 {
        self.slow_delay_ns
    }

    /// Enable fsync-lie mode without evidence attribution.
    pub fn enable_fsync_lie(&mut self) {
        self.fsync_lie = true;
        self.fsync_lie_attempt_id = None;
    }

    /// Enable fsync-lie mode for one selected attempt.
    pub fn enable_fsync_lie_with_attempt(
        &mut self,
        attempt_id: ::chaoscontrol_fault::outcomes::FaultAttemptId,
    ) {
        self.fsync_lie = true;
        self.fsync_lie_attempt_id = Some(attempt_id);
    }

    /// Disable fsync-lie mode. Does NOT flush pending volatile writes.
    pub fn disable_fsync_lie(&mut self) {
        self.fsync_lie = false;
        self.fsync_lie_attempt_id = None;
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

    /// Return the maximum new attributed observations that this queue can retain.
    pub fn central_observation_reservation(&self) -> usize {
        let attributed_mechanism_active = self.fault_attempt_ids.iter().any(Option::is_some)
            || self.slow_attempt_id.is_some()
            || self.fsync_lie_attempt_id.is_some()
            || self.full_attempt_id.is_some();
        if attributed_mechanism_active {
            MAX_PENDING_BLOCK_OBSERVATIONS - self.fault_observations.len()
        } else {
            0
        }
    }

    /// Drain bounded block observations and the overflow count.
    pub fn drain_fault_observations(
        &mut self,
    ) -> (Vec<::chaoscontrol_fault::outcomes::FaultObservation>, u64) {
        let observations = self.fault_observations.drain(..).collect();
        let overflowed = self.observation_overflowed;
        self.observation_overflowed = 0;
        (observations, overflowed)
    }

    /// Restore a failed ledger batch to the front of the pending queue.
    pub fn requeue_fault_observations(
        &mut self,
        observations: Vec<::chaoscontrol_fault::outcomes::FaultObservation>,
        overflowed: u64,
    ) {
        let restored_len = self
            .fault_observations
            .len()
            .checked_add(observations.len())
            .expect("block observation queue length overflow");
        assert!(restored_len <= MAX_PENDING_BLOCK_OBSERVATIONS);
        for observation in observations.into_iter().rev() {
            self.fault_observations.push_front(observation);
        }
        self.observation_overflowed = self
            .observation_overflowed
            .checked_add(overflowed)
            .expect("block observation overflow counter overflow");
    }

    #[cfg(test)]
    pub(crate) fn set_observation_overflow_for_test(&mut self, overflowed: u64) {
        self.observation_overflowed = overflowed;
    }

    /// Capture a snapshot of the device (CoW state + faults + stats).
    ///
    /// Cost is proportional to the number of dirty pages, not the device
    /// size. The base image is shared via `Arc` reference counting.
    pub fn snapshot(&self) -> BlockSnapshot {
        BlockSnapshot {
            base: std::sync::Arc::clone(&self.base),
            dirty: self.dirty.clone(),
            volatile: self.volatile.clone(),
            faults: self.faults.clone(),
            fault_attempt_ids: self.fault_attempt_ids.clone(),
            stats: self.stats.clone(),
            slow_delay_ns: self.slow_delay_ns,
            slow_attempt_id: self.slow_attempt_id,
            fsync_lie: self.fsync_lie,
            fsync_lie_attempt_id: self.fsync_lie_attempt_id,
            full: self.full,
            full_attempt_id: self.full_attempt_id,
            fault_observations: self.fault_observations.clone(),
            operation_sequence: self.operation_sequence,
            observation_overflowed: self.observation_overflowed,
        }
    }

    /// Restore a device from a snapshot.
    ///
    /// Shares the base image with the snapshot via `Arc`.
    pub fn restore(snapshot: &BlockSnapshot) -> Self {
        Self {
            base: std::sync::Arc::clone(&snapshot.base),
            dirty: snapshot.dirty.clone(),
            volatile: snapshot.volatile.clone(),
            faults: snapshot.faults.clone(),
            fault_attempt_ids: snapshot.fault_attempt_ids.clone(),
            stats: snapshot.stats.clone(),
            slow_delay_ns: snapshot.slow_delay_ns,
            slow_attempt_id: snapshot.slow_attempt_id,
            fsync_lie: snapshot.fsync_lie,
            fsync_lie_attempt_id: snapshot.fsync_lie_attempt_id,
            full: snapshot.full,
            full_attempt_id: snapshot.full_attempt_id,
            fault_observations: snapshot.fault_observations.clone(),
            operation_sequence: snapshot.operation_sequence,
            observation_overflowed: snapshot.observation_overflowed,
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

    /// Next deterministic operation sequence.
    pub fn operation_sequence(&self) -> u64 {
        self.operation_sequence
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

    fn begin_operation(&mut self, observation_count: usize) -> Result<u64, BlockError> {
        let operation_sequence = self.operation_sequence;
        let next_sequence = operation_sequence
            .checked_add(1)
            .ok_or(BlockError::OperationSequenceExhausted)?;
        let available = MAX_PENDING_BLOCK_OBSERVATIONS
            .checked_sub(self.fault_observations.len())
            .ok_or(BlockError::ObservationCapacity {
                required: observation_count,
                available: 0,
            })?;
        if observation_count > available {
            return Err(BlockError::ObservationCapacity {
                required: observation_count,
                available,
            });
        }
        self.operation_sequence = next_sequence;
        Ok(operation_sequence)
    }

    fn read_observation_count(&self, offset: u64, read_len: usize) -> usize {
        if let Some(index) = self.find_fault(
            |fault| matches!(fault, BlockFault::ReadError { offset: value } if *value == offset),
        ) {
            return usize::from(self.fault_attempt(index).is_some());
        }
        if let Some(index) = self.find_fault(
            |fault| matches!(fault, BlockFault::PartialRead { offset: value, .. } if *value == offset),
        ) {
            if matches!(
                self.faults.get(index),
                Some(BlockFault::PartialRead { max_bytes, .. }) if *max_bytes < read_len
            ) {
                return usize::from(self.fault_attempt(index).is_some())
                    + usize::from(self.slow_attempt_id.is_some());
            }
        }
        usize::from(self.slow_attempt_id.is_some())
    }

    fn write_observation_count(&self, offset: u64, write_len: usize) -> usize {
        if self.full {
            return usize::from(self.full_attempt_id.is_some());
        }
        if let Some(index) = self.find_fault(|fault| {
            matches!(
                fault,
                BlockFault::WriteError { offset: value }
                    | BlockFault::TornWrite { offset: value, .. }
                    | BlockFault::Corruption { offset: value, .. }
                    if *value == offset
            )
        }) {
            let primary = usize::from(self.fault_attempt(index).is_some());
            match self.faults.get(index) {
                Some(BlockFault::WriteError { .. }) => return primary,
                Some(BlockFault::TornWrite { bytes_written, .. }) if *bytes_written < write_len => {
                    return primary
                        + usize::from(*bytes_written > 0 && self.fsync_lie_attempt_id.is_some());
                }
                Some(BlockFault::Corruption { len, .. }) if *len > 0 && write_len > 0 => {
                    return primary
                        + usize::from(self.fsync_lie_attempt_id.is_some())
                        + usize::from(self.slow_attempt_id.is_some());
                }
                _ => {}
            }
        }
        usize::from(write_len > 0 && self.fsync_lie_attempt_id.is_some())
            + usize::from(self.slow_attempt_id.is_some())
    }

    fn fault_attempt(
        &self,
        index: usize,
    ) -> Option<::chaoscontrol_fault::outcomes::FaultAttemptId> {
        self.fault_attempt_ids.get(index).copied().flatten()
    }

    fn record_observation(
        &mut self,
        attempt_id: Option<::chaoscontrol_fault::outcomes::FaultAttemptId>,
        operation_sequence: u64,
        effect: ::chaoscontrol_fault::outcomes::FaultObservationEffect,
    ) {
        let Some(attempt_id) = attempt_id else {
            return;
        };
        assert!(self.fault_observations.len() < MAX_PENDING_BLOCK_OBSERVATIONS);
        self.fault_observations
            .push_back(::chaoscontrol_fault::outcomes::FaultObservation::new(
                attempt_id,
                ::chaoscontrol_fault::outcomes::FaultObservationSubsystem::Block,
                operation_sequence,
                effect,
            ));
    }

    fn record_slow_observation(&mut self, operation_sequence: u64) {
        if self.slow_delay_ns > 0 {
            self.record_observation(
                self.slow_attempt_id,
                operation_sequence,
                ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockOperationDelayed,
            );
        }
    }

    fn record_fsync_lie_observation(&mut self, operation_sequence: u64, wrote_bytes: bool) {
        if self.fsync_lie && wrote_bytes {
            self.record_observation(
                self.fsync_lie_attempt_id,
                operation_sequence,
                ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockWriteMadeVolatile,
            );
        }
    }

    fn remove_fault_attempt(
        &mut self,
        index: usize,
    ) -> Option<::chaoscontrol_fault::outcomes::FaultAttemptId> {
        self.fault_attempt_ids.remove(index).flatten()
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
        assert_eq!(&buf[..4], &[0x55; 4]); // input bytes were changed
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
    fn snapshot_structure_accepts_valid_partial_last_page() {
        let base_bytes = PAGE_SIZE + 1;
        let mut block = DeterministicBlock::new(base_bytes);
        block
            .write(u64::try_from(PAGE_SIZE).expect("page offset"), &[u8::MAX])
            .expect("write final partial page");

        assert_eq!(block.snapshot().validate_structure(), Ok(()));
    }

    #[test]
    fn snapshot_structure_rejects_out_of_range_and_malformed_pages() {
        let mut out_of_range = DeterministicBlock::new(PAGE_SIZE).snapshot();
        out_of_range.dirty.insert(1, vec![0; PAGE_SIZE]);
        assert!(matches!(
            out_of_range.validate_structure(),
            Err(BlockSnapshotValidationError::PageOutOfBounds {
                layer: "dirty",
                page: 1,
                base_bytes: PAGE_SIZE,
            })
        ));

        let mut malformed = DeterministicBlock::new(PAGE_SIZE).snapshot();
        malformed
            .volatile
            .insert(0, vec![0; PAGE_SIZE.saturating_sub(1)]);
        assert!(matches!(
            malformed.validate_structure(),
            Err(BlockSnapshotValidationError::PageLength {
                layer: "volatile",
                page: 0,
                expected: PAGE_SIZE,
                actual,
            }) if actual == PAGE_SIZE.saturating_sub(1)
        ));

        let snapshot = DeterministicBlock::new(PAGE_SIZE).snapshot();
        let actual = u64::try_from(PAGE_SIZE).expect("page size fits u64");
        let expected = actual.saturating_add(1);
        assert_eq!(
            snapshot.validate_device_size(expected),
            Err(BlockSnapshotValidationError::DeviceSizeMismatch { expected, actual })
        );
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
        assert!(std::sync::Arc::ptr_eq(&blk.base, &snap.base));

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
        assert!(std::sync::Arc::ptr_eq(&snap1.base, &snap2.base));
        assert!(std::sync::Arc::ptr_eq(&snap2.base, &snap3.base));

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
        assert!(std::sync::Arc::ptr_eq(&branch_a.base, &branch_b.base));
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

    #[test]
    fn torn_write_stays_armed_until_it_writes_fewer_bytes_than_requested() {
        const BLOCK_BYTES: usize = 4_096;
        const TORN_BYTES: usize = 4;
        const LONG_WRITE_BYTES: usize = 8;
        let attempt_id = ::chaoscontrol_fault::outcomes::FaultAttemptId([0; 32]);
        let mut disk = DeterministicBlock::new(BLOCK_BYTES);
        disk.inject_fault_with_attempt(
            BlockFault::TornWrite {
                offset: 0,
                bytes_written: TORN_BYTES,
            },
            attempt_id,
        );

        assert!(disk.write(0, &[0xAA; TORN_BYTES]).is_ok());
        assert_eq!(disk.faults.len(), 1);
        assert!(disk.drain_fault_observations().0.is_empty());

        assert!(matches!(
            disk.write(0, &[0xBB; LONG_WRITE_BYTES]),
            Err(BlockError::InjectedTornWrite { .. })
        ));
        let (observations, overflowed) = disk.drain_fault_observations();
        assert_eq!(overflowed, 0);
        assert_eq!(disk.faults.len(), 0);
        assert_eq!(observations.len(), 1);
        assert_eq!(
            observations[0].effect,
            ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockWriteTorn
        );
    }

    #[test]
    fn partial_read_stays_armed_for_equal_or_empty_reads() {
        const BLOCK_BYTES: usize = 4_096;
        const SHORT_READ_BYTES: usize = 4;
        const LONG_READ_BYTES: usize = 8;
        let attempt_id = ::chaoscontrol_fault::outcomes::FaultAttemptId([0; 32]);
        let mut disk = DeterministicBlock::new(BLOCK_BYTES);
        disk.write(0, &[0xAA; LONG_READ_BYTES]).unwrap();
        disk.inject_fault_with_attempt(
            BlockFault::PartialRead {
                offset: 0,
                max_bytes: SHORT_READ_BYTES,
            },
            attempt_id,
        );

        disk.read(0, &mut []).unwrap();
        let mut equal = [0; SHORT_READ_BYTES];
        disk.read(0, &mut equal).unwrap();
        assert_eq!(equal, [0xAA; SHORT_READ_BYTES]);
        assert_eq!(disk.faults.len(), 1);
        assert!(disk.drain_fault_observations().0.is_empty());

        let mut longer = [0xFF; LONG_READ_BYTES];
        disk.read(0, &mut longer).unwrap();
        let (observations, overflowed) = disk.drain_fault_observations();
        assert_eq!(overflowed, 0);
        assert_eq!(&longer[..SHORT_READ_BYTES], &[0xAA; SHORT_READ_BYTES]);
        assert_eq!(&longer[SHORT_READ_BYTES..], &[0; SHORT_READ_BYTES]);
        assert_eq!(disk.faults.len(), 0);
        assert_eq!(observations.len(), 1);
        assert_eq!(
            observations[0].effect,
            ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockReadShortened
        );
    }

    #[test]
    fn corruption_stays_armed_for_empty_write_and_changes_nonempty_bytes() {
        const BLOCK_BYTES: usize = 4_096;
        const WRITE_BYTES: usize = 4;
        let attempt_id = ::chaoscontrol_fault::outcomes::FaultAttemptId([0; 32]);
        let mut disk = DeterministicBlock::new(BLOCK_BYTES);
        disk.inject_fault_with_attempt(
            BlockFault::Corruption {
                offset: 0,
                len: WRITE_BYTES,
            },
            attempt_id,
        );

        disk.write(0, &[]).unwrap();
        assert_eq!(disk.faults.len(), 1);
        assert!(disk.drain_fault_observations().0.is_empty());

        disk.write(0, &[0xAA; WRITE_BYTES]).unwrap();
        let (observations, overflowed) = disk.drain_fault_observations();
        let mut actual = [0; WRITE_BYTES];
        disk.read(0, &mut actual).unwrap();
        assert_eq!(overflowed, 0);
        assert_eq!(actual, [0x55; WRITE_BYTES]);
        assert_eq!(disk.faults.len(), 0);
        assert_eq!(observations.len(), 1);
        assert_eq!(
            observations[0].effect,
            ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockBytesCorrupted
        );
    }

    #[test]
    fn armed_block_fault_is_observed_only_when_io_consumes_it() {
        // r[verify chaoscontrol.fault_outcomes.validation.observation]
        const BLOCK_BYTES: usize = 4_096;
        let attempt_id = ::chaoscontrol_fault::outcomes::FaultAttemptId([0; 32]);
        let mut block = DeterministicBlock::new(BLOCK_BYTES);
        block.inject_fault_with_attempt(BlockFault::ReadError { offset: 0 }, attempt_id);

        let (before, overflowed) = block.drain_fault_observations();
        assert!(before.is_empty());
        assert_eq!(overflowed, 0);

        let mut buffer = [0u8; 8];
        let result = block.read(0, &mut buffer);
        assert!(matches!(result, Err(BlockError::InjectedReadError { .. })));
        let (after, overflowed) = block.drain_fault_observations();
        assert_eq!(overflowed, 0);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].attempt_id, attempt_id);
        assert_eq!(
            after[0].effect,
            ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockReadFailed
        );
    }

    #[test]
    fn unmatched_block_fault_does_not_create_observation() {
        const BLOCK_BYTES: usize = 4_096;
        const OTHER_OFFSET: u64 = 512;
        let attempt_id = ::chaoscontrol_fault::outcomes::FaultAttemptId([8; 32]);
        let mut block = DeterministicBlock::new(BLOCK_BYTES);
        block.inject_fault_with_attempt(
            BlockFault::ReadError {
                offset: OTHER_OFFSET,
            },
            attempt_id,
        );

        let mut buffer = [0u8; 8];
        assert!(block.read(0, &mut buffer).is_ok());
        let (observations, overflowed) = block.drain_fault_observations();
        assert!(observations.is_empty());
        assert_eq!(overflowed, 0);
    }

    #[test]
    fn disk_full_and_snapshot_replay_preserve_attempt_observation() {
        const BLOCK_BYTES: usize = 4_096;
        let attempt_id = ::chaoscontrol_fault::outcomes::FaultAttemptId([9; 32]);
        let mut block = DeterministicBlock::new(BLOCK_BYTES);
        block.set_full_with_attempt(true, attempt_id);
        let snapshot = block.snapshot();

        let first_result = block.write(0, &[1]);
        assert!(matches!(
            first_result,
            Err(BlockError::InjectedWriteError { .. })
        ));
        let (first, first_overflowed) = block.drain_fault_observations();

        let mut replay = DeterministicBlock::restore(&snapshot);
        let replay_result = replay.write(0, &[1]);
        assert!(matches!(
            replay_result,
            Err(BlockError::InjectedWriteError { .. })
        ));
        let (second, second_overflowed) = replay.drain_fault_observations();

        assert_eq!(first_overflowed, 0);
        assert_eq!(second_overflowed, 0);
        assert_eq!(first, second);
        assert_eq!(
            first[0].effect,
            ::chaoscontrol_fault::outcomes::FaultObservationEffect::BlockWriteRejectedFull
        );
    }
}
