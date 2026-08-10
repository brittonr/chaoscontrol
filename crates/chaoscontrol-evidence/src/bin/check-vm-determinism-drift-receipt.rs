use std::env;
use std::fs;
use std::path::PathBuf;
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
    let metadata = fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    validate_byte_length(
        &format!("{}: receipt", path.display()),
        metadata.len(),
        MAX_DRIFT_RECEIPT_BYTES,
    )?;
    let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let receipt: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))?;
    validate_drift_receipt(&receipt)
}
