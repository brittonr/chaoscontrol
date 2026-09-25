use super::format::layout::{classify_epoch_entry, classify_root_entry, EpochEntry, RootEntry};
use super::format::verify::{verify_epoch, verify_epoch_numbers};
use super::*;
use ciborium::value::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tempfile::{NamedTempFile, TempDir};

const TEST_KIND: RecordKind = RecordKind::new(7);
const OTHER_KIND: RecordKind = RecordKind::new(0xBEEF);
const INJECTED_ERROR: &str = "injected publication fault";

fn records(tag: u8, count: u8) -> Vec<JournalRecord> {
    (0..count)
        .map(|index| JournalRecord::new(TEST_KIND, vec![tag, index, 0xA5]))
        .collect()
}

fn single(tag: u8) -> Vec<JournalRecord> {
    vec![JournalRecord::new(OTHER_KIND, vec![tag; 5])]
}

fn epoch_path(root: &Path, epoch: u32) -> PathBuf {
    root.join(JOURNAL_DIRECTORY_NAME)
        .join(epoch_directory_name(epoch))
}

fn segment_path(root: &Path, epoch: u32, seq: u64) -> PathBuf {
    epoch_path(root, epoch).join(segment_file_name(seq))
}

fn stage_files(directory: &Path) -> Vec<PathBuf> {
    let mut stages: Vec<PathBuf> = fs::read_dir(directory)
        .expect("list directory")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(STAGE_FILE_PREFIX))
        })
        .collect();
    stages.sort();
    stages
}

fn open(root: &Path) -> (CampaignJournal, RecoveredJournal) {
    CampaignJournal::open(root).expect("open journal")
}

fn open_faulty(root: &Path) -> (CampaignJournal<FaultInjectingIo>, RecoveredJournal) {
    CampaignJournal::open_with_io(root, FaultInjectingIo::default()).expect("open journal")
}

fn corruption(root: &Path) -> JournalCorruption {
    match CampaignJournal::open(root) {
        Err(JournalError::Corruption { corruption }) => corruption,
        other => panic!("expected corruption, got {other:?}"),
    }
}

/// Journal with epoch 0 holding three committed segments.
fn journal_with_three_segments(root: &Path) -> Vec<CommittedSegment> {
    let (mut journal, _) = open(root);
    assert_eq!(journal.begin_epoch().expect("begin epoch"), 0);
    (0..3)
        .map(|tag| journal.append(&records(tag, 2)).expect("append"))
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaultTiming {
    /// Fail without performing the operation.
    Before,
    /// Perform the operation, then report failure.
    After,
}

#[derive(Clone, Copy, Debug)]
struct Fault {
    phase: PublicationPhase,
    timing: FaultTiming,
    kind: io::ErrorKind,
}

/// One-shot fault injector over the production filesystem effects.
#[derive(Debug, Default)]
struct FaultInjectingIo {
    inner: SystemPublicationIo,
    fault: Option<Fault>,
}

impl FaultInjectingIo {
    fn arm(&mut self, phase: PublicationPhase, timing: FaultTiming, kind: io::ErrorKind) {
        self.fault = Some(Fault {
            phase,
            timing,
            kind,
        });
    }

    fn check(&mut self, phase: PublicationPhase, timing: FaultTiming) -> io::Result<()> {
        match self.fault {
            Some(fault) if fault.phase == phase && fault.timing == timing => {
                self.fault = None;
                Err(io::Error::new(fault.kind, INJECTED_ERROR))
            }
            _ => Ok(()),
        }
    }

    fn run<T>(
        &mut self,
        phase: PublicationPhase,
        operation: impl FnOnce(&mut SystemPublicationIo) -> io::Result<T>,
    ) -> io::Result<T> {
        self.check(phase, FaultTiming::Before)?;
        let value = operation(&mut self.inner)?;
        self.check(phase, FaultTiming::After)?;
        Ok(value)
    }
}

impl PublicationIo for FaultInjectingIo {
    type Stage = NamedTempFile;

    fn create_stage(&mut self, directory: &Path) -> io::Result<Self::Stage> {
        self.run(PublicationPhase::CreateStage, |io| {
            io.create_stage(directory)
        })
    }

    fn write_payload(&mut self, stage: &mut Self::Stage, bytes: &[u8]) -> io::Result<()> {
        self.run(PublicationPhase::WritePayload, |io| {
            io.write_payload(stage, bytes)
        })
    }

    fn sync_payload(&mut self, stage: &mut Self::Stage) -> io::Result<()> {
        self.run(PublicationPhase::SyncPayload, |io| io.sync_payload(stage))
    }

    fn persist_no_replace(&mut self, stage: Self::Stage, destination: &Path) -> io::Result<()> {
        self.run(PublicationPhase::PersistNoReplace, |io| {
            io.persist_no_replace(stage, destination)
        })
    }

    fn create_directory(&mut self, path: &Path) -> io::Result<()> {
        self.run(PublicationPhase::CreateDirectory, |io| {
            io.create_directory(path)
        })
    }

    fn sync_directory(&mut self, path: &Path) -> io::Result<()> {
        self.run(PublicationPhase::SyncDirectory, |io| {
            io.sync_directory(path)
        })
    }
}

// ---------------------------------------------------------------------------
// Pure segment codec
// ---------------------------------------------------------------------------

fn encoded(seq: u64, previous: SegmentDigest, tag: u8) -> EncodedSegment {
    encode_segment(0, seq, previous, &records(tag, 3)).expect("encode")
}

fn decode_value(bytes: &[u8]) -> Value {
    ciborium::de::from_reader(bytes).expect("decode value")
}

fn encode_value(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes).expect("encode value");
    bytes
}

