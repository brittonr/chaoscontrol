//! Named segment bounds and the typed refusals that cite them.

use std::fmt;

/// Schema identifier bound into every segment header.
pub const SEGMENT_SCHEMA: &str = "chaoscontrol.campaign-journal-segment.v1";
/// Smallest record count a segment may carry. Empty segments publish nothing.
pub const MIN_RECORDS_PER_SEGMENT: u32 = 1;
/// Largest record count a segment may carry.
pub const MAX_RECORDS_PER_SEGMENT: u32 = 4_096;
/// Largest opaque payload one record may carry (1 MiB).
pub const MAX_RECORD_PAYLOAD_BYTES: u32 = 1_048_576;
/// Largest encoded segment file (16 MiB). Recovery refuses larger files.
pub const MAX_SEGMENT_BYTES: u64 = 16_777_216;
/// Width of every journal digest.
pub const DIGEST_BYTES: usize = 32;

/// A bound that a segment exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SegmentBound {
    /// Epoch above `MAX_EPOCH`.
    Epoch,
    /// Sequence above `MAX_SEGMENT_SEQ`.
    Sequence,
    /// Record count outside `MIN_RECORDS_PER_SEGMENT..=MAX_RECORDS_PER_SEGMENT`.
    RecordCount,
    /// Record payload above [`MAX_RECORD_PAYLOAD_BYTES`].
    RecordPayload,
    /// Encoded segment above [`MAX_SEGMENT_BYTES`].
    SegmentBytes,
}

impl fmt::Display for SegmentBound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Epoch => "epoch number",
            Self::Sequence => "segment sequence number",
            Self::RecordCount => "segment record count",
            Self::RecordPayload => "record payload size",
            Self::SegmentBytes => "encoded segment size",
        };
        formatter.write_str(text)
    }
}

/// Segment encoding refusal. No bytes exist when this is returned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SegmentEncodeError {
    /// A named bound was exceeded.
    BoundExceeded {
        /// The exceeded bound.
        bound: SegmentBound,
    },
    /// The CBOR serializer failed.
    Serialization {
        /// Serializer diagnostic.
        detail: String,
    },
}

impl fmt::Display for SegmentEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BoundExceeded { bound } => write!(formatter, "segment exceeds the {bound} bound"),
            Self::Serialization { detail } => {
                write!(formatter, "segment serialization failed: {detail}")
            }
        }
    }
}

impl std::error::Error for SegmentEncodeError {}

/// Reason a segment's bytes are not an admissible segment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SegmentDecodeError {
    /// The bytes end before one complete CBOR value.
    Truncated,
    /// The bytes are not well-formed CBOR.
    Garbage,
    /// The CBOR value does not have the segment shape.
    Malformed {
        /// Innermost structure that failed the shape check.
        field: &'static str,
    },
    /// The header names a different schema.
    SchemaMismatch,
    /// A named bound was exceeded.
    BoundExceeded {
        /// The exceeded bound.
        bound: SegmentBound,
    },
    /// The header record count differs from the record array length.
    RecordCountMismatch {
        /// Header record count.
        declared: u32,
        /// Records present.
        actual: u64,
    },
    /// The recomputed body digest differs from the stored one.
    BodyDigestMismatch,
    /// The bytes decode but are not the canonical encoding (e.g. trailing bytes).
    NonCanonical,
}

impl fmt::Display for SegmentDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("segment is truncated"),
            Self::Garbage => formatter.write_str("segment is not well-formed CBOR"),
            Self::Malformed { field } => write!(formatter, "segment {field} is malformed"),
            Self::SchemaMismatch => write!(formatter, "segment schema is not {SEGMENT_SCHEMA}"),
            Self::BoundExceeded { bound } => write!(formatter, "segment exceeds the {bound} bound"),
            Self::RecordCountMismatch { declared, actual } => write!(
                formatter,
                "segment declares {declared} records but carries {actual}"
            ),
            Self::BodyDigestMismatch => formatter.write_str("segment body digest does not match"),
            Self::NonCanonical => formatter.write_str("segment bytes are not canonical"),
        }
    }
}

impl std::error::Error for SegmentDecodeError {}
