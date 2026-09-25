//! Pure epoch and chain verification over already-read segment bytes.

use super::bounds::SegmentDecodeError;
use super::segment::{decode_segment, JournalRecord, SegmentDigest};
use std::collections::BTreeMap;
use std::fmt;

/// Typed journal corruption found during recovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalCorruption {
    /// A name in the reserved `epoch-` namespace is not a valid epoch name.
    MalformedEpochName {
        /// Lossy entry name.
        name: String,
    },
    /// Two names denote the same epoch.
    DuplicateEpoch {
        /// Duplicated epoch.
        epoch: u64,
    },
    /// An epoch name denotes something other than a directory.
    EpochNotDirectory {
        /// Epoch.
        epoch: u32,
    },
    /// Epoch directories are not contiguous from zero.
    EpochGap {
        /// Next expected epoch.
        expected: u32,
        /// Epoch found instead.
        found: u32,
    },
    /// A name in the reserved `seg-` namespace is not a valid segment name.
    MalformedSegmentName {
        /// Epoch.
        epoch: u32,
        /// Lossy entry name.
        name: String,
    },
    /// Two names denote the same segment.
    DuplicateSegment {
        /// Epoch.
        epoch: u32,
        /// Duplicated sequence.
        seq: u64,
    },
    /// A segment name denotes something other than a regular file.
    SegmentNotRegularFile {
        /// Epoch.
        epoch: u32,
        /// Sequence.
        seq: u64,
    },
    /// Segments are not contiguous from zero.
    SequenceGap {
        /// Epoch.
        epoch: u32,
        /// Next expected sequence.
        expected: u64,
        /// Sequence found instead.
        found: u64,
    },
    /// A segment's bytes fail decoding or verification.
    Segment {
        /// Epoch.
        epoch: u32,
        /// Sequence from the file name.
        seq: u64,
        /// Decode failure.
        error: SegmentDecodeError,
    },
    /// A segment header names a different epoch than its directory.
    EpochMismatch {
        /// Epoch from the directory name.
        epoch: u32,
        /// Sequence.
        seq: u64,
        /// Epoch in the header.
        header_epoch: u32,
    },
    /// A segment header names a different sequence than its file name.
    SequenceMismatch {
        /// Epoch.
        epoch: u32,
        /// Sequence from the file name.
        seq: u64,
        /// Sequence in the header.
        header_seq: u64,
    },
    /// A segment's previous digest differs from its predecessor's digest.
    BrokenChain {
        /// Epoch.
        epoch: u32,
        /// Sequence whose link is broken.
        seq: u64,
    },
}

impl fmt::Display for JournalCorruption {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedEpochName { name } => write!(formatter, "malformed epoch name {name:?}"),
            Self::DuplicateEpoch { epoch } => write!(formatter, "epoch {epoch} has two names"),
            Self::EpochNotDirectory { epoch } => {
                write!(formatter, "epoch {epoch} is not a directory")
            }
            Self::EpochGap { expected, found } => {
                write!(formatter, "epoch gap: expected {expected}, found {found}")
            }
            Self::MalformedSegmentName { epoch, name } => {
                write!(formatter, "epoch {epoch}: malformed segment name {name:?}")
            }
            Self::DuplicateSegment { epoch, seq } => {
                write!(formatter, "epoch {epoch}: segment {seq} has two names")
            }
            Self::SegmentNotRegularFile { epoch, seq } => {
                write!(
                    formatter,
                    "epoch {epoch}: segment {seq} is not a regular file"
                )
            }
            Self::SequenceGap {
                epoch,
                expected,
                found,
            } => write!(
                formatter,
                "epoch {epoch}: segment gap, expected {expected}, found {found}"
            ),
            Self::Segment { epoch, seq, error } => {
                write!(formatter, "epoch {epoch}: segment {seq}: {error}")
            }
            Self::EpochMismatch {
                epoch,
                seq,
                header_epoch,
            } => write!(
                formatter,
                "epoch {epoch}: segment {seq} header names epoch {header_epoch}"
            ),
            Self::SequenceMismatch {
                epoch,
                seq,
                header_seq,
            } => write!(
                formatter,
                "epoch {epoch}: segment {seq} header names sequence {header_seq}"
            ),
            Self::BrokenChain { epoch, seq } => {
                write!(
                    formatter,
                    "epoch {epoch}: segment {seq} breaks the digest chain"
                )
            }
        }
    }
}