fn map_entries(value: &mut Value) -> &mut Vec<(Value, Value)> {
    match value {
        Value::Map(entries) => entries,
        other => panic!("expected map, got {other:?}"),
    }
}

/// Rewrite one header field of valid segment bytes.
fn with_header_field(bytes: &[u8], field: &str, replacement: Value) -> Vec<u8> {
    let mut value = decode_value(bytes);
    let header = &mut map_entries(&mut value)[0].1;
    let slot = map_entries(header)
        .iter_mut()
        .find(|(key, _)| key.as_text() == Some(field))
        .expect("header field");
    slot.1 = replacement;
    encode_value(&value)
}

#[test]
fn segment_round_trip_preserves_header_records_and_digest() {
    let records = records(9, 4);
    let previous = SegmentDigest::from_bytes([3; DIGEST_BYTES]);
    let segment = encode_segment(12, 34, previous, &records).expect("encode");
    let decoded = decode_segment(segment.bytes()).expect("decode");
    assert_eq!(
        *decoded.header(),
        SegmentHeader {
            epoch: 12,
            seq: 34,
            previous_digest: previous,
            record_count: 4,
        }
    );
    assert_eq!(decoded.records(), records.as_slice());
    assert_eq!(decoded.digest(), segment.digest());
    assert_eq!(
        segment.digest(),
        SegmentDigest::of_segment_bytes(segment.bytes())
    );
    let again = encode_segment(12, 34, previous, &records).expect("encode");
    assert_eq!(again.bytes(), segment.bytes(), "encoding is deterministic");
}

#[test]
fn segment_digest_binds_every_header_field_and_record() {
    let base = encode_segment(1, 2, SegmentDigest::ZERO, &records(1, 2)).expect("encode");
    let variants = [
        encode_segment(2, 2, SegmentDigest::ZERO, &records(1, 2)),
        encode_segment(1, 3, SegmentDigest::ZERO, &records(1, 2)),
        encode_segment(1, 2, SegmentDigest::from_bytes([1; 32]), &records(1, 2)),
        encode_segment(1, 2, SegmentDigest::ZERO, &records(1, 3)),
        encode_segment(1, 2, SegmentDigest::ZERO, &records(2, 2)),
        encode_segment(1, 2, SegmentDigest::ZERO, &single(1)),
    ];
    for variant in variants {
        assert_ne!(variant.expect("encode").digest(), base.digest());
    }
}

#[test]
fn encode_accepts_exact_bounds() {
    let largest_payload = vec![JournalRecord::new(
        TEST_KIND,
        vec![0; MAX_RECORD_PAYLOAD_BYTES as usize],
    )];
    let segment = encode_segment(
        MAX_EPOCH,
        MAX_SEGMENT_SEQ,
        SegmentDigest::ZERO,
        &largest_payload,
    )
    .expect("largest payload at largest position");
    assert_eq!(
        decode_segment(segment.bytes()).expect("decode").records(),
        largest_payload.as_slice()
    );
    let most_records: Vec<JournalRecord> = (0..MAX_RECORDS_PER_SEGMENT)
        .map(|_| JournalRecord::new(TEST_KIND, Vec::new()))
        .collect();
    let segment = encode_segment(0, 0, SegmentDigest::ZERO, &most_records).expect("most records");
    assert_eq!(
        decode_segment(segment.bytes())
            .expect("decode")
            .header()
            .record_count,
        MAX_RECORDS_PER_SEGMENT
    );
}

#[test]
fn encode_refuses_out_of_bound_segments() {
    let one = single(1);
    let too_many: Vec<JournalRecord> = (0..=MAX_RECORDS_PER_SEGMENT)
        .map(|_| JournalRecord::new(TEST_KIND, Vec::new()))
        .collect();
    let oversized_payload = vec![JournalRecord::new(
        TEST_KIND,
        vec![0; MAX_RECORD_PAYLOAD_BYTES as usize + 1],
    )];
    let payloads_per_full_segment = MAX_SEGMENT_BYTES / u64::from(MAX_RECORD_PAYLOAD_BYTES);
    let oversized_segment: Vec<JournalRecord> = (0..=payloads_per_full_segment)
        .map(|_| JournalRecord::new(TEST_KIND, vec![0; MAX_RECORD_PAYLOAD_BYTES as usize]))
        .collect();
    let cases: [(u32, u64, &[JournalRecord], SegmentBound); 6] = [
        (MAX_EPOCH + 1, 0, &one, SegmentBound::Epoch),
        (0, MAX_SEGMENT_SEQ + 1, &one, SegmentBound::Sequence),
        (0, 0, &[], SegmentBound::RecordCount),
        (0, 0, &too_many, SegmentBound::RecordCount),
        (0, 0, &oversized_payload, SegmentBound::RecordPayload),
        (0, 0, &oversized_segment, SegmentBound::SegmentBytes),
    ];
    for (epoch, seq, records, bound) in cases {
        assert_eq!(
            encode_segment(epoch, seq, SegmentDigest::ZERO, records),
            Err(SegmentEncodeError::BoundExceeded { bound }),
            "{bound:?}"
        );
    }
}

#[test]
fn decode_rejects_every_truncated_prefix() {
    let segment = encoded(0, SegmentDigest::ZERO, 1);
    for length in 0..segment.bytes().len() {
        assert_eq!(
            decode_segment(&segment.bytes()[..length]),
            Err(SegmentDecodeError::Truncated),
            "prefix of {length} bytes"
        );
    }
}

