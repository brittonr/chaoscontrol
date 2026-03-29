//! Determinism log — per-exit binary event log for diagnosing determinism regressions.
//!
//! When enabled, the VMM records a fixed-size [`DlogRecord`] for every VM exit
//! and significant internal event (scheduler switches, fault injections,
//! snapshot boundaries). Two logs from runs with the same seed can be compared
//! with [`dlog_diff`] to find the exact exit where execution diverged.
//!
//! # Format
//!
//! Each record is exactly 64 bytes (`repr(C)`), written sequentially with no
//! framing. Record `n` lives at file offset `n * 64`. This makes seeking
//! trivial and diff O(min(len_a, len_b)).
//!
//! # Performance
//!
//! The writer uses a 64 KB buffered file (~1024 records per syscall).
//! At 500K exits/sec this is ~490 write syscalls/sec — negligible compared
//! to KVM_RUN overhead.

use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

// ═══════════════════════════════════════════════════════════════════════
//  Record format
// ═══════════════════════════════════════════════════════════════════════

/// Size of a single dlog record in bytes.
pub const RECORD_SIZE: usize = 64;

/// Event tag identifying what kind of record this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DlogTag {
    IoIn = 1,
    IoOut = 2,
    MmioRead = 3,
    MmioWrite = 4,
    Hlt = 5,
    Shutdown = 6,
    Hypercall = 7,
    SdkHypercall = 8,
    SchedulerSwitch = 9,
    Intr = 10,
    Debug = 11,
    IrqWindowOpen = 12,
    InternalError = 13,
    FaultApplied = 14,
    InterruptInjected = 15,
    NmiInjected = 16,
    SnapshotTaken = 17,
    SnapshotRestored = 18,
    CoverageSync = 19,
    Marker = 255,
}

impl DlogTag {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::IoIn),
            2 => Some(Self::IoOut),
            3 => Some(Self::MmioRead),
            4 => Some(Self::MmioWrite),
            5 => Some(Self::Hlt),
            6 => Some(Self::Shutdown),
            7 => Some(Self::Hypercall),
            8 => Some(Self::SdkHypercall),
            9 => Some(Self::SchedulerSwitch),
            10 => Some(Self::Intr),
            11 => Some(Self::Debug),
            12 => Some(Self::IrqWindowOpen),
            13 => Some(Self::InternalError),
            14 => Some(Self::FaultApplied),
            15 => Some(Self::InterruptInjected),
            16 => Some(Self::NmiInjected),
            17 => Some(Self::SnapshotTaken),
            18 => Some(Self::SnapshotRestored),
            19 => Some(Self::CoverageSync),
            255 => Some(Self::Marker),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::IoIn => "IoIn",
            Self::IoOut => "IoOut",
            Self::MmioRead => "MmioRead",
            Self::MmioWrite => "MmioWrite",
            Self::Hlt => "Hlt",
            Self::Shutdown => "Shutdown",
            Self::Hypercall => "Hypercall",
            Self::SdkHypercall => "SdkHypercall",
            Self::SchedulerSwitch => "SchedSwitch",
            Self::Intr => "Intr",
            Self::Debug => "Debug",
            Self::IrqWindowOpen => "IrqWinOpen",
            Self::InternalError => "InternalErr",
            Self::FaultApplied => "FaultApplied",
            Self::InterruptInjected => "IntInject",
            Self::NmiInjected => "NmiInject",
            Self::SnapshotTaken => "SnapTaken",
            Self::SnapshotRestored => "SnapRestored",
            Self::CoverageSync => "CovSync",
            Self::Marker => "Marker",
        }
    }
}

