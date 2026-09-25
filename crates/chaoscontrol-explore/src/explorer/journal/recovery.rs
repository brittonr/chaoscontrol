//! Recovery shell: list and read journal entries, then verify them purely.
//!
//! Recovery never creates, removes, or rewrites entries. Stage leftovers and
//! unrelated names are reported and left untouched.

use super::format::bounds::MAX_SEGMENT_BYTES;
use super::format::layout::{classify_epoch_entry, classify_root_entry, EpochEntry, RootEntry};
use super::format::verify::{
    verify_epoch, verify_epoch_numbers, JournalCorruption, RecoveredEpoch,
};
use super::{JournalError, JournalOperation};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Why recovery ignored an entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IgnoredReason {
    /// A staged segment left by an interrupted publication.
    Stage,
    /// A name outside the journal's reserved namespaces.
    Unrelated,
}

/// An entry recovery left untouched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IgnoredEntry {
    path: PathBuf,
    reason: IgnoredReason,
}

impl IgnoredEntry {
    /// Entry path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Why the entry was ignored.
    pub const fn reason(&self) -> IgnoredReason {
        self.reason
    }
}

/// Verified journal contents in epoch order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredJournal {
    epochs: Vec<RecoveredEpoch>,
    ignored: Vec<IgnoredEntry>,
}

impl RecoveredJournal {
    /// Epochs `0..n` in order.
    pub fn epochs(&self) -> &[RecoveredEpoch] {
        &self.epochs
    }

    /// Entries recovery ignored and left untouched.
    pub fn ignored(&self) -> &[IgnoredEntry] {
        &self.ignored
    }
}

pub(super) fn recover_journal(journal_directory: &Path) -> Result<RecoveredJournal, JournalError> {
    let mut ignored = Vec::new();
    let mut epochs = BTreeMap::new();
    let mut aliases = Vec::new();
    for (name, path) in list_directory(journal_directory, JournalOperation::ListJournal)? {
        match classify_root_entry(name.as_bytes()) {
            RootEntry::Lock => {}
            RootEntry::Epoch(epoch) => {
                epochs.insert(epoch, path);
            }
            RootEntry::EpochAlias(epoch) => aliases.push((epoch, name)),
            RootEntry::MalformedEpoch => {
                return Err(corrupt(JournalCorruption::MalformedEpochName {
                    name: lossy(&name),
                }))
            }
            RootEntry::Unrelated => ignored.push(IgnoredEntry {
                path,
                reason: IgnoredReason::Unrelated,
            }),
        }
    }
    if let Some((epoch, name)) = aliases.into_iter().next() {
        let canonical_exists = u32::try_from(epoch).is_ok_and(|epoch| epochs.contains_key(&epoch));
        return Err(corrupt(if canonical_exists {
            JournalCorruption::DuplicateEpoch { epoch }
        } else {
            JournalCorruption::MalformedEpochName { name: lossy(&name) }
        }));
    }
    verify_epoch_numbers(epochs.keys().copied()).map_err(corrupt)?;
    let mut recovered = Vec::with_capacity(epochs.len());
    for (epoch, path) in epochs {
        recovered.push(recover_epoch(epoch, &path, &mut ignored)?);
    }
    Ok(RecoveredJournal {
        epochs: recovered,
        ignored,
    })
}

fn recover_epoch(
    epoch: u32,
    path: &Path,
    ignored: &mut Vec<IgnoredEntry>,
) -> Result<RecoveredEpoch, JournalError> {
    let metadata =
        fs::symlink_metadata(path).map_err(io_error(JournalOperation::InspectEpoch, path))?;
    if !metadata.is_dir() {
        return Err(corrupt(JournalCorruption::EpochNotDirectory { epoch }));
    }
    let mut segments = BTreeMap::new();
    let mut aliases = Vec::new();
    for (name, entry_path) in list_directory(path, JournalOperation::ListEpoch)? {
        match classify_epoch_entry(name.as_bytes()) {
            EpochEntry::Segment(seq) => {
                segments.insert(seq, entry_path);
            }
            EpochEntry::SegmentAlias(seq) => aliases.push((seq, name)),
            EpochEntry::MalformedSegment => {
                return Err(corrupt(JournalCorruption::MalformedSegmentName {
                    epoch,
                    name: lossy(&name),
                }))
            }
            EpochEntry::Stage => ignored.push(IgnoredEntry {
                path: entry_path,
                reason: IgnoredReason::Stage,
            }),
            EpochEntry::Unrelated => ignored.push(IgnoredEntry {
                path: entry_path,
                reason: IgnoredReason::Unrelated,
            }),
        }
    }
    if let Some((seq, name)) = aliases.into_iter().next() {
        return Err(corrupt(if segments.contains_key(&seq) {
            JournalCorruption::DuplicateSegment { epoch, seq }
        } else {
            JournalCorruption::MalformedSegmentName {
                epoch,
                name: lossy(&name),
            }
        }));
    }
    let mut contents = BTreeMap::new();
    for (seq, segment_path) in segments {
        contents.insert(seq, read_segment(epoch, seq, &segment_path)?);
    }
    verify_epoch(epoch, contents).map_err(corrupt)
}

fn read_segment(epoch: u32, seq: u64, path: &Path) -> Result<Vec<u8>, JournalError> {
    let not_regular = || corrupt(JournalCorruption::SegmentNotRegularFile { epoch, seq });
    let metadata =
        fs::symlink_metadata(path).map_err(io_error(JournalOperation::InspectSegment, path))?;
    if !metadata.is_file() {
        return Err(not_regular());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(io_error(JournalOperation::ReadSegment, path))?;
    if !file
        .metadata()
        .map_err(io_error(JournalOperation::InspectSegment, path))?
        .is_file()
    {
        return Err(not_regular());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_SEGMENT_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(io_error(JournalOperation::ReadSegment, path))?;
    Ok(bytes)
}

fn list_directory(
    path: &Path,
    operation: JournalOperation,
) -> Result<Vec<(OsString, PathBuf)>, JournalError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path).map_err(io_error(operation, path))? {
        let entry = entry.map_err(io_error(operation, path))?;
        entries.push((entry.file_name(), entry.path()));
    }
    entries.sort();
    Ok(entries)
}

fn lossy(name: &OsString) -> String {
    name.to_string_lossy().into_owned()
}

fn corrupt(corruption: JournalCorruption) -> JournalError {
    JournalError::Corruption { corruption }
}

fn io_error(
    operation: JournalOperation,
    path: &Path,
) -> impl FnOnce(io::Error) -> JournalError + '_ {
    move |source| JournalError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}