#[test]
fn decode_rejects_garbage_and_wrong_shapes() {
    assert_eq!(
        decode_segment(&[0xFF; 16]),
        Err(SegmentDecodeError::Garbage)
    );
    assert_eq!(
        decode_segment(&encode_value(&Value::Integer(7.into()))),
        Err(SegmentDecodeError::Malformed { field: "segment" })
    );
    assert_eq!(
        decode_segment(&encode_value(&Value::Map(Vec::new()))),
        Err(SegmentDecodeError::Malformed { field: "segment" })
    );
    let oversized = vec![0; MAX_SEGMENT_BYTES as usize + 1];
    assert_eq!(
        decode_segment(&oversized),
        Err(SegmentDecodeError::BoundExceeded {
            bound: SegmentBound::SegmentBytes
        })
    );
}

#[test]
fn decode_rejects_trailing_bytes_as_non_canonical() {
    let mut bytes = encoded(0, SegmentDigest::ZERO, 1).bytes().to_vec();
    bytes.push(0);
    assert_eq!(
        decode_segment(&bytes),
        Err(SegmentDecodeError::NonCanonical)
    );
}

#[test]
fn decode_rejects_header_tampering() {
    let bytes = encoded(0, SegmentDigest::ZERO, 1).bytes().to_vec();
    let cases = [
        (
            "schema",
            Value::Text("chaoscontrol.campaign-journal-segment.v2".into()),
            SegmentDecodeError::SchemaMismatch,
        ),
        (
            "schema",
            Value::Integer(1.into()),
            SegmentDecodeError::Malformed { field: "schema" },
        ),
        (
            "epoch",
            Value::Integer((u64::from(MAX_EPOCH) + 1).into()),
            SegmentDecodeError::BoundExceeded {
                bound: SegmentBound::Epoch,
            },
        ),
        (
            "seq",
            Value::Integer((MAX_SEGMENT_SEQ + 1).into()),
            SegmentDecodeError::BoundExceeded {
                bound: SegmentBound::Sequence,
            },
        ),
        (
            "record_count",
            Value::Integer(0.into()),
            SegmentDecodeError::BoundExceeded {
                bound: SegmentBound::RecordCount,
            },
        ),
        (
            "record_count",
            Value::Integer(2.into()),
            SegmentDecodeError::RecordCountMismatch {
                declared: 2,
                actual: 3,
            },
        ),
        (
            "previous_digest",
            Value::Bytes(vec![0; DIGEST_BYTES - 1]),
            SegmentDecodeError::Malformed {
                field: "previous_digest",
            },
        ),
        (
            "seq",
            Value::Integer(1.into()),
            SegmentDecodeError::BodyDigestMismatch,
        ),
    ];
    for (field, replacement, expected) in cases {
        assert_eq!(
            decode_segment(&with_header_field(&bytes, field, replacement)),
            Err(expected.clone()),
            "{field}: {expected:?}"
        );
    }
}

#[test]
fn decode_rejects_unknown_and_reordered_fields() {
    let bytes = encoded(0, SegmentDigest::ZERO, 1).bytes().to_vec();
    let mut extra = decode_value(&bytes);
    map_entries(&mut extra).push((Value::Text("extra".into()), Value::Null));
    assert_eq!(
        decode_segment(&encode_value(&extra)),
        Err(SegmentDecodeError::Malformed { field: "segment" })
    );
    let mut reordered = decode_value(&bytes);
    map_entries(&mut reordered).swap(0, 1);
    assert_eq!(
        decode_segment(&encode_value(&reordered)),
        Err(SegmentDecodeError::Malformed { field: "segment" })
    );
}

#[test]
fn decode_rejects_any_payload_bit_flip() {
    let segment = encoded(0, SegmentDigest::ZERO, 0x11);
    let mut bytes = segment.bytes().to_vec();
    let payload_index = bytes
        .windows(3)
        .position(|window| window == [0x11, 0x01, 0xA5])
        .expect("second payload bytes");
    bytes[payload_index] ^= 0x01;
    assert_eq!(
        decode_segment(&bytes),
        Err(SegmentDecodeError::BodyDigestMismatch)
    );
}

#[test]
fn decode_rejects_non_minimal_integer_encoding() {
    let bytes = encoded(0, SegmentDigest::ZERO, 1).bytes().to_vec();
    // `seq: 0` is encoded as the one-byte integer 0x00 after the key "seq".
    let key = [0x63, b's', b'e', b'q'];
    let index = bytes
        .windows(key.len())
        .position(|window| window == key)
        .expect("seq key")
        + key.len();
    assert_eq!(bytes[index], 0x00);
    let mut widened = bytes[..index].to_vec();
    widened.extend_from_slice(&[0x18, 0x00]);
    widened.extend_from_slice(&bytes[index + 1..]);
    assert_eq!(
        decode_segment(&widened),
        Err(SegmentDecodeError::NonCanonical)
    );
}

// ---------------------------------------------------------------------------
// Pure layout, verification, and classification
// ---------------------------------------------------------------------------

#[test]
fn canonical_names_classify_as_journal_entries() {
    assert_eq!(epoch_directory_name(7), "epoch-0007");
    assert_eq!(segment_file_name(42), "seg-00000042.cbor");
    assert_eq!(
        classify_root_entry(epoch_directory_name(MAX_EPOCH).as_bytes()),
        RootEntry::Epoch(MAX_EPOCH)
    );
    assert_eq!(
        classify_epoch_entry(segment_file_name(MAX_SEGMENT_SEQ).as_bytes()),
        EpochEntry::Segment(MAX_SEGMENT_SEQ)
    );
    assert_eq!(classify_root_entry(b"LOCK"), RootEntry::Lock);
    assert_eq!(
        classify_epoch_entry(b".seg-stage-abc.tmp"),
        EpochEntry::Stage
    );
    for unrelated in [
        &b"notes.txt"[..],
        b"lock",
        b"Epoch-0000",
        b"checkpoint.json",
    ] {
        assert_eq!(classify_root_entry(unrelated), RootEntry::Unrelated);
    }
    for unrelated in [&b"README"[..], b"segment", b".hidden"] {
        assert_eq!(classify_epoch_entry(unrelated), EpochEntry::Unrelated);
    }
}

