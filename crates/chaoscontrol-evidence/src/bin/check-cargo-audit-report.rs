use std::env;
use std::fs;
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
    Ok((report, allowlist))
}

fn value_after<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn load_json(path: &Path) -> Result<Value, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))?;
    validate_byte_length(
        &format!("invalid JSON in {}", path.display()),
        metadata.len(),
        MAX_AUDIT_JSON_BYTES,
    )?;
    let bytes =
        fs::read(path).map_err(|error| format!("invalid JSON in {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))
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
