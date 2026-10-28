use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::bounded_input::validate_byte_length;
use chaoscontrol_evidence::rust_automation::vm_determinism::matrix_summary;
use serde_json::Value;

const MAX_MATRIX_RECEIPT_BYTES: u64 = 16 * 1_024 * 1_024;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<String, String> {
    if args.len() != 2 {
        return Err(String::from(
            "usage: render-vm-determinism-matrix-summary RECEIPT OUTPUT",
        ));
    }
    let receipt_path = PathBuf::from(&args[0]);
    let output_path = PathBuf::from(&args[1]);
    let bytes = read_bounded(&receipt_path, MAX_MATRIX_RECEIPT_BYTES)?;
    let receipt: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", receipt_path.display()))?;
    let summary = matrix_summary(&receipt)?;
    write_summary(&output_path, format!("{summary}\n").as_bytes())
        .map_err(|error| format!("{}: {error}", output_path.display()))?;
    Ok(summary)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    validate_byte_length(
        &format!("{}: receipt", path.display()),
        metadata.len(),
        limit,
    )?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    Ok(bytes)
}

fn write_summary(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn receipt_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded(&link, MAX_MATRIX_RECEIPT_BYTES).is_err());

        let fifo = root.path().join("receipt.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded(&fifo, MAX_MATRIX_RECEIPT_BYTES).is_err());
    }

    #[test]
    fn summary_writer_rejects_symlink_targets() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.md");
        fs::write(&target, b"original").unwrap();
        let link = root.path().join("summary.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(write_summary(&link, b"summary").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
    }
}
