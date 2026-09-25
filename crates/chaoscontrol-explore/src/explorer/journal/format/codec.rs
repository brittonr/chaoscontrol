//! Canonical CBOR wire form: serialization, shape checks, and body digest.

use super::bounds::{
    SegmentBound, SegmentDecodeError, DIGEST_BYTES, MAX_RECORDS_PER_SEGMENT,
    MAX_RECORD_PAYLOAD_BYTES, MIN_RECORDS_PER_SEGMENT, SEGMENT_SCHEMA,
};
use super::layout::{MAX_EPOCH, MAX_SEGMENT_SEQ};
use super::segment::{JournalRecord, RecordKind, SegmentDigest, SegmentHeader};
use ciborium::value::Value;
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Serialize, Serializer};

const BODY_DIGEST_CONTEXT: &str = "chaoscontrol.campaign-journal-segment.v1 body";

const FIELD_SEGMENT: &str = "segment";
const FIELD_HEADER: &str = "header";
const FIELD_RECORDS: &str = "records";
const FIELD_BODY_DIGEST: &str = "body_digest";
const FIELD_SCHEMA: &str = "schema";
const FIELD_EPOCH: &str = "epoch";
const FIELD_SEQ: &str = "seq";
const FIELD_PREVIOUS_DIGEST: &str = "previous_digest";
const FIELD_RECORD_COUNT: &str = "record_count";
const FIELD_RECORD: &str = "record";
const FIELD_KIND: &str = "kind";
const FIELD_PAYLOAD: &str = "payload";

const SEGMENT_FIELDS: [&str; 3] = [FIELD_HEADER, FIELD_RECORDS, FIELD_BODY_DIGEST];
const HEADER_FIELDS: [&str; 5] = [
    FIELD_SCHEMA,
    FIELD_EPOCH,
    FIELD_SEQ,
    FIELD_PREVIOUS_DIGEST,
    FIELD_RECORD_COUNT,
];
const RECORD_FIELDS: [&str; 2] = [FIELD_KIND, FIELD_PAYLOAD];

/// Check the exact segment shape and bounds; return header, records, and
/// the stored body digest.
pub(super) fn decode_parts(
    value: Value,
) -> Result<(SegmentHeader, Vec<JournalRecord>, SegmentDigest), SegmentDecodeError> {
    let [header, records, body_digest] = take_fields(value, &SEGMENT_FIELDS, FIELD_SEGMENT)?;
    let header = decode_header(header)?;
    let records = decode_records(records, header.record_count)?;
    let body_digest = decode_digest(body_digest, FIELD_BODY_DIGEST)?;
    Ok((header, records, body_digest))
}

pub(super) fn payload_exceeds_bound(payload: &[u8]) -> bool {
    payload.len() as u64 > u64::from(MAX_RECORD_PAYLOAD_BYTES)
}

pub(super) fn classify_cbor_error(
    error: ciborium::de::Error<std::io::Error>,
) -> SegmentDecodeError {
    match error {
        ciborium::de::Error::Io(_) => SegmentDecodeError::Truncated,
        ciborium::de::Error::Syntax(_)
        | ciborium::de::Error::Semantic(..)
        | ciborium::de::Error::RecursionLimitExceeded => SegmentDecodeError::Garbage,
    }
}

fn take_fields<const N: usize>(
    value: Value,
    names: &[&str; N],
    field: &'static str,
) -> Result<[Value; N], SegmentDecodeError> {
    let malformed = SegmentDecodeError::Malformed { field };
    let Value::Map(entries) = value else {
        return Err(malformed);
    };
    if entries.len() != N {
        return Err(malformed);
    }
    let mut fields: [Value; N] = std::array::from_fn(|_| Value::Null);
    for ((key, value), (slot, name)) in entries.into_iter().zip(fields.iter_mut().zip(names)) {
        match key {
            Value::Text(key) if key == *name => *slot = value,
            _ => return Err(malformed),
        }
    }
    Ok(fields)
}

fn decode_unsigned(value: Value, field: &'static str) -> Result<u64, SegmentDecodeError> {
    match value {
        Value::Integer(integer) => {
            u64::try_from(integer).map_err(|_| SegmentDecodeError::Malformed { field })
        }
        _ => Err(SegmentDecodeError::Malformed { field }),
    }
}

fn decode_digest(value: Value, field: &'static str) -> Result<SegmentDigest, SegmentDecodeError> {
    match value {
        Value::Bytes(bytes) => <[u8; DIGEST_BYTES]>::try_from(bytes.as_slice())
            .map(SegmentDigest::from_bytes)
            .map_err(|_| SegmentDecodeError::Malformed { field }),
        _ => Err(SegmentDecodeError::Malformed { field }),
    }
}