#[test]
fn reserved_non_canonical_names_are_aliases_or_malformed() {
    assert_eq!(classify_root_entry(b"epoch-1"), RootEntry::EpochAlias(1));
    assert_eq!(
        classify_root_entry(b"epoch-00001"),
        RootEntry::EpochAlias(1)
    );
    for malformed in [
        &b"epoch-"[..],
        b"epoch-abcd",
        b"epoch-00a1",
        b"epoch-0001.bak",
    ] {
        assert_eq!(classify_root_entry(malformed), RootEntry::MalformedEpoch);
    }
    assert_eq!(
        classify_root_entry(b"epoch-99999999999999999999999"),
        RootEntry::MalformedEpoch
    );
    assert_eq!(
        classify_epoch_entry(b"seg-1.cbor"),
        EpochEntry::SegmentAlias(1)
    );
    for malformed in [
        &b"seg-.cbor"[..],
        b"seg-0000000x.cbor",
        b"seg-00000001.json",
        b"seg-00000001",
        b"seg-00000001.cbor.bak",
    ] {
        assert_eq!(
            classify_epoch_entry(malformed),
            EpochEntry::MalformedSegment
        );
    }
}

fn chain(count: u64) -> (Vec<EncodedSegment>, BTreeMap<u64, Vec<u8>>) {
    let mut previous = SegmentDigest::ZERO;
    let mut segments = Vec::new();
    for seq in 0..count {
        let segment = encoded(seq, previous, seq as u8);
        previous = segment.digest();
        segments.push(segment);
    }
    let map = segments
        .iter()
        .map(|segment| (segment.header().seq, segment.bytes().to_vec()))
        .collect();
    (segments, map)
}

#[test]
fn verify_epoch_accepts_contiguous_chain() {
    let (segments, map) = chain(3);
    let epoch = verify_epoch(0, map).expect("verified");
    assert_eq!(epoch.next_seq(), 3);
    assert_eq!(epoch.final_digest(), Some(segments[2].digest()));
    assert_eq!(epoch.records().count(), 9);
    let empty = verify_epoch(4, BTreeMap::new()).expect("empty epoch");
    assert_eq!(empty.final_digest(), None);
    assert_eq!(empty.next_seq(), 0);
}

#[test]
fn verify_epoch_rejects_gap_chain_and_position_mismatches() {
    let (segments, mut gapped) = chain(3);
    gapped.remove(&1);
    assert_eq!(
        verify_epoch(0, gapped),
        Err(JournalCorruption::SequenceGap {
            epoch: 0,
            expected: 1,
            found: 2
        })
    );
    let (_, mut missing_first) = chain(2);
    missing_first.remove(&0);
    assert_eq!(
        verify_epoch(0, missing_first),
        Err(JournalCorruption::SequenceGap {
            epoch: 0,
            expected: 0,
            found: 1
        })
    );
    let (_, mut broken) = chain(3);
    broken.insert(2, encoded(2, SegmentDigest::ZERO, 2).bytes().to_vec());
    assert_eq!(
        verify_epoch(0, broken),
        Err(JournalCorruption::BrokenChain { epoch: 0, seq: 2 })
    );
    let (_, map) = chain(1);
    assert_eq!(
        verify_epoch(3, map),
        Err(JournalCorruption::EpochMismatch {
            epoch: 3,
            seq: 0,
            header_epoch: 0
        })
    );
    let mut renamed = BTreeMap::new();
    renamed.insert(0, segments[0].bytes().to_vec());
    renamed.insert(1, segments[2].bytes().to_vec());
    assert_eq!(
        verify_epoch(0, renamed),
        Err(JournalCorruption::SequenceMismatch {
            epoch: 0,
            seq: 1,
            header_seq: 2
        })
    );
}

#[test]
fn verify_epoch_numbers_requires_zero_based_contiguity() {
    assert_eq!(verify_epoch_numbers([]), Ok(()));
    assert_eq!(verify_epoch_numbers([0, 1, 2]), Ok(()));
    assert_eq!(
        verify_epoch_numbers([1]),
        Err(JournalCorruption::EpochGap {
            expected: 0,
            found: 1
        })
    );
    assert_eq!(
        verify_epoch_numbers([0, 2]),
        Err(JournalCorruption::EpochGap {
            expected: 1,
            found: 2
        })
    );
}

