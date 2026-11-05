//! Read-only investigation request runner.
//!
//! Reads one versioned JSON request, executes it against a retained checkpoint,
//! and writes one JSON response. It never executes a VM or writes artifacts.

use chaoscontrol_explore::investigation::{
    decode_request, run_request, InvestigationFailure, MAX_REQUEST_BYTES,
};
use std::io::Read;
use std::process::ExitCode;

const EXIT_REQUEST: u8 = 2;
const EXIT_OPERATIONAL: u8 = 3;

fn usage() -> ExitCode {
    eprintln!("usage: chaoscontrol-investigate --request <path|-> [--output <path>]");
    ExitCode::from(64)
}

fn read_request(path: &str) -> Result<Vec<u8>, InvestigationFailure> {
    let mut bytes = Vec::new();
    let limit = u64::try_from(MAX_REQUEST_BYTES).unwrap_or(u64::MAX) + 1;
    if path == "-" {
        std::io::stdin()
            .take(limit)
            .read_to_end(&mut bytes)
            .map_err(|error| InvestigationFailure::new("io", format!("read stdin: {error}")))?;
    } else {
        let file = std::fs::File::open(path)
            .map_err(|error| InvestigationFailure::new("io", format!("open request: {error}")))?;
        file.take(limit)
            .read_to_end(&mut bytes)
            .map_err(|error| InvestigationFailure::new("io", format!("read request: {error}")))?;
    }
    Ok(bytes)
}

fn write_document<T: serde::Serialize>(value: &T, output: Option<&str>) -> Result<(), String> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|error| format!("encode response: {error}"))?;
    bytes.push(b'\n');
    match output {
        Some(path) => {
            std::fs::write(path, bytes).map_err(|error| format!("write response {path}: {error}"))
        }
        None => {
            use std::io::Write;
            std::io::stdout()
                .write_all(&bytes)
                .and_then(|()| std::io::stdout().flush())
                .map_err(|error| format!("write stdout: {error}"))
        }
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut request_path = None;
    let mut output = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--request" => request_path = args.next(),
            "--output" => output = args.next(),
            _ => return usage(),
        }
    }
    let Some(request_path) = request_path else {
        return usage();
    };

    let bytes = match read_request(&request_path) {
        Ok(bytes) => bytes,
        Err(failure) => {
            let _ = write_document(&failure, output.as_deref());
            return ExitCode::from(EXIT_OPERATIONAL);
        }
    };
    let request = match decode_request(&bytes) {
        Ok(request) => request,
        Err(failure) => {
            let _ = write_document(&failure, output.as_deref());
            return ExitCode::from(EXIT_REQUEST);
        }
    };
    match run_request(&request) {
        Ok(response) => match write_document(&response, output.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(EXIT_OPERATIONAL)
            }
        },
        Err(failure) => {
            let _ = write_document(&failure, output.as_deref());
            ExitCode::from(EXIT_OPERATIONAL)
        }
    }
}
