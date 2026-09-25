//! Exclusive journal writer: open, recover, append, and epoch rollover.

use super::error::{JournalError, JournalOperation, PublicationTarget};
use super::format::layout::{
    epoch_directory_name, segment_file_name, JOURNAL_DIRECTORY_NAME, LOCK_FILE_NAME, MAX_EPOCH,
};
use super::format::segment::{encode_segment, JournalRecord, SegmentDigest};
use super::lock::{acquire_lock, JournalLock, LockFailure};
use super::publication::{
    publish_directory, publish_file, CommittedSegment, Durability, PublicationFailure,
    PublicationIo, SystemPublicationIo,
};
use super::recovery::{recover_journal, RecoveredJournal};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WriterTail {
    epoch: u32,
    next_seq: u64,
    previous_digest: SegmentDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriterState {
    Ready,
    Stale,
    Poisoned,
}

/// Exclusive journal writer. Holds the lock until dropped.
#[derive(Debug)]
pub struct CampaignJournal<Io: PublicationIo = SystemPublicationIo> {
    directory: PathBuf,
    _lock: JournalLock,
    io: Io,
    tail: Option<WriterTail>,
    state: WriterState,
}

impl CampaignJournal<SystemPublicationIo> {
    /// Open, lock, and recover the journal under `output_dir`.
    pub fn open(output_dir: &Path) -> Result<(Self, RecoveredJournal), JournalError> {
        Self::open_with_io(output_dir, SystemPublicationIo)
    }
}

impl<Io: PublicationIo> CampaignJournal<Io> {
    /// Open with an injected [`PublicationIo`].
    ///
    /// Creates `output_dir/campaign-journal` if absent, takes the exclusive
    /// lock, verifies every epoch, synchronizes the journal directory and the
    /// last epoch directory, and positions the writer after the last segment
    /// of the last epoch.
    pub fn open_with_io(
        output_dir: &Path,
        mut io: Io,
    ) -> Result<(Self, RecoveredJournal), JournalError> {
        let directory = output_dir.join(JOURNAL_DIRECTORY_NAME);
        ensure_journal_directory(&mut io, output_dir, &directory)?;
        let lock_path = directory.join(LOCK_FILE_NAME);
        let lock = acquire_lock(&lock_path).map_err(|failure| match failure {
            LockFailure::Busy => JournalError::ConcurrentWriter {
                lock_path: lock_path.clone(),
            },
            LockFailure::Open(source) => JournalError::Io {
                operation: JournalOperation::OpenLock,
                path: lock_path.clone(),
                source,
            },
            LockFailure::Lock(source) => JournalError::Io {
                operation: JournalOperation::LockJournal,
                path: lock_path.clone(),
                source,
            },
        })?;
        let mut journal = Self {
            directory,
            _lock: lock,
            io,
            tail: None,
            state: WriterState::Ready,
        };
        let recovered = journal.load()?;
        Ok((journal, recovered))
    }

    /// Journal directory.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Epoch the next append targets, if any epoch exists.
    pub fn current_epoch(&self) -> Option<u32> {
        self.tail.map(|tail| tail.epoch)
    }

    /// Sequence the next append will use, if any epoch exists.
    pub fn next_seq(&self) -> Option<u64> {
        self.tail.map(|tail| tail.next_seq)
    }

    /// True after a [`Durability::DurabilityUnknown`] publication.
    pub fn is_poisoned(&self) -> bool {
        self.state == WriterState::Poisoned
    }

    /// True after a [`Durability::DestinationExists`] publication.
    pub fn is_stale(&self) -> bool {
        self.state == WriterState::Stale
    }

    /// Publish one segment of `records` at the tail of the current epoch.
    pub fn append(&mut self, records: &[JournalRecord]) -> Result<CommittedSegment, JournalError> {
        self.ensure_writable()?;
        let tail = self.tail.ok_or(JournalError::NoOpenEpoch)?;
        let encoded = encode_segment(tail.epoch, tail.next_seq, tail.previous_digest, records)
            .map_err(|error| JournalError::Encode { error })?;
        let epoch_directory = self.directory.join(epoch_directory_name(tail.epoch));
        let destination = epoch_directory.join(segment_file_name(tail.next_seq));
        let target = PublicationTarget::Segment {
            epoch: tail.epoch,
            seq: tail.next_seq,
        };
        if let Err(failure) = publish_file(
            &mut self.io,
            &epoch_directory,
            &destination,
            encoded.bytes(),
        ) {
            return Err(self.record_failure(target, failure));
        }
        self.tail = Some(WriterTail {
            epoch: tail.epoch,
            next_seq: tail.next_seq.saturating_add(1),
            previous_digest: encoded.digest(),
        });
        Ok(CommittedSegment {
            epoch: tail.epoch,
            seq: tail.next_seq,
            digest: encoded.digest(),
            record_count: encoded.header().record_count,
        })
    }

    /// Create the next epoch directory durably and direct appends to it.
    pub fn begin_epoch(&mut self) -> Result<u32, JournalError> {
        self.ensure_writable()?;
        let epoch = match self.tail {
            None => 0,
            Some(tail) => tail
                .epoch
                .checked_add(1)
                .filter(|epoch| *epoch <= MAX_EPOCH)
                .ok_or(JournalError::EpochLimitReached {
                    last_epoch: tail.epoch,
                })?,
        };
        let path = self.directory.join(epoch_directory_name(epoch));
        if let Err(failure) = publish_directory(&mut self.io, &self.directory, &path) {
            return Err(self.record_failure(PublicationTarget::Epoch { epoch }, failure));
        }
        self.tail = Some(WriterTail {
            epoch,
            next_seq: 0,
            previous_digest: SegmentDigest::ZERO,
        });
        Ok(epoch)
    }

    /// Re-verify the journal under the held lock and adopt its tail.
    ///
    /// Clears a stale writer after [`Durability::DestinationExists`]. A
    /// poisoned writer is refused: only a fresh open may recover it.
    pub fn resynchronize(&mut self) -> Result<RecoveredJournal, JournalError> {
        if self.state == WriterState::Poisoned {
            return Err(JournalError::Poisoned);
        }
        let recovered = self.load()?;
        self.state = WriterState::Ready;
        Ok(recovered)
    }

    #[cfg(test)]
    pub(super) fn io_mut(&mut self) -> &mut Io {
        &mut self.io
    }

    fn ensure_writable(&self) -> Result<(), JournalError> {
        match self.state {
            WriterState::Ready => Ok(()),
            WriterState::Stale => Err(JournalError::StaleWriter),
            WriterState::Poisoned => Err(JournalError::Poisoned),
        }
    }

    fn load(&mut self) -> Result<RecoveredJournal, JournalError> {
        let recovered = recover_journal(&self.directory)?;
        sync_recovered(&mut self.io, &self.directory)?;
        self.tail = match recovered.epochs().last() {
            None => None,
            Some(last) => {
                let epoch_directory = self.directory.join(epoch_directory_name(last.epoch()));
                sync_recovered(&mut self.io, &epoch_directory)?;
                Some(WriterTail {
                    epoch: last.epoch(),
                    next_seq: last.next_seq(),
                    previous_digest: last.final_digest().unwrap_or(SegmentDigest::ZERO),
                })
            }
        };
        Ok(recovered)
    }

    fn record_failure(
        &mut self,
        target: PublicationTarget,
        failure: PublicationFailure,
    ) -> JournalError {
        let PublicationFailure {
            phase,
            durability,
            source,
        } = failure;
        match durability {
            Durability::NotCommitted => JournalError::NotCommitted {
                target,
                phase,
                source,
            },
            Durability::DestinationExists => {
                self.state = WriterState::Stale;
                JournalError::DestinationExists {
                    target,
                    phase,
                    source,
                }
            }
            // A failure is never classified Committed; treat it as unknown.
            Durability::DurabilityUnknown | Durability::Committed => {
                self.state = WriterState::Poisoned;
                JournalError::DurabilityUnknown {
                    target,
                    phase,
                    source,
                }
            }
        }
    }
}

fn sync_recovered<Io: PublicationIo>(io: &mut Io, path: &Path) -> Result<(), JournalError> {
    io.sync_directory(path).map_err(|source| JournalError::Io {
        operation: JournalOperation::SyncRecoveredDirectory,
        path: path.to_path_buf(),
        source,
    })
}

fn ensure_journal_directory<Io: PublicationIo>(
    io: &mut Io,
    output_dir: &Path,
    directory: &Path,
) -> Result<(), JournalError> {
    let io_error = |operation| {
        move |source| JournalError::Io {
            operation,
            path: directory.to_path_buf(),
            source,
        }
    };
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.is_dir() => return Ok(()),
        Ok(_) => {
            return Err(JournalError::JournalNotDirectory {
                path: directory.to_path_buf(),
            })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(JournalOperation::InspectJournalDirectory)(error)),
    }
    match io.create_directory(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(io_error(JournalOperation::CreateJournalDirectory)(error)),
    }
    io.sync_directory(output_dir)
        .map_err(|source| JournalError::Io {
            operation: JournalOperation::SyncOutputDirectory,
            path: output_dir.to_path_buf(),
            source,
        })
}
