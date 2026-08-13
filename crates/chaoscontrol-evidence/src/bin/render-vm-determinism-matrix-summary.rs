use std::env;
use std::fs;
use std::path::PathBuf;
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
    let metadata = fs::metadata(&receipt_path)
        .map_err(|error| format!("{}: {error}", receipt_path.display()))?;
    validate_byte_length(
        &format!("{}: receipt", receipt_path.display()),
        metadata.len(),
        MAX_MATRIX_RECEIPT_BYTES,
    )?;
    let bytes =
        fs::read(&receipt_path).map_err(|error| format!("{}: {error}", receipt_path.display()))?;
    let receipt: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", receipt_path.display()))?;
    let summary = matrix_summary(&receipt)?;
    fs::write(&output_path, format!("{summary}\n"))
        .map_err(|error| format!("{}: {error}", output_path.display()))?;
    Ok(summary)
}
