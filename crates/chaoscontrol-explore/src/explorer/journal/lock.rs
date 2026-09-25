//! Exclusive, non-blocking writer lock on `campaign-journal/LOCK`.
//!
//! The lock is an advisory `flock` held for the life of the open file
//! description, so a second open (in this or another process) is refused.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

const LOCK_FILE_MODE: u32 = 0o600;

/// Held writer lock. Dropping it releases the lock.
#[derive(Debug)]
pub(super) struct JournalLock {
    _file: File,
}

#[derive(Debug)]
pub(super) enum LockFailure {
    Busy,
    Open(io::Error),
    Lock(io::Error),
}

pub(super) fn acquire_lock(path: &Path) -> Result<JournalLock, LockFailure> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(LOCK_FILE_MODE)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(LockFailure::Open)?;
    let metadata = file.metadata().map_err(LockFailure::Open)?;
    if !metadata.file_type().is_file() {
        return Err(LockFailure::Open(io::Error::new(
            io::ErrorKind::InvalidData,
            "journal lock is not a regular file",
        )));
    }
    match file.try_lock() {
        Ok(()) => Ok(JournalLock { _file: file }),
        Err(TryLockError::WouldBlock) => Err(LockFailure::Busy),
        Err(TryLockError::Error(error)) => Err(LockFailure::Lock(error)),
    }
}
