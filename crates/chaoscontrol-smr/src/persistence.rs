//! Deterministic per-node durable storage for the storage-recovery workload.
//!
//! The model owns byte framing, bounded decoding, and content identity only. It
//! performs no host I/O and grants no consensus or durability authority.
//!
//! r[impl chaoscontrol.smr_storage_recovery.persistent_nodes]

use serde::{Deserialize, Serialize};

use crate::{SmrError, SmrResult};

/// Schema bound into every encoded storage record.
pub const STORAGE_RECORD_SCHEMA: &str = "chaoscontrol.smr-storage-record.v1";
/// Encoded record version.
pub const STORAGE_RECORD_VERSION: u8 = 1;
/// Maximum bytes one node may stage or publish.
pub const MAX_STORAGE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum log entries one node may retain.
pub const MAX_STORAGE_LOG_ENTRIES: usize = 65_536;
/// Maximum command or snapshot bytes in one entry.
pub const MAX_STORAGE_MEMBER_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentLogEntry {
    pub index: u64,
    pub term: u64,
    pub command: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentSnapshot {
    pub last_index: u64,
    pub last_term: u64,
    pub state: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentNodeState {
    pub term: u64,
    pub voted_for: Option<u64>,
    pub snapshot: Option<PersistentSnapshot>,
    pub log: Vec<PersistentLogEntry>,
    pub application: Vec<u8>,
}

fn deny(class: &'static str, detail: impl Into<String>) -> SmrError {
    SmrError::new(class, detail)
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> SmrResult<u64> {
    let end = cursor
        .checked_add(8)
        .ok_or_else(|| deny("storage-record-overflow", "length offset overflowed"))?;
    let slice = bytes.get(*cursor..end).ok_or_else(|| {
        deny(
            "storage-record-truncated",
            "record ended inside a length field",
        )
    })?;
    let mut value = [0_u8; 8];
    value.copy_from_slice(slice);
    *cursor = end;
    Ok(u64::from_le_bytes(value))
}

fn read_bytes(bytes: &[u8], cursor: &mut usize, maximum: usize) -> SmrResult<Vec<u8>> {
    let length = read_u64(bytes, cursor)?;
    let length = usize::try_from(length).map_err(|_| {
        deny(
            "storage-record-overflow",
            "member length does not fit the host",
        )
    })?;
    if length > maximum {
        return Err(deny(
            "storage-record-over-bound",
            format!("member of {length} bytes exceeds {maximum}"),
        ));
    }
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| deny("storage-record-overflow", "member offset overflowed"))?;
    let slice = bytes
        .get(*cursor..end)
        .ok_or_else(|| deny("storage-record-truncated", "record ended inside a member"))?;
    *cursor = end;
    Ok(slice.to_vec())
}

impl PersistentNodeState {
    fn validate(&self) -> SmrResult<()> {
        if self.log.len() > MAX_STORAGE_LOG_ENTRIES {
            return Err(deny(
                "storage-record-over-bound",
                format!(
                    "log of {} entries exceeds {MAX_STORAGE_LOG_ENTRIES}",
                    self.log.len()
                ),
            ));
        }
        if let Some(snapshot) = &self.snapshot {
            if snapshot.state.len() > MAX_STORAGE_MEMBER_BYTES {
                return Err(deny(
                    "storage-record-over-bound",
                    "snapshot bytes exceed the member bound",
                ));
            }
            if snapshot.last_index >= self.log.first().map_or(u64::MAX, |entry| entry.index) {
                return Err(deny(
                    "storage-record-inconsistent",
                    "snapshot boundary is not below the retained log suffix",
                ));
            }
        }
        let mut previous = None;
        for entry in &self.log {
            if entry.command.len() > MAX_STORAGE_MEMBER_BYTES {
                return Err(deny(
                    "storage-record-over-bound",
                    "command bytes exceed the member bound",
                ));
            }
            if entry.term == 0 {
                return Err(deny(
                    "storage-record-inconsistent",
                    "log entry carries term zero",
                ));
            }
            if let Some(previous) = previous {
                if entry.index != previous + 1 {
                    return Err(deny(
                        "storage-record-inconsistent",
                        "log indices are not a contiguous suffix",
                    ));
                }
            }
            previous = Some(entry.index);
        }
        if self.application.len() > MAX_STORAGE_MEMBER_BYTES {
            return Err(deny(
                "storage-record-over-bound",
                "application bytes exceed the member bound",
            ));
        }
        Ok(())
    }

    /// Encode one bounded canonical record.
    pub fn encode(&self) -> SmrResult<Vec<u8>> {
        self.validate()?;
        let mut out = Vec::new();
        out.extend_from_slice(STORAGE_RECORD_SCHEMA.as_bytes());
        out.push(0);
        out.push(STORAGE_RECORD_VERSION);
        push_u64(&mut out, self.term);
        match self.voted_for {
            Some(vote) => {
                out.push(1);
                push_u64(&mut out, vote);
            }
            None => out.push(0),
        }
        match &self.snapshot {
            Some(snapshot) => {
                out.push(1);
                push_u64(&mut out, snapshot.last_index);
                push_u64(&mut out, snapshot.last_term);
                push_u64(
                    &mut out,
                    u64::try_from(snapshot.state.len()).expect("bounded member"),
                );
                out.extend_from_slice(&snapshot.state);
            }
            None => out.push(0),
        }
        push_u64(
            &mut out,
            u64::try_from(self.log.len()).expect("bounded log"),
        );
        for entry in &self.log {
            push_u64(&mut out, entry.index);
            push_u64(&mut out, entry.term);
            push_u64(
                &mut out,
                u64::try_from(entry.command.len()).expect("bounded member"),
            );
            out.extend_from_slice(&entry.command);
        }
        push_u64(
            &mut out,
            u64::try_from(self.application.len()).expect("bounded member"),
        );
        out.extend_from_slice(&self.application);
        if out.len() > MAX_STORAGE_BYTES {
            return Err(deny(
                "storage-record-over-bound",
                format!("record of {} bytes exceeds {MAX_STORAGE_BYTES}", out.len()),
            ));
        }
        Ok(out)
    }

    /// Decode one bounded canonical record.
    pub fn decode(bytes: &[u8]) -> SmrResult<Self> {
        let schema = STORAGE_RECORD_SCHEMA.as_bytes();
        let schema_end = schema.len();
        if bytes.get(..schema_end) != Some(schema) || bytes.get(schema_end) != Some(&0) {
            return Err(deny(
                "storage-record-schema",
                "record does not begin with the current schema",
            ));
        }
        let mut cursor = schema_end + 1;
        if bytes.get(cursor) != Some(&STORAGE_RECORD_VERSION) {
            return Err(deny(
                "storage-record-version",
                "record version is not supported",
            ));
        }
        cursor += 1;
        let term = read_u64(bytes, &mut cursor)?;
        let voted_for = match bytes.get(cursor) {
            Some(0) => {
                cursor += 1;
                None
            }
            Some(1) => {
                cursor += 1;
                Some(read_u64(bytes, &mut cursor)?)
            }
            _ => return Err(deny("storage-record-framing", "vote marker is not 0 or 1")),
        };
        let snapshot = match bytes.get(cursor) {
            Some(0) => {
                cursor += 1;
                None
            }
            Some(1) => {
                cursor += 1;
                let last_index = read_u64(bytes, &mut cursor)?;
                let last_term = read_u64(bytes, &mut cursor)?;
                let state = read_bytes(bytes, &mut cursor, MAX_STORAGE_MEMBER_BYTES)?;
                Some(PersistentSnapshot {
                    last_index,
                    last_term,
                    state,
                })
            }
            _ => {
                return Err(deny(
                    "storage-record-framing",
                    "snapshot marker is not 0 or 1",
                ))
            }
        };
        let entry_count = read_u64(bytes, &mut cursor)?;
        let entry_count = usize::try_from(entry_count).map_err(|_| {
            deny(
                "storage-record-overflow",
                "entry count does not fit the host",
            )
        })?;
        if entry_count > MAX_STORAGE_LOG_ENTRIES {
            return Err(deny(
                "storage-record-over-bound",
                format!("log of {entry_count} entries exceeds {MAX_STORAGE_LOG_ENTRIES}"),
            ));
        }
        let mut log = Vec::with_capacity(entry_count.min(1024));
        for _ in 0..entry_count {
            let index = read_u64(bytes, &mut cursor)?;
            let entry_term = read_u64(bytes, &mut cursor)?;
            let command = read_bytes(bytes, &mut cursor, MAX_STORAGE_MEMBER_BYTES)?;
            log.push(PersistentLogEntry {
                index,
                term: entry_term,
                command,
            });
        }
        let application = read_bytes(bytes, &mut cursor, MAX_STORAGE_MEMBER_BYTES)?;
        if cursor != bytes.len() {
            return Err(deny(
                "storage-record-trailing",
                "record carries trailing bytes",
            ));
        }
        let state = Self {
            term,
            voted_for,
            snapshot,
            log,
            application,
        };
        state.validate()?;
        Ok(state)
    }
}

/// Outcome of one staged write and flush.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePublication {
    pub identity: String,
    pub bytes: usize,
}

/// One node's deterministic, bounded disk image.
#[derive(Debug, Clone, Default)]
pub struct SmrDisk {
    staged: Vec<u8>,
    published: Vec<u8>,
    capacity_bytes: usize,
    is_flush_failed: bool,
    is_write_failed: bool,
}

impl SmrDisk {
    /// Create an empty disk with an explicit byte capacity.
    pub fn new(capacity_bytes: usize) -> SmrResult<Self> {
        if capacity_bytes == 0 || capacity_bytes > MAX_STORAGE_BYTES {
            return Err(deny(
                "storage-capacity",
                format!("capacity {capacity_bytes} is outside 1..={MAX_STORAGE_BYTES}"),
            ));
        }
        Ok(Self {
            staged: Vec::new(),
            published: Vec::new(),
            capacity_bytes,
            is_flush_failed: false,
            is_write_failed: false,
        })
    }

    /// Arm one immediate write failure.
    pub fn fail_next_write(&mut self) {
        self.is_write_failed = true;
    }

    /// Arm one flush failure that leaves staged bytes unpublished.
    pub fn fail_next_flush(&mut self) {
        self.is_flush_failed = true;
    }

    /// Identity of the currently published bytes, or `None` when empty.
    pub fn published_identity(&self) -> Option<String> {
        if self.published.is_empty() {
            return None;
        }
        Some(format!("blake3:{}", blake3::hash(&self.published).to_hex()))
    }

    /// Bytes currently published on this disk.
    pub fn published_bytes(&self) -> &[u8] {
        &self.published
    }

    /// Stage and flush one bounded state in a single attempt.
    pub fn write(&mut self, state: &PersistentNodeState) -> SmrResult<StoragePublication> {
        self.stage(state)?;
        self.flush()
    }

    /// Stage one bounded state without publishing it.
    pub fn stage(&mut self, state: &PersistentNodeState) -> SmrResult<()> {
        if self.is_write_failed {
            self.is_write_failed = false;
            return Err(deny(
                "storage-write-failed",
                "the disk rejected the staged write",
            ));
        }
        let encoded = state.encode()?;
        if encoded.len() > self.capacity_bytes {
            return Err(deny(
                "storage-capacity",
                format!(
                    "record of {} bytes exceeds capacity {}",
                    encoded.len(),
                    self.capacity_bytes
                ),
            ));
        }
        self.staged = encoded;
        Ok(())
    }

    /// Publish the staged record.
    pub fn flush(&mut self) -> SmrResult<StoragePublication> {
        if self.staged.is_empty() {
            return Err(deny(
                "storage-flush-empty",
                "no staged record is available to flush",
            ));
        }
        if self.is_flush_failed {
            self.is_flush_failed = false;
            return Err(deny(
                "storage-flush-failed",
                "the flush failed; staged bytes remain unpublished",
            ));
        }
        self.published.clone_from(&self.staged);
        Ok(StoragePublication {
            identity: format!("blake3:{}", blake3::hash(&self.published).to_hex()),
            bytes: self.published.len(),
        })
    }

    /// Load the published state, or `None` when nothing was ever published.
    pub fn load(&self) -> SmrResult<Option<PersistentNodeState>> {
        if self.published.is_empty() {
            return Ok(None);
        }
        PersistentNodeState::decode(&self.published).map(Some)
    }

    /// Corrupt published bytes for a negative recovery observation.
    pub fn corrupt_published(&mut self, offset: usize, replacement: u8) -> SmrResult<()> {
        let byte = self.published.get_mut(offset).ok_or_else(|| {
            deny(
                "storage-range",
                "corruption offset is outside the published record",
            )
        })?;
        *byte = replacement;
        Ok(())
    }

    /// Truncate the published record for a negative recovery observation.
    pub fn truncate_published(&mut self, length: usize) -> SmrResult<()> {
        if length > self.published.len() {
            return Err(deny(
                "storage-range",
                "truncation length exceeds the published record",
            ));
        }
        self.published.truncate(length);
        Ok(())
    }
}

/// Independent disk images for one declared Raft participant set.
#[derive(Debug, Clone, Default)]
pub struct PersistentNodeCohort {
    disks: Vec<SmrDisk>,
}

impl PersistentNodeCohort {
    /// Create one independent disk per declared node.
    pub fn new(node_count: usize, capacity_bytes: usize) -> SmrResult<Self> {
        if node_count == 0 || node_count > 64 {
            return Err(deny("storage-cohort", "node count is outside 1..=64"));
        }
        let mut disks = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            disks.push(SmrDisk::new(capacity_bytes)?);
        }
        Ok(Self { disks })
    }

    /// Number of independent disks.
    pub fn len(&self) -> usize {
        self.disks.len()
    }

    /// Whether the cohort has no disks.
    pub fn is_empty(&self) -> bool {
        self.disks.is_empty()
    }

    /// Borrow one node's disk.
    pub fn node(&self, index: usize) -> SmrResult<&SmrDisk> {
        self.disks.get(index).ok_or_else(|| {
            deny(
                "storage-cohort",
                format!("node {index} is outside the cohort"),
            )
        })
    }

    /// Borrow one node's disk mutably.
    pub fn node_mut(&mut self, index: usize) -> SmrResult<&mut SmrDisk> {
        self.disks.get_mut(index).ok_or_else(|| {
            deny(
                "storage-cohort",
                format!("node {index} is outside the cohort"),
            )
        })
    }

    /// Published identities in participant order.
    pub fn identities(&self) -> Vec<Option<String>> {
        self.disks.iter().map(SmrDisk::published_identity).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(log: &[(u64, u64, &[u8])]) -> PersistentNodeState {
        PersistentNodeState {
            term: 7,
            voted_for: Some(3),
            snapshot: None,
            log: log
                .iter()
                .map(|(index, term, command)| PersistentLogEntry {
                    index: *index,
                    term: *term,
                    command: command.to_vec(),
                })
                .collect(),
            application: b"applied".to_vec(),
        }
    }

    #[test]
    fn write_and_load_round_trip_every_member() {
        let mut disk = SmrDisk::new(4096).expect("disk");
        let expected = state(&[(1, 2, b"set a"), (2, 2, b"set b")]);
        let publication = disk.write(&expected).expect("publication");
        assert!(publication.identity.starts_with("blake3:"));
        assert_eq!(disk.load().expect("load"), Some(expected));
        assert_eq!(disk.published_identity(), Some(publication.identity));
    }

    #[test]
    fn crash_before_flush_preserves_only_published_state() {
        let mut disk = SmrDisk::new(4096).expect("disk");
        disk.write(&state(&[(1, 1, b"durable")]))
            .expect("first write");
        let published = disk.load().expect("load").expect("published");
        disk.fail_next_flush();
        let second = state(&[(1, 1, b"staged")]);
        disk.stage(&second).expect("stage");
        assert!(disk.flush().is_err());
        assert_eq!(disk.load().expect("load"), Some(published));
    }

    #[test]
    fn write_failure_and_capacity_denial_leave_published_state_unchanged() {
        let mut disk = SmrDisk::new(128).expect("disk");
        disk.write(&state(&[])).expect("small record");
        let published = disk.published_identity();
        disk.fail_next_write();
        assert!(disk.write(&state(&[])).is_err());
        assert_eq!(disk.published_identity(), published);
        let oversized = state(&[(1, 1, &[0_u8; 256])]);
        assert_eq!(
            disk.write(&oversized).expect_err("capacity").class,
            "storage-capacity"
        );
        assert_eq!(disk.published_identity(), published);
    }

    #[test]
    fn corrupted_and_truncated_records_are_denied() {
        let mut disk = SmrDisk::new(4096).expect("disk");
        disk.write(&state(&[(1, 1, b"value")])).expect("write");
        disk.corrupt_published(0, b'X').expect("corrupt");
        assert_eq!(
            disk.load().expect_err("schema").class,
            "storage-record-schema"
        );

        let mut disk = SmrDisk::new(4096).expect("disk");
        disk.write(&state(&[(1, 1, b"value")])).expect("write");
        let length = disk.published_bytes().len();
        disk.truncate_published(length - 1).expect("truncate");
        assert_eq!(
            disk.load().expect_err("truncated").class,
            "storage-record-truncated"
        );
    }

    #[test]
    fn inconsistent_logs_and_snapshots_are_denied() {
        let mut inner = state(&[(2, 1, b"a"), (4, 1, b"b")]);
        assert_eq!(
            inner.clone().encode().expect_err("gap").class,
            "storage-record-inconsistent"
        );
        inner.log = vec![PersistentLogEntry {
            index: 3,
            term: 1,
            command: b"a".to_vec(),
        }];
        inner.snapshot = Some(PersistentSnapshot {
            last_index: 3,
            last_term: 1,
            state: b"snapshot".to_vec(),
        });
        assert_eq!(
            inner.encode().expect_err("snapshot overlap").class,
            "storage-record-inconsistent"
        );
    }

    #[test]
    fn cohort_disks_are_independent() {
        let mut cohort = PersistentNodeCohort::new(3, 4096).expect("cohort");
        cohort
            .node_mut(0)
            .expect("node 0")
            .write(&state(&[]))
            .expect("node 0 write");
        assert!(cohort
            .node(1)
            .expect("node 1")
            .load()
            .expect("load")
            .is_none());
        let identities = cohort.identities();
        assert!(identities[0].is_some());
        assert!(identities[1..].iter().all(Option::is_none));
    }
}