fn decode_header(value: Value) -> Result<SegmentHeader, SegmentDecodeError> {
    let [schema, epoch, seq, previous_digest, record_count] =
        take_fields(value, &HEADER_FIELDS, FIELD_HEADER)?;
    match schema {
        Value::Text(schema) if schema == SEGMENT_SCHEMA => {}
        Value::Text(_) => return Err(SegmentDecodeError::SchemaMismatch),
        _ => {
            return Err(SegmentDecodeError::Malformed {
                field: FIELD_SCHEMA,
            })
        }
    }
    let bound = |bound| SegmentDecodeError::BoundExceeded { bound };
    let epoch = u32::try_from(decode_unsigned(epoch, FIELD_EPOCH)?)
        .ok()
        .filter(|epoch| *epoch <= MAX_EPOCH)
        .ok_or_else(|| bound(SegmentBound::Epoch))?;
    let seq = decode_unsigned(seq, FIELD_SEQ)?;
    if seq > MAX_SEGMENT_SEQ {
        return Err(bound(SegmentBound::Sequence));
    }
    let previous_digest = decode_digest(previous_digest, FIELD_PREVIOUS_DIGEST)?;
    let record_count = u32::try_from(decode_unsigned(record_count, FIELD_RECORD_COUNT)?)
        .ok()
        .filter(|count| (MIN_RECORDS_PER_SEGMENT..=MAX_RECORDS_PER_SEGMENT).contains(count))
        .ok_or_else(|| bound(SegmentBound::RecordCount))?;
    Ok(SegmentHeader {
        epoch,
        seq,
        previous_digest,
        record_count,
    })
}

fn decode_records(value: Value, declared: u32) -> Result<Vec<JournalRecord>, SegmentDecodeError> {
    let Value::Array(items) = value else {
        return Err(SegmentDecodeError::Malformed {
            field: FIELD_RECORDS,
        });
    };
    if items.len() as u64 != u64::from(declared) {
        return Err(SegmentDecodeError::RecordCountMismatch {
            declared,
            actual: items.len() as u64,
        });
    }
    items.into_iter().map(decode_record).collect()
}

fn decode_record(value: Value) -> Result<JournalRecord, SegmentDecodeError> {
    let [kind, payload] = take_fields(value, &RECORD_FIELDS, FIELD_RECORD)?;
    let kind = u16::try_from(decode_unsigned(kind, FIELD_KIND)?)
        .map_err(|_| SegmentDecodeError::Malformed { field: FIELD_KIND })?;
    let Value::Bytes(payload) = payload else {
        return Err(SegmentDecodeError::Malformed {
            field: FIELD_PAYLOAD,
        });
    };
    if payload_exceeds_bound(&payload) {
        return Err(SegmentDecodeError::BoundExceeded {
            bound: SegmentBound::RecordPayload,
        });
    }
    Ok(JournalRecord::new(RecordKind::new(kind), payload))
}

pub(super) fn body_digest(
    header: &SegmentHeader,
    records: &[JournalRecord],
) -> Result<SegmentDigest, ciborium::ser::Error<std::io::Error>> {
    let mut hasher = blake3::Hasher::new_derive_key(BODY_DIGEST_CONTEXT);
    ciborium::ser::into_writer(&HeaderWire(header), &mut hasher)?;
    ciborium::ser::into_writer(&RecordsWire(records), &mut hasher)?;
    Ok(SegmentDigest::from_bytes(*hasher.finalize().as_bytes()))
}

pub(super) fn serialize_segment(
    header: &SegmentHeader,
    records: &[JournalRecord],
    body_digest: &SegmentDigest,
) -> Result<Vec<u8>, ciborium::ser::Error<std::io::Error>> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(
        &SegmentWire {
            header,
            records,
            body_digest,
        },
        &mut bytes,
    )?;
    Ok(bytes)
}

struct BytesWire<'a>(&'a [u8]);

impl Serialize for BytesWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

struct HeaderWire<'a>(&'a SegmentHeader);

impl Serialize for HeaderWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_struct(FIELD_HEADER, HEADER_FIELDS.len())?;
        map.serialize_field(FIELD_SCHEMA, SEGMENT_SCHEMA)?;
        map.serialize_field(FIELD_EPOCH, &self.0.epoch)?;
        map.serialize_field(FIELD_SEQ, &self.0.seq)?;
        map.serialize_field(
            FIELD_PREVIOUS_DIGEST,
            &BytesWire(self.0.previous_digest.as_bytes()),
        )?;
        map.serialize_field(FIELD_RECORD_COUNT, &self.0.record_count)?;
        map.end()
    }
}

struct RecordWire<'a>(&'a JournalRecord);

impl Serialize for RecordWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_struct(FIELD_RECORD, RECORD_FIELDS.len())?;
        map.serialize_field(FIELD_KIND, &self.0.kind().get())?;
        map.serialize_field(FIELD_PAYLOAD, &BytesWire(self.0.payload()))?;
        map.end()
    }
}

struct RecordsWire<'a>(&'a [JournalRecord]);

impl Serialize for RecordsWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for record in self.0 {
            sequence.serialize_element(&RecordWire(record))?;
        }
        sequence.end()
    }
}

struct SegmentWire<'a> {
    header: &'a SegmentHeader,
    records: &'a [JournalRecord],
    body_digest: &'a SegmentDigest,
}

impl Serialize for SegmentWire<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_struct(FIELD_SEGMENT, SEGMENT_FIELDS.len())?;
        map.serialize_field(FIELD_HEADER, &HeaderWire(self.header))?;
        map.serialize_field(FIELD_RECORDS, &RecordsWire(self.records))?;
        map.serialize_field(FIELD_BODY_DIGEST, &BytesWire(self.body_digest.as_bytes()))?;
        map.end()
    }
}