#[test]
fn failure_classification_separates_durability_classes() {
    use io::ErrorKind::{AlreadyExists, Other, PermissionDenied};
    use PublicationPhase::*;
    let cases = [
        (CreateStage, Other, Durability::NotCommitted),
        (CreateStage, AlreadyExists, Durability::NotCommitted),
        (WritePayload, Other, Durability::NotCommitted),
        (SyncPayload, Other, Durability::NotCommitted),
        (
            PersistNoReplace,
            AlreadyExists,
            Durability::DestinationExists,
        ),
        (PersistNoReplace, Other, Durability::DurabilityUnknown),
        (
            PersistNoReplace,
            PermissionDenied,
            Durability::DurabilityUnknown,
        ),
        (
            CreateDirectory,
            AlreadyExists,
            Durability::DestinationExists,
        ),
        (CreateDirectory, PermissionDenied, Durability::NotCommitted),
        (SyncDirectory, Other, Durability::DurabilityUnknown),
        (SyncDirectory, AlreadyExists, Durability::DurabilityUnknown),
    ];
    for (phase, kind, expected) in cases {
        assert_eq!(
            classify_failure(phase, kind),
            expected,
            "{phase:?} {kind:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Filesystem shell
// ---------------------------------------------------------------------------

#[test]
fn append_then_recover_round_trips_records() {
    let root = TempDir::new().expect("tempdir");
    let (mut journal, recovered) = open(root.path());
    assert!(recovered.epochs().is_empty());
    assert!(root
        .path()
        .join(JOURNAL_DIRECTORY_NAME)
        .join(LOCK_FILE_NAME)
        .is_file());
    assert_eq!(journal.begin_epoch().expect("begin"), 0);
    let batches = [records(1, 3), single(2), records(3, 1)];
    let mut committed = Vec::new();
    for batch in &batches {
        committed.push(journal.append(batch).expect("append"));
    }
    assert_eq!(
        committed
            .iter()
            .map(CommittedSegment::seq)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(committed
        .iter()
        .all(|segment| segment.durability() == Durability::Committed));
    assert_eq!(committed[0].record_count(), 3);
    drop(journal);

    let (journal, recovered) = open(root.path());
    assert_eq!(recovered.epochs().len(), 1);
    let epoch = &recovered.epochs()[0];
    assert_eq!(epoch.epoch(), 0);
    for ((segment, batch), receipt) in epoch.segments().iter().zip(&batches).zip(&committed) {
        assert_eq!(segment.records(), batch.as_slice());
        assert_eq!(segment.digest(), receipt.digest());
        assert_eq!(segment.seq(), receipt.seq());
    }
    assert_eq!(epoch.records().count(), 5);
    assert_eq!(journal.current_epoch(), Some(0));
    assert_eq!(journal.next_seq(), Some(3));
    assert!(recovered.ignored().is_empty());
}

#[test]
fn append_without_epoch_is_refused_without_effects() {
    let root = TempDir::new().expect("tempdir");
    let (mut journal, _) = open(root.path());
    assert!(matches!(
        journal.append(&single(1)),
        Err(JournalError::NoOpenEpoch)
    ));
    assert!(matches!(
        journal.append(&[]),
        Err(JournalError::NoOpenEpoch)
    ));
    journal.begin_epoch().expect("begin");
    assert!(matches!(
        journal.append(&[]),
        Err(JournalError::Encode {
            error: SegmentEncodeError::BoundExceeded {
                bound: SegmentBound::RecordCount
            }
        })
    ));
    assert_eq!(journal.next_seq(), Some(0));
    assert!(fs::read_dir(epoch_path(root.path(), 0))
        .expect("list")
        .next()
        .is_none());
}

#[test]
fn appends_continue_the_recovered_epoch_chain() {
    let root = TempDir::new().expect("tempdir");
    let first = journal_with_three_segments(root.path());
    let (mut journal, _) = open(root.path());
    let fourth = journal.append(&single(4)).expect("append after recovery");
    assert_eq!((fourth.epoch(), fourth.seq()), (0, 3));
    drop(journal);
    let (_, recovered) = open(root.path());
    let segments = recovered.epochs()[0].segments();
    assert_eq!(segments.len(), 4);
    assert_eq!(segments[2].digest(), first[2].digest());
    let fourth_bytes = fs::read(segment_path(root.path(), 0, 3)).expect("read");
    assert_eq!(
        decode_segment(&fourth_bytes)
            .expect("decode")
            .header()
            .previous_digest,
        first[2].digest()
    );
}

#[test]
fn epoch_rollover_starts_a_new_chain_and_admits_empty_epochs() {
    let root = TempDir::new().expect("tempdir");
    let first = journal_with_three_segments(root.path());
    let (mut journal, _) = open(root.path());
    assert_eq!(journal.begin_epoch().expect("epoch 1"), 1);
    assert_eq!(journal.next_seq(), Some(0));
    let genesis = journal.append(&single(10)).expect("epoch 1 genesis");
    assert_eq!((genesis.epoch(), genesis.seq()), (1, 0));
    assert_eq!(journal.begin_epoch().expect("epoch 2"), 2);
    drop(journal);

    let (mut journal, recovered) = open(root.path());
    let epochs: Vec<u32> = recovered
        .epochs()
        .iter()
        .map(RecoveredEpoch::epoch)
        .collect();
    assert_eq!(epochs, [0, 1, 2]);
    assert_eq!(
        recovered.epochs()[0].final_digest(),
        Some(first[2].digest())
    );
    assert_eq!(recovered.epochs()[1].final_digest(), Some(genesis.digest()));
    assert!(recovered.epochs()[2].segments().is_empty());
    let genesis_bytes = fs::read(segment_path(root.path(), 1, 0)).expect("read");
    assert_eq!(
        decode_segment(&genesis_bytes)
            .expect("decode")
            .header()
            .previous_digest,
        SegmentDigest::ZERO
    );
    assert_eq!(journal.current_epoch(), Some(2));
    assert_eq!(journal.begin_epoch().expect("epoch 3"), 3);
}

#[test]
fn concurrent_writer_is_rejected_until_the_lock_is_released() {
    let root = TempDir::new().expect("tempdir");
    let (holder, _) = open(root.path());
    match CampaignJournal::open(root.path()) {
        Err(JournalError::ConcurrentWriter { lock_path }) => assert_eq!(
            lock_path,
            root.path()
                .join(JOURNAL_DIRECTORY_NAME)
                .join(LOCK_FILE_NAME)
        ),
        other => panic!("expected concurrent writer, got {other:?}"),
    }
    drop(holder);
    open(root.path());
}

#[test]
fn pre_rename_faults_are_not_committed_and_leave_nothing_visible() {
    let faults = [
        (PublicationPhase::CreateStage, FaultTiming::Before),
        (PublicationPhase::CreateStage, FaultTiming::After),
        (PublicationPhase::WritePayload, FaultTiming::Before),
        (PublicationPhase::WritePayload, FaultTiming::After),
        (PublicationPhase::SyncPayload, FaultTiming::Before),
        (PublicationPhase::SyncPayload, FaultTiming::After),
    ];
    for (phase, timing) in faults {
        let root = TempDir::new().expect("tempdir");
        let (mut journal, _) = open_faulty(root.path());
        journal.begin_epoch().expect("begin");
        let committed = journal.append(&single(1)).expect("first append");
        journal.io_mut().arm(phase, timing, io::ErrorKind::Other);
        let error = journal.append(&single(2)).expect_err("injected fault");
        assert_eq!(
            error.durability(),
            Some(Durability::NotCommitted),
            "{phase:?}"
        );
        assert!(matches!(
            error,
            JournalError::NotCommitted {
                target: PublicationTarget::Segment { epoch: 0, seq: 1 },
                phase: failed,
                ..
            } if failed == phase
        ));
        assert!(!segment_path(root.path(), 0, 1).exists(), "{phase:?}");
        assert!(
            stage_files(&epoch_path(root.path(), 0)).is_empty(),
            "{phase:?}"
        );
        assert!(!journal.is_poisoned());
        let retried = journal.append(&single(3)).expect("retry same sequence");
        assert_eq!(retried.seq(), 1);
        drop(journal);
        let (_, recovered) = open(root.path());
        let segments = recovered.epochs()[0].segments();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].digest(), committed.digest());
        assert_eq!(segments[1].records(), single(3).as_slice());
    }
}

fn assert_poisoned<Io: PublicationIo>(journal: &mut CampaignJournal<Io>) {
    assert!(journal.is_poisoned());
    assert!(matches!(
        journal.append(&single(9)),
        Err(JournalError::Poisoned)
    ));
    assert!(matches!(journal.begin_epoch(), Err(JournalError::Poisoned)));
    assert!(matches!(
        journal.resynchronize(),
        Err(JournalError::Poisoned)
    ));
}

#[test]
fn rename_failure_poisons_without_visible_destination() {
    let root = TempDir::new().expect("tempdir");
    let (mut journal, _) = open_faulty(root.path());
    journal.begin_epoch().expect("begin");
    journal.io_mut().arm(
        PublicationPhase::PersistNoReplace,
        FaultTiming::Before,
        io::ErrorKind::Other,
    );
    let error = journal.append(&single(1)).expect_err("injected");
    assert!(matches!(
        error,
        JournalError::DurabilityUnknown {
            target: PublicationTarget::Segment { epoch: 0, seq: 0 },
            phase: PublicationPhase::PersistNoReplace,
            ..
        }
    ));
    assert!(!segment_path(root.path(), 0, 0).exists());
    assert_poisoned(&mut journal);
    assert!(
        !segment_path(root.path(), 0, 0).exists(),
        "poisoned appends publish nothing"
    );
}

#[test]
fn rename_uncertainty_poisons_and_recovery_admits_the_visible_segment() {
    let root = TempDir::new().expect("tempdir");
    let (mut journal, _) = open_faulty(root.path());
    journal.begin_epoch().expect("begin");
    journal.io_mut().arm(
        PublicationPhase::PersistNoReplace,
        FaultTiming::After,
        io::ErrorKind::Other,
    );
    let error = journal.append(&single(1)).expect_err("injected");
    assert_eq!(error.durability(), Some(Durability::DurabilityUnknown));
    assert!(segment_path(root.path(), 0, 0).is_file());
    assert_poisoned(&mut journal);
    drop(journal);
    let (_, recovered) = open(root.path());
    assert_eq!(
        recovered.epochs()[0].segments()[0].records(),
        single(1).as_slice()
    );
}

#[test]
fn directory_sync_failure_poisons_the_journal() {
    for timing in [FaultTiming::Before, FaultTiming::After] {
        let root = TempDir::new().expect("tempdir");
        let (mut journal, _) = open_faulty(root.path());
        journal.begin_epoch().expect("begin");
        journal.append(&single(1)).expect("first");
        journal.io_mut().arm(
            PublicationPhase::SyncDirectory,
            timing,
            io::ErrorKind::Other,
        );
        let error = journal.append(&single(2)).expect_err("injected");
        assert!(matches!(
            error,
            JournalError::DurabilityUnknown {
                target: PublicationTarget::Segment { epoch: 0, seq: 1 },
                phase: PublicationPhase::SyncDirectory,
                ..
            }
        ));
        assert!(segment_path(root.path(), 0, 1).is_file());
        assert_poisoned(&mut journal);
        assert!(!segment_path(root.path(), 0, 2).exists());
    }
}

#[test]
fn sequence_collision_marks_the_writer_stale_until_resynchronized() {
    let root = TempDir::new().expect("tempdir");
    let (mut journal, _) = open(root.path());
    journal.begin_epoch().expect("begin");
    let first = journal.append(&single(1)).expect("first");
    // A stale writer's view: someone else already published sequence 1.
    let foreign = encode_segment(0, 1, first.digest(), &single(7)).expect("encode");
    fs::write(segment_path(root.path(), 0, 1), foreign.bytes()).expect("foreign segment");

    let error = journal.append(&single(2)).expect_err("collision");
    assert!(matches!(
        error,
        JournalError::DestinationExists {
            target: PublicationTarget::Segment { epoch: 0, seq: 1 },
            phase: PublicationPhase::PersistNoReplace,
            ..
        }
    ));
    assert_eq!(error.durability(), Some(Durability::DestinationExists));
    assert!(journal.is_stale());
    assert_eq!(
        fs::read(segment_path(root.path(), 0, 1)).expect("read"),
        foreign.bytes(),
        "the existing destination is never replaced"
    );
    assert!(stage_files(&epoch_path(root.path(), 0)).is_empty());
    assert!(matches!(
        journal.append(&single(3)),
        Err(JournalError::StaleWriter)
    ));
    assert!(matches!(
        journal.begin_epoch(),
        Err(JournalError::StaleWriter)
    ));

    let recovered = journal.resynchronize().expect("resynchronize");
    assert_eq!(
        recovered.epochs()[0].segments()[1].digest(),
        foreign.digest()
    );
    assert!(!journal.is_stale());
    let next = journal.append(&single(4)).expect("replanned append");
    assert_eq!(next.seq(), 2);
}

#[test]
fn injected_collision_is_classified_destination_exists() {
    let root = TempDir::new().expect("tempdir");
    let (mut journal, _) = open_faulty(root.path());
    journal.begin_epoch().expect("begin");
    journal.io_mut().arm(
        PublicationPhase::PersistNoReplace,
        FaultTiming::Before,
        io::ErrorKind::AlreadyExists,
    );
    let error = journal.append(&single(1)).expect_err("injected");
    assert_eq!(error.durability(), Some(Durability::DestinationExists));
    assert!(journal.is_stale());
    assert!(!journal.is_poisoned());
}

#[test]
fn epoch_creation_faults_are_classified() {
    let root = TempDir::new().expect("tempdir");
    let (mut journal, _) = open_faulty(root.path());
    journal.io_mut().arm(
        PublicationPhase::CreateDirectory,
        FaultTiming::Before,
        io::ErrorKind::PermissionDenied,
    );
    let error = journal.begin_epoch().expect_err("injected");
    assert!(matches!(
        error,
        JournalError::NotCommitted {
            target: PublicationTarget::Epoch { epoch: 0 },
            phase: PublicationPhase::CreateDirectory,
            ..
        }
    ));
    assert!(!epoch_path(root.path(), 0).exists());
    assert_eq!(journal.current_epoch(), None);
    assert_eq!(journal.begin_epoch().expect("retry"), 0);

    journal.io_mut().arm(
        PublicationPhase::SyncDirectory,
        FaultTiming::After,
        io::ErrorKind::Other,
    );
    let error = journal.begin_epoch().expect_err("injected");
    assert!(matches!(
        error,
        JournalError::DurabilityUnknown {
            target: PublicationTarget::Epoch { epoch: 1 },
            phase: PublicationPhase::SyncDirectory,
            ..
        }
    ));
    assert_poisoned(&mut journal);
    drop(journal);
    let (_, recovered) = open(root.path());
    assert_eq!(
        recovered.epochs().len(),
        2,
        "the uncertain empty epoch is admitted"
    );
}

#[test]
fn stale_epoch_directory_collision_marks_the_writer_stale() {
    let root = TempDir::new().expect("tempdir");
    let (mut journal, _) = open(root.path());
    fs::create_dir(epoch_path(root.path(), 0)).expect("foreign epoch");
    let error = journal.begin_epoch().expect_err("collision");
    assert!(matches!(
        error,
        JournalError::DestinationExists {
            target: PublicationTarget::Epoch { epoch: 0 },
            ..
        }
    ));
    assert!(journal.is_stale());
    journal.resynchronize().expect("resynchronize");
    assert_eq!(journal.current_epoch(), Some(0));
    assert_eq!(journal.begin_epoch().expect("next epoch"), 1);
}

#[test]
fn recovery_rejects_truncated_segment() {
    let root = TempDir::new().expect("tempdir");
    journal_with_three_segments(root.path());
    let path = segment_path(root.path(), 0, 1);
    let bytes = fs::read(&path).expect("read");
    fs::write(&path, &bytes[..bytes.len() / 2]).expect("truncate");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::Segment {
            epoch: 0,
            seq: 1,
            error: SegmentDecodeError::Truncated
        }
    );
    fs::write(&path, []).expect("empty");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::Segment {
            epoch: 0,
            seq: 1,
            error: SegmentDecodeError::Truncated
        }
    );
}

