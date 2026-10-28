use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::bounded_input::validate_byte_length;
use chaoscontrol_evidence::rust_automation::dogfood_summary::{format_line, summarize_values};
use serde_json::Value;

const ACCEPTED_FILE: &str = "accepted-snapshot-verdict-summary.json";
const ATTEMPTS_FILE: &str = "attempts-summary.json";
const USAGE_EXIT: u8 = 2;
const MAX_SUMMARY_BYTES: u64 = 16 * 1_024 * 1_024;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("dogfood summary failed: {error}");
            ExitCode::from(USAGE_EXIT)
        }
    }
}

fn run(args: Vec<String>) -> Result<String, String> {
    let (output, json_mode) = parse_args(&args)?;
    let output = output
        .canonicalize()
        .map_err(|error| format!("{}: {error}", output.display()))?;
    let accepted_path = output.join(ACCEPTED_FILE);
    let attempts_path = output.join(ATTEMPTS_FILE);
    let accepted = read_optional(&accepted_path)?;
    let attempts = read_optional(&attempts_path)?;
    let summary = summarize_values(&output, accepted.as_ref(), attempts.as_ref())?;
    if json_mode {
        serde_json::to_string_pretty(&summary)
            .map_err(|error| format!("summary encode failed: {error}"))
    } else {
        Ok(format_line(&summary))
    }
}

fn parse_args(args: &[String]) -> Result<(PathBuf, bool), String> {
    let mut output = None;
    let mut json_mode = false;
    for arg in args {
        if arg == "--json" {
            json_mode = true;
        } else if arg.starts_with('-') {
            return Err(format!("unknown argument: {arg}"));
        } else if output.replace(PathBuf::from(arg)).is_some() {
            return Err(String::from("expected one output directory"));
        }
    }
    Ok((
        output
            .ok_or_else(|| String::from("accepted-verdict dogfood output directory is required"))?,
        json_mode,
    ))
}

fn read_optional(path: &Path) -> Result<Option<Value>, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    validate_byte_length(
        &format!("{}: summary", path.display()),
        metadata.len(),
        MAX_SUMMARY_BYTES,
    )?;
    let mut bytes = Vec::new();
    file.take(MAX_SUMMARY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_SUMMARY_BYTES {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    if bytes.len() as u64 != metadata.len() {
        return Err(format!("{} changed while reading", path.display()));
    }
    chaoscontrol_evidence::preflight_json_document(&String::from_utf8_lossy(&bytes))
        .map_err(|error| format!("{}: {}", path.display(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))?;
    if !value.is_object() {
        return Err(format!("{}: expected JSON object", path.display()));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reads_reject_length_changes() {
        assert!(read_optional(std::path::Path::new("/proc/self/cmdline")).is_err());
    }

    #[test]
    fn summary_reads_reject_excessive_nesting() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("summary.json");
        std::fs::write(&path, format!("{}{}", "[".repeat(512), "]".repeat(512))).unwrap();
        assert!(read_optional(&path).is_err());
    }
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn optional_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let missing = read_optional(&root.path().join("missing.json")).unwrap();
        assert!(missing.is_none());

        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("summary.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_optional(&link).is_err());

        let fifo = root.path().join("summary.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_optional(&fifo).is_err());
    }
}
