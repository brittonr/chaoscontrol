use crate::assertion_summary::AssertionSummaryV2;
use std::fs;
use std::io::{self, Write};

use tempfile::NamedTempFile;

const MEBIBYTE_BYTES: usize = 1024 * 1024;
const MAX_ASSERTION_SUMMARY_SERIALIZED_BYTES: usize = 64 * MEBIBYTE_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionSummaryWrite {
    Written,
}

pub fn write_assertion_summary<F>(
    destination: impl AsRef<std::path::Path>,
    build: F,
) -> Result<AssertionSummaryWrite, String>
where
    F: FnOnce() -> Result<AssertionSummaryV2, String>,
{
    let destination = destination.as_ref();
    remove_destination(destination)?;
    let summary = build()?;
    summary.validate()?;
    let bytes = serialize_bounded(&summary)?;
    persist_same_directory(destination, &bytes)?;
    Ok(AssertionSummaryWrite::Written)
}

fn remove_destination(destination: &std::path::Path) -> Result<(), String> {
    match fs::remove_file(destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to remove stale assertion summary {}: {error}",
            destination.display()
        )),
    }
}

fn serialize_bounded(summary: &AssertionSummaryV2) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec_pretty(summary)
        .map_err(|error| format!("failed to serialize assertion summary: {error}"))?;
    if bytes.len() > MAX_ASSERTION_SUMMARY_SERIALIZED_BYTES {
        return Err(format!(
            "assertion summary has {} bytes; maximum is {MAX_ASSERTION_SUMMARY_SERIALIZED_BYTES}",
            bytes.len()
        ));
    }
    Ok(bytes)
}

fn persist_same_directory(destination: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| write_error(destination, "create temporary file", error))?;
    temporary
        .write_all(bytes)
        .map_err(|error| write_error(destination, "write temporary file", error))?;
    temporary
        .flush()
        .map_err(|error| write_error(destination, "flush temporary file", error))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| write_error(destination, "sync temporary file", error))?;
    temporary
        .persist(destination)
        .map_err(|error| write_error(destination, "persist temporary file", error.error))?;
    sync_parent(parent, destination)
}

fn sync_parent(parent: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| write_error(destination, "sync parent directory", error))
}

fn write_error(destination: &std::path::Path, action: &str, error: io::Error) -> String {
    format!(
        "failed to {action} for assertion summary {}: {error}",
        destination.display()
    )
}