#[test]
fn recovery_rejects_garbage_and_tampered_segments() {
    let root = TempDir::new().expect("tempdir");
    journal_with_three_segments(root.path());
    let path = segment_path(root.path(), 0, 2);
    let original = fs::read(&path).expect("read");
    fs::write(&path, [0xFF; 64]).expect("garbage");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::Segment {
            epoch: 0,
            seq: 2,
            error: SegmentDecodeError::Garbage
        }
    );
    let mut flipped = original.clone();
    let last_digest_byte = flipped.len() - 1;
    flipped[last_digest_byte] ^= 0x01;
    fs::write(&path, &flipped).expect("flip");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::Segment {
            epoch: 0,
            seq: 2,
            error: SegmentDecodeError::BodyDigestMismatch
        }
    );
    let mut oversized = original;
    oversized.resize(MAX_SEGMENT_BYTES as usize + 1, 0);
    fs::write(&path, &oversized).expect("oversized");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::Segment {
            epoch: 0,
            seq: 2,
            error: SegmentDecodeError::BoundExceeded {
                bound: SegmentBound::SegmentBytes
            }
        }
    );
}

#[test]
fn recovery_rejects_broken_chain() {
    let root = TempDir::new().expect("tempdir");
    journal_with_three_segments(root.path());
    let forged = encode_segment(0, 1, SegmentDigest::ZERO, &single(1)).expect("encode");
    fs::write(segment_path(root.path(), 0, 1), forged.bytes()).expect("forge");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::BrokenChain { epoch: 0, seq: 1 }
    );
}

