//! Journal failure taxonomy.

use super::format::bounds::SegmentEncodeError;
use super::format::verify::JournalCorruption;
use super::publication::{Durability, PublicationPhase};
use std::fmt;
use std::io;
use std::path::PathBuf;

/// Filesystem operation outside publication that failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalOperation {
    /// Inspect the journal directory.
    InspectJournalDirectory,
    /// Create the journal directory.
    CreateJournalDirectory,
    /// Synchronize `output_dir` after creating the journal directory.
    SyncOutputDirectory,
    /// Open the lock file.
    OpenLock,
    /// Take the exclusive lock.
    LockJournal,
    /// List the journal directory.
    ListJournal,
    /// Inspect an epoch directory.
    InspectEpoch,
    /// List an epoch directory.
    ListEpoch,
    /// Inspect a segment file.
    InspectSegment,
    /// Read a segment file.
    ReadSegment,
    /// Synchronize a recovered directory before building on it.
    SyncRecoveredDirectory,
}

impl fmt::Display for JournalOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::InspectJournalDirectory => "inspect journal directory",
            Self::CreateJournalDirectory => "create journal directory",
            Self::SyncOutputDirectory => "sync output directory",
            Self::OpenLock => "open journal lock",
            Self::LockJournal => "lock journal",
            Self::ListJournal => "list journal directory",
            Self::InspectEpoch => "inspect epoch directory",
            Self::ListEpoch => "list epoch directory",
            Self::InspectSegment => "inspect segment",
            Self::ReadSegment => "read segment",
            Self::SyncRecoveredDirectory => "sync recovered directory",
        };
        formatter.write_str(text)
    }
}

/// What a failed publication was creating.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationTarget {
    /// A segment file.
    Segment {
        /// Epoch.
        epoch: u32,
        /// Sequence.
        seq: u64,
    },
    /// An epoch directory.
    Epoch {
        /// Epoch.
        epoch: u32,
    },
}

impl fmt::Display for PublicationTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Segment { epoch, seq } => write!(formatter, "epoch {epoch} segment {seq}"),
            Self::Epoch { epoch } => write!(formatter, "epoch {epoch}"),
        }
    }
}

/// Journal failure.
#[derive(Debug)]
pub enum JournalError {
    /// Another writer holds the journal lock.
    ConcurrentWriter {
        /// Lock file path.
        lock_path: PathBuf,
    },
    /// The journal path exists but is not a directory.
    JournalNotDirectory {
        /// Journal path.
        path: PathBuf,
    },
    /// A non-publication filesystem operation failed.
    Io {
        /// Operation.
        operation: JournalOperation,
        /// Path operated on.
        path: PathBuf,
        /// Underlying error.
        source: io::Error,
    },
    /// Recovery found corruption. Nothing was modified.
    Corruption {
        /// The corruption.
        corruption: JournalCorruption,
    },
    /// The records cannot form a segment. Nothing was published.
    Encode {
        /// Encoding refusal.
        error: SegmentEncodeError,
    },
    /// No epoch exists yet; call [`CampaignJournal::begin_epoch`](super::CampaignJournal::begin_epoch) first.
    NoOpenEpoch,
    /// The epoch namespace is exhausted.
    EpochLimitReached {
        /// Last epoch in use.
        last_epoch: u32,
    },
    /// [`Durability::NotCommitted`]: nothing became visible.
    NotCommitted {
        /// Target.
        target: PublicationTarget,
        /// Failed phase.
        phase: PublicationPhase,
        /// Underlying error.
        source: io::Error,
    },
    /// [`Durability::DestinationExists`]: stale writer; resynchronize and replan.
    DestinationExists {
        /// Target.
        target: PublicationTarget,
        /// Failed phase.
        phase: PublicationPhase,
        /// Underlying error.
        source: io::Error,
    },
    /// [`Durability::DurabilityUnknown`]: the journal is now poisoned.
    DurabilityUnknown {
        /// Target.
        target: PublicationTarget,
        /// Failed phase.
        phase: PublicationPhase,
        /// Underlying error.
        source: io::Error,
    },
    /// An earlier publication left durability unknown.
    Poisoned,
    /// An earlier sequence collision left this writer stale.
    StaleWriter,
}

impl JournalError {
    /// Publication class of this failure, if it came from a publication.
    pub const fn durability(&self) -> Option<Durability> {
        match self {
            Self::NotCommitted { .. } => Some(Durability::NotCommitted),
            Self::DestinationExists { .. } => Some(Durability::DestinationExists),
            Self::DurabilityUnknown { .. } => Some(Durability::DurabilityUnknown),
            _ => None,
        }
    }
}

impl fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConcurrentWriter { lock_path } => write!(
                formatter,
                "another writer holds the journal lock {}",
                lock_path.display()
            ),
            Self::JournalNotDirectory { path } => {
                write!(
                    formatter,
                    "journal path {} is not a directory",
                    path.display()
                )
            }
            Self::Io {
                operation,
                path,
                source,
            } => write!(formatter, "{operation} {}: {source}", path.display()),
            Self::Corruption { corruption } => {
                write!(formatter, "journal corruption: {corruption}")
            }
            Self::Encode { error } => write!(formatter, "journal encoding refused: {error}"),
            Self::NoOpenEpoch => formatter.write_str("no journal epoch is open"),
            Self::EpochLimitReached { last_epoch } => {
                write!(
                    formatter,
                    "journal epoch limit reached after epoch {last_epoch}"
                )
            }
            Self::NotCommitted {
                target,
                phase,
                source,
            } => write!(formatter, "{target} not committed at {phase}: {source}"),
            Self::DestinationExists {
                target,
                phase,
                source,
            } => write!(formatter, "{target} already exists at {phase}: {source}"),
            Self::DurabilityUnknown {
                target,
                phase,
                source,
            } => write!(
                formatter,
                "{target} durability unknown at {phase}: {source}; journal poisoned"
            ),
            Self::Poisoned => formatter.write_str("journal is poisoned by unknown durability"),
            Self::StaleWriter => {
                formatter.write_str("journal writer is stale; resynchronize before appending")
            }
        }
    }
}

impl std::error::Error for JournalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. }
            | Self::NotCommitted { source, .. }
            | Self::DestinationExists { source, .. }
            | Self::DurabilityUnknown { source, .. } => Some(source),
            Self::Corruption { corruption } => Some(corruption),
            Self::Encode { error } => Some(error),
            _ => None,
        }
    }
}