impl std::error::Error for JournalCorruption {}

/// One verified segment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredSegment {
    seq: u64,
    digest: SegmentDigest,
    records: Vec<JournalRecord>,
}

impl RecoveredSegment {
    /// Sequence within its epoch.
    pub const fn seq(&self) -> u64 {
        self.seq
    }

    /// Digest of the segment bytes.
    pub const fn digest(&self) -> SegmentDigest {
        self.digest
    }

    /// Records in publication order.
    pub fn records(&self) -> &[JournalRecord] {
        &self.records
    }
}

/// One verified epoch: contiguous, chained segments in sequence order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredEpoch {
    epoch: u32,
    segments: Vec<RecoveredSegment>,
}

impl RecoveredEpoch {
    /// Epoch number.
    pub const fn epoch(&self) -> u32 {
        self.epoch
    }

    /// Segments in sequence order. Empty for an epoch with no publication.
    pub fn segments(&self) -> &[RecoveredSegment] {
        &self.segments
    }

    /// All records of the epoch in publication order.
    pub fn records(&self) -> impl Iterator<Item = &JournalRecord> {
        self.segments
            .iter()
            .flat_map(|segment| segment.records.iter())
    }

    /// Digest of the last segment, or `None` when the epoch is empty.
    pub fn final_digest(&self) -> Option<SegmentDigest> {
        self.segments.last().map(|segment| segment.digest)
    }

    /// Sequence the next segment of this epoch must use.
    pub fn next_seq(&self) -> u64 {
        self.segments.len() as u64
    }
}

/// Verify epoch numbers are exactly `0..n`. Input must be ascending.
pub(in crate::explorer::journal) fn verify_epoch_numbers(
    epochs: impl IntoIterator<Item = u32>,
) -> Result<(), JournalCorruption> {
    for (expected, found) in (0_u32..).zip(epochs) {
        if expected != found {
            return Err(JournalCorruption::EpochGap { expected, found });
        }
    }
    Ok(())
}

/// Verify one epoch's segments, keyed by the sequence from their file names.
pub(in crate::explorer::journal) fn verify_epoch(
    epoch: u32,
    segments: BTreeMap<u64, Vec<u8>>,
) -> Result<RecoveredEpoch, JournalCorruption> {
    let mut previous_digest = SegmentDigest::ZERO;
    let mut recovered = Vec::with_capacity(segments.len());
    for (expected, (seq, bytes)) in (0_u64..).zip(segments) {
        if seq != expected {
            return Err(JournalCorruption::SequenceGap {
                epoch,
                expected,
                found: seq,
            });
        }
        let decoded = decode_segment(&bytes).map_err(|error| JournalCorruption::Segment {
            epoch,
            seq,
            error,
        })?;
        let (header, records, digest) = decoded.into_parts();
        if header.epoch != epoch {
            return Err(JournalCorruption::EpochMismatch {
                epoch,
                seq,
                header_epoch: header.epoch,
            });
        }
        if header.seq != seq {
            return Err(JournalCorruption::SequenceMismatch {
                epoch,
                seq,
                header_seq: header.seq,
            });
        }
        if header.previous_digest != previous_digest {
            return Err(JournalCorruption::BrokenChain { epoch, seq });
        }
        previous_digest = digest;
        recovered.push(RecoveredSegment {
            seq,
            digest,
            records,
        });
    }
    Ok(RecoveredEpoch {
        epoch,
        segments: recovered,
    })
}