#[test]
fn recovery_rejects_sequence_and_epoch_gaps() {
    let root = TempDir::new().expect("tempdir");
    journal_with_three_segments(root.path());
    fs::remove_file(segment_path(root.path(), 0, 1)).expect("remove");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::SequenceGap {
            epoch: 0,
            expected: 1,
            found: 2
        }
    );

    let root = TempDir::new().expect("tempdir");
    journal_with_three_segments(root.path());
    fs::create_dir(epoch_path(root.path(), 2)).expect("epoch 2");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::EpochGap {
            expected: 1,
            found: 2
        }
    );
}

#[test]
fn recovery_rejects_duplicate_and_malformed_reserved_names() {
    let root = TempDir::new().expect("tempdir");
    journal_with_three_segments(root.path());
    let alias = epoch_path(root.path(), 0).join("seg-1.cbor");
    fs::copy(segment_path(root.path(), 0, 1), &alias).expect("alias");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::DuplicateSegment { epoch: 0, seq: 1 }
    );
    fs::remove_file(&alias).expect("remove alias");

    let malformed = epoch_path(root.path(), 0).join("seg-latest.cbor");
    fs::write(&malformed, b"x").expect("malformed");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::MalformedSegmentName {
            epoch: 0,
            name: "seg-latest.cbor".into()
        }
    );
    fs::remove_file(&malformed).expect("remove malformed");

    let journal_directory = root.path().join(JOURNAL_DIRECTORY_NAME);
    fs::create_dir(journal_directory.join("epoch-0")).expect("epoch alias");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::DuplicateEpoch { epoch: 0 }
    );
    fs::remove_dir(journal_directory.join("epoch-0")).expect("remove epoch alias");

    fs::write(journal_directory.join("epoch-next"), b"x").expect("malformed epoch");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::MalformedEpochName {
            name: "epoch-next".into()
        }
    );
    fs::remove_file(journal_directory.join("epoch-next")).expect("remove");
    open(root.path());
}

