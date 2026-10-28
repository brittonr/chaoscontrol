use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::bounded_input::validate_byte_length;
use chaoscontrol_evidence::rust_automation::vm_determinism::validate_drift_receipt;
use serde_json::Value;

const MAX_DRIFT_RECEIPT_BYTES: u64 = 16 * 1_024 * 1_024;

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
    if args.len() != 1 {
        return Err(String::from(
            "usage: check-vm-determinism-drift-receipt RECEIPT",
        ));
    }
    let path = PathBuf::from(&args[0]);
    if path.as_os_str().is_empty() {
        return Err(String::from("receipt path must not be empty"));
    }
    let bytes = read_bounded(&path, MAX_DRIFT_RECEIPT_BYTES)?;
    chaoscontrol_evidence::preflight_json_document(&String::from_utf8_lossy(&bytes))
        .map_err(|error| format!("{}: {}", path.display(), error))?;
    let receipt: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))?;
    validate_drift_receipt(&receipt)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn empty_receipt_path_is_rejected() {
        assert!(run(vec![String::new()]).is_err());
    }

    #[test]
    fn receipt_reads_reject_excessive_nesting() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("receipt.json");
        fs::write(&path, format!("{}{}", "[".repeat(512), "]".repeat(512))).unwrap();
        let error = run(vec![path.display().to_string()]).unwrap_err();
        assert!(error.contains("JSON nesting depth exceeded"));
    }

    #[test]
    fn receipt_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded(&link, MAX_DRIFT_RECEIPT_BYTES).is_err());

        let fifo = root.path().join("receipt.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded(&fifo, MAX_DRIFT_RECEIPT_BYTES).is_err());
    }
}
