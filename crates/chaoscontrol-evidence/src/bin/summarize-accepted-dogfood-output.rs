use std::env;
use std::fs;

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

fn parse_args(args: &[String]) -> Result<(std::path::PathBuf, bool), String> {
    let mut output = None;
    let mut json_mode = false;
    for arg in args {
        if arg == "--json" {
            json_mode = true;
        } else if arg.starts_with('-') {
            return Err(format!("unknown argument: {arg}"));
        } else if output.replace(std::path::PathBuf::from(arg)).is_some() {
            return Err(String::from("expected one output directory"));
        }
    }
    Ok((
        output
            .ok_or_else(|| String::from("accepted-verdict dogfood output directory is required"))?,
        json_mode,
    ))
}

fn read_optional(path: &std::path::Path) -> Result<Option<Value>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    validate_byte_length(
        &format!("{}: summary", path.display()),
        metadata.len(),
        MAX_SUMMARY_BYTES,
    )?;
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))?;
    if !value.is_object() {
        return Err(format!("{}: expected JSON object", path.display()));
    }
    Ok(Some(value))
}