#[test]
fn recovery_rejects_non_directory_epochs_and_non_file_segments() {
    let root = TempDir::new().expect("tempdir");
    journal_with_three_segments(root.path());
    let journal_directory = root.path().join(JOURNAL_DIRECTORY_NAME);
    fs::write(journal_directory.join(epoch_directory_name(1)), b"x").expect("epoch file");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::EpochNotDirectory { epoch: 1 }
    );
    fs::remove_file(journal_directory.join(epoch_directory_name(1))).expect("remove");

    fs::create_dir(segment_path(root.path(), 0, 3)).expect("segment directory");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::SegmentNotRegularFile { epoch: 0, seq: 3 }
    );
    fs::remove_dir(segment_path(root.path(), 0, 3)).expect("remove");

    std::os::unix::fs::symlink(
        segment_path(root.path(), 0, 0),
        segment_path(root.path(), 0, 3),
    )
    .expect("segment symlink");
    assert_eq!(
        corruption(root.path()),
        JournalCorruption::SegmentNotRegularFile { epoch: 0, seq: 3 }
    );
}

#[test]
fn recovery_leaves_stage_leftovers_and_unrelated_files_untouched() {
    let root = TempDir::new().expect("tempdir");
    journal_with_three_segments(root.path());
    let journal_directory = root.path().join(JOURNAL_DIRECTORY_NAME);
    let epoch_directory = epoch_path(root.path(), 0);
    let fixtures = [
        (
            journal_directory.join("notes.txt"),
            IgnoredReason::Unrelated,
            &b"operator notes"[..],
        ),
        (
            epoch_directory.join("README"),
            IgnoredReason::Unrelated,
            b"readme",
        ),
        (
            epoch_directory.join(".seg-stage-leftover.tmp"),
            IgnoredReason::Stage,
            b"partial segment",
        ),
    ];
    for (path, _, contents) in &fixtures {
        fs::write(path, contents).expect("fixture");
    }
    let output_sibling = root.path().join("checkpoint.json");
    fs::write(&output_sibling, b"{}").expect("sibling");
    let modified: Vec<_> = fixtures
        .iter()
        .map(|(path, _, _)| {
            fs::metadata(path)
                .expect("metadata")
                .modified()
                .expect("mtime")
        })
        .collect();

    let (mut journal, recovered) = open(root.path());
    let mut ignored: Vec<(PathBuf, IgnoredReason)> = recovered
        .ignored()
        .iter()
        .map(|entry| (entry.path().to_path_buf(), entry.reason()))
        .collect();
    ignored.sort_by(|left, right| left.0.cmp(&right.0));
    let mut expected: Vec<(PathBuf, IgnoredReason)> = fixtures
        .iter()
        .map(|(path, reason, _)| (path.clone(), *reason))
        .collect();
    expected.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(ignored, expected);
    journal.append(&single(4)).expect("append");
    journal.begin_epoch().expect("rollover");
    drop(journal);
    open(root.path());

    for ((path, _, contents), before) in fixtures.iter().zip(modified) {
        assert_eq!(
            fs::read(path).expect("read"),
            *contents,
            "{}",
            path.display()
        );
        let after = fs::metadata(path)
            .expect("metadata")
            .modified()
            .expect("mtime");
        assert_eq!(after, before, "{}", path.display());
    }
    assert_eq!(fs::read(&output_sibling).expect("read"), b"{}");
}

#[test]
fn open_rejects_missing_output_and_non_directory_journal() {
    let root = TempDir::new().expect("tempdir");
    let missing = root.path().join("missing");
    assert!(matches!(
        CampaignJournal::open(&missing),
        Err(JournalError::Io {
            operation: JournalOperation::CreateJournalDirectory,
            ..
        })
    ));
    assert!(!missing.exists());
    fs::write(root.path().join(JOURNAL_DIRECTORY_NAME), b"x").expect("file");
    assert!(matches!(
        CampaignJournal::open(root.path()),
        Err(JournalError::JournalNotDirectory { .. })
    ));
}
