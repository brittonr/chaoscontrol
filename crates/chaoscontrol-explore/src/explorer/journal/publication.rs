//! NoReplace publication behind an injectable filesystem trait.
//!
//! Segment publication: stage a temporary file in the destination directory,
//! write, fsync the file, rename it without replacement, fsync the directory.
//! Epoch creation: create the directory, fsync its parent.

use super::format::layout::STAGE_FILE_PREFIX;
use super::format::segment::SegmentDigest;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

const STAGE_FILE_SUFFIX: &str = ".tmp";

/// Outcome class of one publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Durability {
    /// The destination and its directory entry were synchronized.
    Committed,
    /// The destination may or may not be visible or durable. The journal is
    /// poisoned: no later append is admitted by this writer.
    DurabilityUnknown,
    /// Nothing became visible at the destination.
    NotCommitted,
    /// The destination already existed; this writer's view is stale.
    DestinationExists,
}

impl fmt::Display for Durability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Committed => "committed",
            Self::DurabilityUnknown => "durability unknown",
            Self::NotCommitted => "not committed",
            Self::DestinationExists => "destination exists",
        };
        formatter.write_str(text)
    }
}

/// One mechanical publication step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationPhase {
    /// Create the staged temporary file.
    CreateStage,
    /// Write the complete payload to the stage.
    WritePayload,
    /// Synchronize the staged payload.
    SyncPayload,
    /// Rename the stage to the destination without replacement.
    PersistNoReplace,
    /// Create a directory.
    CreateDirectory,
    /// Synchronize the directory that received the new name.
    SyncDirectory,
}

impl fmt::Display for PublicationPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::CreateStage => "create stage",
            Self::WritePayload => "write payload",
            Self::SyncPayload => "sync payload",
            Self::PersistNoReplace => "persist without replacement",
            Self::CreateDirectory => "create directory",
            Self::SyncDirectory => "sync directory",
        };
        formatter.write_str(text)
    }
}

/// Classify a failed publication phase. Pure.
///
/// Failures before the rename leave nothing visible. A rename that reports
/// the destination exists is a stale-writer collision. Any other rename
/// failure, and any directory synchronization failure after a new name was
/// created, leaves durability unknown. A failed directory creation leaves at
/// most an empty directory, which recovery admits as an empty epoch.
pub fn classify_failure(phase: PublicationPhase, kind: io::ErrorKind) -> Durability {
    let destination_exists = kind == io::ErrorKind::AlreadyExists;
    match phase {
        PublicationPhase::CreateStage
        | PublicationPhase::WritePayload
        | PublicationPhase::SyncPayload => Durability::NotCommitted,
        PublicationPhase::PersistNoReplace | PublicationPhase::CreateDirectory
            if destination_exists =>
        {
            Durability::DestinationExists
        }
        PublicationPhase::CreateDirectory => Durability::NotCommitted,
        PublicationPhase::PersistNoReplace | PublicationPhase::SyncDirectory => {
            Durability::DurabilityUnknown
        }
    }
}

/// Filesystem effects used by journal publication.
///
/// Dropping a stage that was not persisted must remove it.
pub trait PublicationIo {
    /// Staged file handle.
    type Stage;

    /// Create a staged file inside `directory`.
    fn create_stage(&mut self, directory: &Path) -> io::Result<Self::Stage>;
    /// Write the complete payload to the stage.
    fn write_payload(&mut self, stage: &mut Self::Stage, bytes: &[u8]) -> io::Result<()>;
    /// Synchronize the staged payload.
    fn sync_payload(&mut self, stage: &mut Self::Stage) -> io::Result<()>;
    /// Rename the stage to `destination`, failing with
    /// [`io::ErrorKind::AlreadyExists`] if the destination exists.
    fn persist_no_replace(&mut self, stage: Self::Stage, destination: &Path) -> io::Result<()>;
    /// Create one directory. Fails with [`io::ErrorKind::AlreadyExists`] if it exists.
    fn create_directory(&mut self, path: &Path) -> io::Result<()>;
    /// Synchronize a directory.
    fn sync_directory(&mut self, path: &Path) -> io::Result<()>;
}

