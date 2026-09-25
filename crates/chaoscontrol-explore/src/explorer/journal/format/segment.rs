//! Pure segment encoding, decoding, and digest rules.
//!
//! A segment is one canonical CBOR map with three fields in this order:
//!
//! 1. `header`: `schema`, `epoch`, `seq`, `previous_digest`, `record_count`.
//! 2. `records`: an array of `{kind, payload}` maps.
//! 3. `body_digest`: the BLAKE3 derive-key digest of the canonical `header`
//!    encoding followed by the canonical `records` encoding.
//!
//! Decoding accepts only the exact bytes that this module would encode for the
//! decoded values, so trailing bytes, unknown fields, reordered fields, and
//! non-minimal encodings are all rejected.

use super::bounds::{
    SegmentBound, SegmentDecodeError, SegmentEncodeError, DIGEST_BYTES, MAX_RECORDS_PER_SEGMENT,
    MAX_SEGMENT_BYTES, MIN_RECORDS_PER_SEGMENT,
};
use super::codec::{
    body_digest, classify_cbor_error, decode_parts, payload_exceeds_bound, serialize_segment,
};
use super::layout::{MAX_EPOCH, MAX_SEGMENT_SEQ};
use ciborium::value::Value;

const SEGMENT_DIGEST_CONTEXT: &str = "chaoscontrol.campaign-journal-segment.v1 segment-bytes";

/// BLAKE3 digest used for segment chaining and body integrity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SegmentDigest([u8; DIGEST_BYTES]);

impl SegmentDigest {
    /// Chain predecessor of the first segment in every epoch.
    pub const ZERO: Self = Self([0; DIGEST_BYTES]);

    /// Wrap raw digest bytes.
    pub const fn from_bytes(bytes: [u8; DIGEST_BYTES]) -> Self {
        Self(bytes)
    }

    /// Digest of complete segment file bytes, as chained by the next segment.
    pub fn of_segment_bytes(bytes: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new_derive_key(SEGMENT_DIGEST_CONTEXT);
        hasher.update(bytes);
        Self(*hasher.finalize().as_bytes())
    }

    /// Raw digest bytes.
    pub const fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
        &self.0
    }
}

/// Product-owned record kind tag. The journal treats it as opaque.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecordKind(u16);

impl RecordKind {
    /// Wrap a record kind tag.
    pub const fn new(tag: u16) -> Self {
        Self(tag)
    }

    /// The raw tag.
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// One journal record: a kind tag and opaque product bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalRecord {
    kind: RecordKind,
    payload: Vec<u8>,
}

impl JournalRecord {
    /// Build a record. Bounds are enforced when the record is encoded.
    pub const fn new(kind: RecordKind, payload: Vec<u8>) -> Self {
        Self { kind, payload }
    }

    /// The record kind tag.
    pub const fn kind(&self) -> RecordKind {
        self.kind
    }

    /// The opaque payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Decoded or encoded segment header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentHeader {
    /// Epoch that owns the segment.
    pub epoch: u32,
    /// Contiguous sequence number within the epoch, starting at zero.
    pub seq: u64,
    /// Digest of the previous segment bytes, or [`SegmentDigest::ZERO`].
    pub previous_digest: SegmentDigest,
    /// Number of records in the segment body.
    pub record_count: u32,
}

/// Canonical segment bytes ready for publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedSegment {
    header: SegmentHeader,
    bytes: Vec<u8>,
    digest: SegmentDigest,
}

impl EncodedSegment {
    /// The encoded header.
    pub const fn header(&self) -> &SegmentHeader {
        &self.header
    }

    /// Canonical file bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Digest of [`Self::bytes`], chained by the next segment.
    pub const fn digest(&self) -> SegmentDigest {
        self.digest
    }
}

/// A verified segment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedSegment {
    header: SegmentHeader,
    records: Vec<JournalRecord>,
    digest: SegmentDigest,
}

impl DecodedSegment {
    /// The verified header.
    pub const fn header(&self) -> &SegmentHeader {
        &self.header
    }

    /// Records in publication order.
    pub fn records(&self) -> &[JournalRecord] {
        &self.records
    }

    /// Digest of the complete segment bytes.
    pub const fn digest(&self) -> SegmentDigest {
        self.digest
    }

    pub(super) fn into_parts(self) -> (SegmentHeader, Vec<JournalRecord>, SegmentDigest) {
        (self.header, self.records, self.digest)
    }
}

/// Encode one segment. Pure: no filesystem access.
pub fn encode_segment(
    epoch: u32,
    seq: u64,
    previous_digest: SegmentDigest,
    records: &[JournalRecord],
) -> Result<EncodedSegment, SegmentEncodeError> {
    let bound = |bound| SegmentEncodeError::BoundExceeded { bound };
    if epoch > MAX_EPOCH {
        return Err(bound(SegmentBound::Epoch));
    }
    if seq > MAX_SEGMENT_SEQ {
        return Err(bound(SegmentBound::Sequence));
    }
    let record_count = u32::try_from(records.len())
        .ok()
        .filter(|count| (MIN_RECORDS_PER_SEGMENT..=MAX_RECORDS_PER_SEGMENT).contains(count))
        .ok_or_else(|| bound(SegmentBound::RecordCount))?;
    if records
        .iter()
        .any(|record| payload_exceeds_bound(&record.payload))
    {
        return Err(bound(SegmentBound::RecordPayload));
    }
    let header = SegmentHeader {
        epoch,
        seq,
        previous_digest,
        record_count,
    };
    let serialization =
        |error: ciborium::ser::Error<std::io::Error>| SegmentEncodeError::Serialization {
            detail: error.to_string(),
        };
    let body_digest = body_digest(&header, records).map_err(serialization)?;
    let bytes = serialize_segment(&header, records, &body_digest).map_err(serialization)?;
    if bytes.len() as u64 > MAX_SEGMENT_BYTES {
        return Err(bound(SegmentBound::SegmentBytes));
    }
    let digest = SegmentDigest::of_segment_bytes(&bytes);
    Ok(EncodedSegment {
        header,
        bytes,
        digest,
    })
}

/// Decode and verify one segment's bytes. Pure: no filesystem access.
///
/// Checks the byte bound, CBOR well-formedness, exact shape, schema, header
/// bounds, record count, record bounds, body digest, and canonical encoding.
/// Chain position (epoch, sequence, previous digest) is the caller's check.
pub fn decode_segment(bytes: &[u8]) -> Result<DecodedSegment, SegmentDecodeError> {
    if bytes.len() as u64 > MAX_SEGMENT_BYTES {
        return Err(SegmentDecodeError::BoundExceeded {
            bound: SegmentBound::SegmentBytes,
        });
    }
    let value: Value = ciborium::de::from_reader(bytes).map_err(classify_cbor_error)?;
    let (header, records, stored_body_digest) = decode_parts(value)?;
    let computed_body_digest =
        body_digest(&header, &records).map_err(|_| SegmentDecodeError::NonCanonical)?;
    if computed_body_digest != stored_body_digest {
        return Err(SegmentDecodeError::BodyDigestMismatch);
    }
    let canonical = serialize_segment(&header, &records, &stored_body_digest)
        .map_err(|_| SegmentDecodeError::NonCanonical)?;
    if canonical != bytes {
        return Err(SegmentDecodeError::NonCanonical);
    }
    Ok(DecodedSegment {
        header,
        records,
        digest: SegmentDigest::of_segment_bytes(bytes),
    })
}
