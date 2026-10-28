use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::audit::validate_report;
use chaoscontrol_evidence::rust_automation::bounded_input::validate_byte_length;
use serde_json::Value;

const PROGRAM: &str = "check-cargo-audit-report";
const DEFAULT_ALLOWLIST: &str = "audits/cargo-audit-warning-allowlist.json";
const MAX_AUDIT_JSON_BYTES: u64 = 64 * 1_024 * 1_024;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<String, String> {
    if args.as_slice() == ["--selftest"] {
        selftest()?;
        return Ok(String::from("cargo audit policy selftest ok"));
    }
    let (report, allowlist) = parse_args(&args)?;
    let report = load_json(&report)?;
    let allowlist = load_json(&allowlist)?;
    validate_report(&report, &allowlist)
}

fn parse_args(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    let mut report = None;
    let mut allowlist = PathBuf::from(DEFAULT_ALLOWLIST);
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--report" => {
                report = Some(PathBuf::from(value_after(args, index, "--report")?));
                index += 2;
            }
            "--allowlist" => {
                allowlist = PathBuf::from(value_after(args, index, "--allowlist")?);
                index += 2;
            }
            _ => return Err(format!("{PROGRAM}: unknown argument: {}", args[index])),
        }
    }
    let report =
        report.ok_or_else(|| String::from("--report is required unless --selftest is set"))?;
    if report.as_os_str().is_empty() || allowlist.as_os_str().is_empty() {
        return Err(String::from("--report and --allowlist must not be empty"));
    }
    Ok((report, allowlist))
}

fn value_after<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn load_json(path: &Path) -> Result<Value, String> {
    let bytes = read_bounded(path, MAX_AUDIT_JSON_BYTES)?;
    chaoscontrol_evidence::preflight_json_document(&String::from_utf8_lossy(&bytes))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))?;
    validate_byte_length(
        &format!("invalid JSON in {}", path.display()),
        metadata.len(),
        limit,
    )?;
    if !metadata.is_file() {
        return Err(format!(
            "invalid JSON in {}: not a regular file",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "invalid JSON in {}: exceeds the input byte bound",
            path.display()
        ));
    }
    Ok(bytes)
}

fn selftest() -> Result<(), String> {
    let report = serde_json::json!({"vulnerabilities": {"list": []}, "warnings": {}});
    let allowlist = serde_json::json!({"version": 1, "warnings": []});
    validate_report(&report, &allowlist)?;
    let invalid = serde_json::json!({"vulnerabilities": {"list": [{}]}, "warnings": {}});
    if validate_report(&invalid, &allowlist).is_ok() {
        return Err(String::from(
            "selftest negative fixture unexpectedly passed",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn empty_paths_are_rejected() {
        assert!(parse_args(&["--report".to_string(), String::new()]).is_err());
        assert!(parse_args(&[
            "--report".to_string(),
            "report.json".to_string(),
            "--allowlist".to_string(),
            String::new(),
        ])
        .is_err());
    }

    #[test]
    fn report_reads_reject_excessive_nesting() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("audit.json");
        fs::write(&path, format!("{}{}", "[".repeat(512), "]".repeat(512))).unwrap();
        let error = load_json(&path).unwrap_err();
        assert!(error.contains("JSON nesting depth exceeded"));
    }

    #[test]
    fn report_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("audit.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded(&link, MAX_AUDIT_JSON_BYTES).is_err());

        let fifo = root.path().join("audit.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded(&fifo, MAX_AUDIT_JSON_BYTES).is_err());
    }
}