/// Production [`PublicationIo`] using the `tempfile` crate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SystemPublicationIo;

impl PublicationIo for SystemPublicationIo {
    type Stage = tempfile::NamedTempFile;

    fn create_stage(&mut self, directory: &Path) -> io::Result<Self::Stage> {
        tempfile::Builder::new()
            .prefix(STAGE_FILE_PREFIX)
            .suffix(STAGE_FILE_SUFFIX)
            .tempfile_in(directory)
    }

    fn write_payload(&mut self, stage: &mut Self::Stage, bytes: &[u8]) -> io::Result<()> {
        stage.write_all(bytes)
    }

    fn sync_payload(&mut self, stage: &mut Self::Stage) -> io::Result<()> {
        stage.as_file().sync_all()
    }

    fn persist_no_replace(&mut self, stage: Self::Stage, destination: &Path) -> io::Result<()> {
        stage
            .persist_noclobber(destination)
            .map(drop)
            .map_err(|error| error.error)
    }

    fn create_directory(&mut self, path: &Path) -> io::Result<()> {
        fs::create_dir(path)
    }

    fn sync_directory(&mut self, path: &Path) -> io::Result<()> {
        fs::File::open(path)?.sync_all()
    }
}

/// A failed publication with its phase and class.
#[derive(Debug)]
pub(super) struct PublicationFailure {
    pub(super) phase: PublicationPhase,
    pub(super) durability: Durability,
    pub(super) source: io::Error,
}

fn failure(phase: PublicationPhase) -> impl FnOnce(io::Error) -> PublicationFailure {
    move |source| PublicationFailure {
        phase,
        durability: classify_failure(phase, source.kind()),
        source,
    }
}

/// Publish `bytes` at `destination` inside `directory` without replacement.
pub(super) fn publish_file<Io: PublicationIo>(
    io: &mut Io,
    directory: &Path,
    destination: &Path,
    bytes: &[u8],
) -> Result<(), PublicationFailure> {
    let mut stage = io
        .create_stage(directory)
        .map_err(failure(PublicationPhase::CreateStage))?;
    io.write_payload(&mut stage, bytes)
        .map_err(failure(PublicationPhase::WritePayload))?;
    io.sync_payload(&mut stage)
        .map_err(failure(PublicationPhase::SyncPayload))?;
    io.persist_no_replace(stage, destination)
        .map_err(failure(PublicationPhase::PersistNoReplace))?;
    io.sync_directory(directory)
        .map_err(failure(PublicationPhase::SyncDirectory))
}

/// Create `path` inside `parent` and synchronize `parent`.
pub(super) fn publish_directory<Io: PublicationIo>(
    io: &mut Io,
    parent: &Path,
    path: &Path,
) -> Result<(), PublicationFailure> {
    io.create_directory(path)
        .map_err(failure(PublicationPhase::CreateDirectory))?;
    io.sync_directory(parent)
        .map_err(failure(PublicationPhase::SyncDirectory))
}

/// Receipt for a committed and synchronized segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedSegment {
    pub(super) epoch: u32,
    pub(super) seq: u64,
    pub(super) digest: SegmentDigest,
    pub(super) record_count: u32,
}

impl CommittedSegment {
    /// Epoch.
    pub const fn epoch(&self) -> u32 {
        self.epoch
    }

    /// Sequence.
    pub const fn seq(&self) -> u64 {
        self.seq
    }

    /// Digest of the published bytes.
    pub const fn digest(&self) -> SegmentDigest {
        self.digest
    }

    /// Records published.
    pub const fn record_count(&self) -> u32 {
        self.record_count
    }

    /// Always [`Durability::Committed`].
    pub const fn durability(&self) -> Durability {
        Durability::Committed
    }
}