impl fmt::Display for DlogTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A single determinism log record.
///
/// Fixed at 64 bytes. Written and read as raw bytes — no serde, no framing.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DlogRecord {
    /// Monotonically increasing sequence number per VM.
    pub seq: u64,
    /// Virtual TSC at time of record.
    pub virtual_tsc: u64,
    /// VM exit count (matches `DeterministicVm::exit_count`).
    pub exit_count: u64,
    /// Guest RIP at the exit point.
    pub rip: u64,
    /// Record type tag.
    pub tag: u8,
    /// Active vCPU index.
    pub vcpu: u8,
    /// I/O port (for IoIn/IoOut) or low 16 bits of MMIO address.
    pub port_or_addr_lo: u16,
    /// High 16 bits of MMIO address (0 for port I/O).
    pub port_or_addr_hi: u16,
    /// Padding to align `data` to offset 42.
    pub _pad0: u16,
    /// First 8 bytes of I/O data, SDK command ID, fault type, etc.
    pub data: [u8; 8],
    /// Extra context: scheduler quantum remaining, SDK response,
    /// interrupt vector, etc.
    pub extra: [u8; 8],
    /// Reserved — keeps the struct at exactly 64 bytes.
    pub _pad1: [u8; 4],
}

// Compile-time size check.
const _: () = assert!(std::mem::size_of::<DlogRecord>() == RECORD_SIZE);

impl DlogRecord {
    /// Create a new record. Caller fills in context-specific fields;
    /// this constructor zeros out everything else.
    pub fn new(
        seq: u64,
        virtual_tsc: u64,
        exit_count: u64,
        rip: u64,
        tag: DlogTag,
        vcpu: u8,
    ) -> Self {
        Self {
            seq,
            virtual_tsc,
            exit_count,
            rip,
            tag: tag as u8,
            vcpu,
            port_or_addr_lo: 0,
            port_or_addr_hi: 0,
            _pad0: 0,
            data: [0; 8],
            extra: [0; 8],
            _pad1: [0; 4],
        }
    }

    /// Set port (for IoIn/IoOut).
    pub fn with_port(mut self, port: u16) -> Self {
        self.port_or_addr_lo = port;
        self
    }

    /// Set MMIO address (split across lo/hi).
    pub fn with_mmio_addr(mut self, addr: u64) -> Self {
        self.port_or_addr_lo = addr as u16;
        self.port_or_addr_hi = (addr >> 16) as u16;
        self
    }

    /// Set the data field from a byte slice (up to 8 bytes).
    pub fn with_data(mut self, src: &[u8]) -> Self {
        let n = src.len().min(8);
        self.data[..n].copy_from_slice(&src[..n]);
        self
    }

    /// Set the extra field from a byte slice (up to 8 bytes).
    pub fn with_extra(mut self, src: &[u8]) -> Self {
        let n = src.len().min(8);
        self.extra[..n].copy_from_slice(&src[..n]);
        self
    }

    /// Set extra from a u64 (little-endian).
    pub fn with_extra_u64(mut self, v: u64) -> Self {
        self.extra = v.to_le_bytes();
        self
    }

    /// Set data from a u64 (little-endian).
    pub fn with_data_u64(mut self, v: u64) -> Self {
        self.data = v.to_le_bytes();
        self
    }

    /// Parsed tag, or `None` for unknown values.
    pub fn tag(&self) -> Option<DlogTag> {
        DlogTag::from_u8(self.tag)
    }

    /// Full 32-bit MMIO address reconstructed from lo/hi.
    pub fn mmio_addr(&self) -> u64 {
        (self.port_or_addr_hi as u64) << 16 | self.port_or_addr_lo as u64
    }

    /// Encode to a 64-byte array.
    pub fn to_bytes(&self) -> [u8; RECORD_SIZE] {
        // Safety: DlogRecord is repr(C), 64 bytes, no padding holes we
        // care about (we zero-init everything).
        unsafe { std::mem::transmute(*self) }
    }

    /// Decode from a 64-byte array.
    pub fn from_bytes(bytes: [u8; RECORD_SIZE]) -> Self {
        unsafe { std::mem::transmute(bytes) }
    }

    /// Compare two records for divergence. Returns true if they match
    /// on the fields that matter for determinism.
    ///
    /// When `strict` is true, RIP is also compared.
    pub fn determinism_eq(&self, other: &Self, strict: bool) -> bool {
        self.tag == other.tag
            && self.exit_count == other.exit_count
            && self.virtual_tsc == other.virtual_tsc
            && self.vcpu == other.vcpu
            && self.port_or_addr_lo == other.port_or_addr_lo
            && self.port_or_addr_hi == other.port_or_addr_hi
            && self.data == other.data
            && self.extra == other.extra
            && (!strict || self.rip == other.rip)
    }
}

