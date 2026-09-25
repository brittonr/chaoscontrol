//! Product-owned durable journal for Campaign decisions (design CC-D9).
//!
//! Layout under the admitted `output_dir`:
//!
//! ```text
//! campaign-journal/
//!   LOCK                         exclusive writer lock (flock)
//!   epoch-0000/
//!     seg-00000000.cbor          first segment: previous digest is zero
//!     seg-00000001.cbor          previous digest = digest of seg-00000000 bytes
//!   epoch-0001/ ...
//! ```
//!
//! Each segment carries generic typed records (a kind tag plus opaque bytes);
//! the Campaign record mapping is owned elsewhere. Segments are published
//! without replacement through [`PublicationIo`] and classified as a
//! [`Durability`]:
//!
//! - [`Durability::Committed`]: file and directory synchronized; the append
//!   returns a [`CommittedSegment`].
//! - [`Durability::NotCommitted`]: failed before the rename; nothing visible.
//!   The writer stays usable and the same sequence may be retried.
//! - [`Durability::DestinationExists`]: sequence collision; the writer is
//!   stale and refuses appends until [`CampaignJournal::resynchronize`].
//! - [`Durability::DurabilityUnknown`]: rename or directory synchronization
//!   uncertainty; the writer is poisoned and refuses every later operation.
//!
//! Segment encoding, decoding, naming, and chain verification are pure; the
//! filesystem is reached only through this shell.

mod error;
mod format;
mod lock;
mod publication;
mod recovery;
#[cfg(test)]
mod tests;
mod writer;

pub use error::{JournalError, JournalOperation, PublicationTarget};
pub use format::bounds::{
    SegmentBound, SegmentDecodeError, SegmentEncodeError, DIGEST_BYTES, MAX_RECORDS_PER_SEGMENT,
    MAX_RECORD_PAYLOAD_BYTES, MAX_SEGMENT_BYTES, MIN_RECORDS_PER_SEGMENT, SEGMENT_SCHEMA,
};
pub use format::layout::{
    epoch_directory_name, segment_file_name, EPOCH_DIRECTORY_PREFIX, EPOCH_NAME_DIGITS,
    JOURNAL_DIRECTORY_NAME, LOCK_FILE_NAME, MAX_EPOCH, MAX_SEGMENT_SEQ, SEGMENT_FILE_PREFIX,
    SEGMENT_FILE_SUFFIX, SEGMENT_NAME_DIGITS, STAGE_FILE_PREFIX,
};
pub use format::segment::{
    decode_segment, encode_segment, DecodedSegment, EncodedSegment, JournalRecord, RecordKind,
    SegmentDigest, SegmentHeader,
};
pub use format::verify::{JournalCorruption, RecoveredEpoch, RecoveredSegment};
pub use publication::{
    classify_failure, CommittedSegment, Durability, PublicationIo, PublicationPhase,
    SystemPublicationIo,
};
pub use recovery::{IgnoredEntry, IgnoredReason, RecoveredJournal};
pub use writer::CampaignJournal;
