use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::bounded_input::validate_byte_length;
use chaoscontrol_evidence::rust_automation::source_guard::validate;

const MAX_ENTRIES: usize = 4_096;
const MAX_FLAKE_BYTES: u64 = 4 * 1_024 * 1_024;

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
            "usage: check-rust-product-automation-source ROOT",
        ));
    }
    let root = PathBuf::from(&args[0]);
    let scripts = list_files(&root.join("scripts"))?;
    let bins = list_files(&root.join("crates/chaoscontrol-evidence/src/bin"))?;
    let flake_path = root.join("flake.nix");
    let flake_metadata =
        fs::metadata(&flake_path).map_err(|error| format!("flake.nix: {error}"))?;
    validate_byte_length("flake.nix", flake_metadata.len(), MAX_FLAKE_BYTES)?;
    let flake = fs::read_to_string(&flake_path).map_err(|error| format!("flake.nix: {error}"))?;
    validate(&scripts, &flake, &bins)?;
    Ok(format!(
        "Rust product automation source guard ok: scripts={} rust_bins={}",
        scripts.len(),
        bins.len()
    ))
}

fn list_files(root: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        for entry in entries {
            if files.len() + pending.len() >= MAX_ENTRIES {
                return Err(String::from("source guard entry bound exceeded"));
            }
            let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                files.push(entry.path().display().to_string());
            }
        }
    }
    files.sort();
    Ok(files)
}