impl fmt::Display for DlogRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag_name = self.tag().map_or("Unknown", |t| t.name());
        write!(
            f,
            "#{:<8} tsc={:<12} exits={:<8} vcpu={} {:14} ",
            self.seq, self.virtual_tsc, self.exit_count, self.vcpu, tag_name,
        )?;
        match self.tag() {
            Some(DlogTag::IoIn | DlogTag::IoOut) => {
                write!(
                    f,
                    "port=0x{:04x} data={:02x?}",
                    self.port_or_addr_lo,
                    &self.data[..4]
                )?;
            }
            Some(DlogTag::MmioRead | DlogTag::MmioWrite) => {
                write!(
                    f,
                    "addr=0x{:08x} data={:02x?}",
                    self.mmio_addr(),
                    &self.data[..8]
                )?;
            }
            Some(DlogTag::SchedulerSwitch) => {
                write!(
                    f,
                    "prev_vcpu={} quantum_left={}",
                    self.data[0],
                    u64::from_le_bytes(self.extra)
                )?;
            }
            Some(DlogTag::FaultApplied) => {
                write!(f, "fault_type={}", u64::from_le_bytes(self.data))?;
            }
            Some(DlogTag::InterruptInjected) => {
                write!(f, "irq={}", u64::from_le_bytes(self.data))?;
            }
            Some(DlogTag::NmiInjected) => {
                write!(f, "target_vcpu={}", u64::from_le_bytes(self.data))?;
            }
            _ => {
                // RIP is generally useful for everything else.
                write!(f, "rip=0x{:x}", self.rip)?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for DlogRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Writer
// ═══════════════════════════════════════════════════════════════════════

/// Buffered binary writer for determinism log records.
pub struct DlogWriter {
    writer: BufWriter<File>,
    seq: u64,
}

impl DlogWriter {
    /// Create a new writer that writes to `path`.
    /// Creates the file (truncating if it exists).
    pub fn create(path: &Path) -> io::Result<Self> {
        let file = File::create(path)?;
        Ok(Self {
            writer: BufWriter::with_capacity(64 * 1024, file),
            seq: 0,
        })
    }

    /// Write a record to the log. Assigns the next sequence number.
    pub fn emit(&mut self, mut record: DlogRecord) -> io::Result<()> {
        record.seq = self.seq;
        self.seq += 1;
        self.writer.write_all(&record.to_bytes())
    }

    /// Flush buffered data to disk.
    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    /// Current sequence number (records written so far).
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

impl Drop for DlogWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Reader
// ═══════════════════════════════════════════════════════════════════════

/// Sequential reader for determinism log files.
pub struct DlogReader {
    reader: BufReader<File>,
    offset: u64,
}

impl DlogReader {
    /// Open a dlog file for reading.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(Self {
            reader: BufReader::with_capacity(64 * 1024, file),
            offset: 0,
        })
    }

    /// Read the next record. Returns `None` at EOF.
    pub fn next_record(&mut self) -> io::Result<Option<DlogRecord>> {
        let mut buf = [0u8; RECORD_SIZE];
        match self.reader.read_exact(&mut buf) {
            Ok(()) => {
                self.offset += 1;
                Ok(Some(DlogRecord::from_bytes(buf)))
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Number of records read so far.
    pub fn records_read(&self) -> u64 {
        self.offset
    }
}

impl Iterator for DlogReader {
    type Item = io::Result<DlogRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_record() {
            Ok(Some(r)) => Some(Ok(r)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Diff
// ═══════════════════════════════════════════════════════════════════════

/// Result of comparing two determinism logs.
#[derive(Debug)]
pub enum DiffResult {
    /// Logs are identical (up to the length of the shorter one).
    Identical { records: u64 },
    /// First divergence found at this record index.
    Diverged {
        /// Record index (0-based) where divergence was detected.
        index: u64,
        /// Up to 5 records before the divergence from log A.
        context_a: Vec<DlogRecord>,
        /// Up to 5 records before the divergence from log B.
        context_b: Vec<DlogRecord>,
        /// The divergent record from log A.
        record_a: DlogRecord,
        /// The divergent record from log B.
        record_b: DlogRecord,
    },
    /// Logs match but have different lengths.
    LengthMismatch {
        /// Records that matched.
        matched: u64,
        /// Length of log A.
        len_a: u64,
        /// Length of log B.
        len_b: u64,
    },
}

impl fmt::Display for DiffResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identical { records } => write!(f, "Identical: {} records", records),
            Self::Diverged {
                index,
                context_a,
                context_b,
                record_a,
                record_b,
            } => {
                writeln!(f, "Divergence at record #{index}")?;
                writeln!(f)?;
                let ctx_start = index.saturating_sub(context_a.len() as u64);
                for (i, rec) in context_a.iter().enumerate() {
                    writeln!(f, "  A[{}]: {}", ctx_start + i as u64, rec)?;
                }
                writeln!(f, ">>A[{index}]: {record_a}")?;
                writeln!(f)?;
                let ctx_start = index.saturating_sub(context_b.len() as u64);
                for (i, rec) in context_b.iter().enumerate() {
                    writeln!(f, "  B[{}]: {}", ctx_start + i as u64, rec)?;
                }
                writeln!(f, ">>B[{index}]: {record_b}")?;
                Ok(())
            }
            Self::LengthMismatch {
                matched,
                len_a,
                len_b,
            } => {
                write!(
                    f,
                    "Length mismatch: {matched} records match, A has {len_a}, B has {len_b}"
                )
            }
        }
    }
}

/// Compare two dlog files record-by-record.
///
/// Returns the first divergence with a context window of up to 5
/// preceding records from each file.
///
/// When `strict` is true, RIP is included in the comparison.
pub fn dlog_diff(a: &Path, b: &Path, strict: bool) -> io::Result<DiffResult> {
    let mut ra = DlogReader::open(a)?;
    let mut rb = DlogReader::open(b)?;

    const CONTEXT: usize = 5;
    let mut ring_a: Vec<DlogRecord> = Vec::with_capacity(CONTEXT);
    let mut ring_b: Vec<DlogRecord> = Vec::with_capacity(CONTEXT);
    let mut index: u64 = 0;

    loop {
        let rec_a = ra.next_record()?;
        let rec_b = rb.next_record()?;

        match (rec_a, rec_b) {
            (Some(a_rec), Some(b_rec)) => {
                if !a_rec.determinism_eq(&b_rec, strict) {
                    return Ok(DiffResult::Diverged {
                        index,
                        context_a: ring_a,
                        context_b: ring_b,
                        record_a: a_rec,
                        record_b: b_rec,
                    });
                }
                // Maintain rolling context window.
                if ring_a.len() >= CONTEXT {
                    ring_a.remove(0);
                    ring_b.remove(0);
                }
                ring_a.push(a_rec);
                ring_b.push(b_rec);
                index += 1;
            }
            (None, None) => {
                return Ok(DiffResult::Identical { records: index });
            }
            (some_a, some_b) => {
                // One ended before the other. Count remaining records
                // in the longer file.
                let len_a = if some_a.is_some() {
                    let mut count = index + 1;
                    while ra.next_record()?.is_some() {
                        count += 1;
                    }
                    count
                } else {
                    index
                };
                let len_b = if some_b.is_some() {
                    let mut count = index + 1;
                    while rb.next_record()?.is_some() {
                        count += 1;
                    }
                    count
                } else {
                    index
                };
                return Ok(DiffResult::LengthMismatch {
                    matched: index,
                    len_a,
                    len_b,
                });
            }
        }
    }
}

/// Dump records from a dlog file as human-readable text.
///
/// Prints records `from..from+count` to the provided writer.
pub fn dlog_dump(path: &Path, from: u64, count: u64, out: &mut dyn Write) -> io::Result<u64> {
    let mut reader = DlogReader::open(path)?;
    let mut printed = 0u64;

    // Skip to `from`.
    for _ in 0..from {
        if reader.next_record()?.is_none() {
            return Ok(0);
        }
    }

    for _ in 0..count {
        match reader.next_record()? {
            Some(rec) => {
                writeln!(out, "{rec}")?;
                printed += 1;
            }
            None => break,
        }
    }
    Ok(printed)
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn record_size_is_64_bytes() {
        assert_eq!(std::mem::size_of::<DlogRecord>(), 64);
    }

    #[test]
    fn round_trip_encode_decode() {
        let rec = DlogRecord::new(42, 1_000_000, 500, 0xDEAD_BEEF, DlogTag::IoOut, 1)
            .with_port(0x3f8)
            .with_data(&[0xAB, 0xCD]);

        let bytes = rec.to_bytes();
        let decoded = DlogRecord::from_bytes(bytes);

        assert_eq!(decoded.seq, 42);
        assert_eq!(decoded.virtual_tsc, 1_000_000);
        assert_eq!(decoded.exit_count, 500);
        assert_eq!(decoded.rip, 0xDEAD_BEEF);
        assert_eq!(decoded.tag, DlogTag::IoOut as u8);
        assert_eq!(decoded.vcpu, 1);
        assert_eq!(decoded.port_or_addr_lo, 0x3f8);
        assert_eq!(decoded.data[0], 0xAB);
        assert_eq!(decoded.data[1], 0xCD);
        assert_eq!(decoded.data[2], 0);
    }

    #[test]
    fn display_io_in() {
        let rec = DlogRecord::new(0, 5000, 10, 0x1234, DlogTag::IoIn, 0).with_port(0x3fd);
        let s = format!("{rec}");
        assert!(s.contains("IoIn"), "got: {s}");
        assert!(s.contains("0x03fd"), "got: {s}");
    }

    #[test]
    fn display_mmio_write() {
        let rec =
            DlogRecord::new(0, 5000, 10, 0x1234, DlogTag::MmioWrite, 0).with_mmio_addr(0xD000_1000);
        let s = format!("{rec}");
        assert!(s.contains("MmioWrite"), "got: {s}");
        assert!(s.contains("0xd0001000"), "got: {s}");
    }

    #[test]
    fn display_scheduler_switch() {
        let rec = DlogRecord::new(0, 5000, 10, 0, DlogTag::SchedulerSwitch, 1)
            .with_data(&[0]) // prev_vcpu=0
            .with_extra_u64(50); // quantum_left=50
        let s = format!("{rec}");
        assert!(s.contains("SchedSwitch"), "got: {s}");
        assert!(s.contains("prev_vcpu=0"), "got: {s}");
        assert!(s.contains("quantum_left=50"), "got: {s}");
    }

    #[test]
    fn tag_round_trip() {
        for val in [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 255,
        ] {
            let tag = DlogTag::from_u8(val).unwrap();
            assert_eq!(tag as u8, val);
        }
        assert!(DlogTag::from_u8(0).is_none());
        assert!(DlogTag::from_u8(20).is_none());
        assert!(DlogTag::from_u8(254).is_none());
    }

    #[test]
    fn determinism_eq_matches() {
        let a = DlogRecord::new(0, 100, 5, 0x1000, DlogTag::IoIn, 0).with_port(0x3f8);
        let b = DlogRecord::new(99, 100, 5, 0x1000, DlogTag::IoIn, 0).with_port(0x3f8);
        // seq differs but determinism_eq ignores seq.
        assert!(a.determinism_eq(&b, true));
    }

    #[test]
    fn determinism_eq_rip_strict() {
        let a = DlogRecord::new(0, 100, 5, 0x1000, DlogTag::IoIn, 0);
        let b = DlogRecord::new(0, 100, 5, 0x2000, DlogTag::IoIn, 0);
        assert!(a.determinism_eq(&b, false)); // non-strict: skip rip
        assert!(!a.determinism_eq(&b, true)); // strict: rip differs
    }

    #[test]
    fn writer_reader_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.dlog");

        // Write 100 records.
        {
            let mut w = DlogWriter::create(&path).unwrap();
            for i in 0..100u64 {
                let rec = DlogRecord::new(0, i * 1000, i, 0x4000 + i, DlogTag::IoOut, 0)
                    .with_port(0x3f8)
                    .with_data(&[i as u8]);
                w.emit(rec).unwrap();
            }
        } // Drop flushes.

        // Read back.
        let reader = DlogReader::open(&path).unwrap();
        let records: Vec<DlogRecord> = reader.map(|r| r.unwrap()).collect();
        assert_eq!(records.len(), 100);
        assert_eq!(records[0].seq, 0);
        assert_eq!(records[99].seq, 99);
        assert_eq!(records[50].virtual_tsc, 50_000);
        assert_eq!(records[50].data[0], 50);
    }

    #[test]
    fn diff_identical() {
        let dir = tempfile::tempdir().unwrap();
        let pa = dir.path().join("a.dlog");
        let pb = dir.path().join("b.dlog");

        let write_log = |path: &Path| {
            let mut w = DlogWriter::create(path).unwrap();
            for i in 0..50u64 {
                w.emit(DlogRecord::new(0, i, i, 0, DlogTag::IoIn, 0))
                    .unwrap();
            }
        };
        write_log(&pa);
        write_log(&pb);

        match dlog_diff(&pa, &pb, false).unwrap() {
            DiffResult::Identical { records } => assert_eq!(records, 50),
            other => panic!("expected Identical, got {other}"),
        }
    }

    #[test]
    fn diff_diverged() {
        let dir = tempfile::tempdir().unwrap();
        let pa = dir.path().join("a.dlog");
        let pb = dir.path().join("b.dlog");

        {
            let mut w = DlogWriter::create(&pa).unwrap();
            for i in 0..20u64 {
                w.emit(DlogRecord::new(0, i, i, 0, DlogTag::IoIn, 0))
                    .unwrap();
            }
        }
        {
            let mut w = DlogWriter::create(&pb).unwrap();
            for i in 0..20u64 {
                let tag = if i == 10 {
                    DlogTag::IoOut
                } else {
                    DlogTag::IoIn
                };
                w.emit(DlogRecord::new(0, i, i, 0, tag, 0)).unwrap();
            }
        }

        match dlog_diff(&pa, &pb, false).unwrap() {
            DiffResult::Diverged {
                index,
                context_a,
                record_a,
                record_b,
                ..
            } => {
                assert_eq!(index, 10);
                assert_eq!(context_a.len(), 5); // 5 records of context
                assert_eq!(record_a.tag, DlogTag::IoIn as u8);
                assert_eq!(record_b.tag, DlogTag::IoOut as u8);
            }
            other => panic!("expected Diverged, got {other}"),
        }
    }

    #[test]
    fn diff_length_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let pa = dir.path().join("a.dlog");
        let pb = dir.path().join("b.dlog");

        {
            let mut w = DlogWriter::create(&pa).unwrap();
            for i in 0..30u64 {
                w.emit(DlogRecord::new(0, i, i, 0, DlogTag::Hlt, 0))
                    .unwrap();
            }
        }
        {
            let mut w = DlogWriter::create(&pb).unwrap();
            for i in 0..20u64 {
                w.emit(DlogRecord::new(0, i, i, 0, DlogTag::Hlt, 0))
                    .unwrap();
            }
        }

        match dlog_diff(&pa, &pb, false).unwrap() {
            DiffResult::LengthMismatch {
                matched,
                len_a,
                len_b,
            } => {
                assert_eq!(matched, 20);
                assert_eq!(len_a, 30);
                assert_eq!(len_b, 20);
            }
            other => panic!("expected LengthMismatch, got {other}"),
        }
    }

    #[test]
    fn dump_formatting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.dlog");

        {
            let mut w = DlogWriter::create(&path).unwrap();
            for i in 0..10u64 {
                w.emit(DlogRecord::new(0, i * 100, i, 0, DlogTag::Hlt, 0))
                    .unwrap();
            }
        }

        let mut buf = Vec::new();
        let printed = dlog_dump(&path, 3, 4, &mut buf).unwrap();
        assert_eq!(printed, 4);
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("Hlt"));
    }

    #[test]
    fn dump_skip_past_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.dlog");

        {
            let mut w = DlogWriter::create(&path).unwrap();
            for i in 0..5u64 {
                w.emit(DlogRecord::new(0, i, i, 0, DlogTag::Hlt, 0))
                    .unwrap();
            }
        }

        let mut buf = Vec::new();
        let printed = dlog_dump(&path, 100, 10, &mut buf).unwrap();
        assert_eq!(printed, 0);
    }
}
