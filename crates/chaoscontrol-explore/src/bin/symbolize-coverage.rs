//! Bounded symbolized-coverage producer and report tool.

use chaoscontrol_explore::symbolized_coverage::{
    admit_catalog, map_coverage, rank_differential, receipt_for, CoverageObservation, LoadedModule,
    SymbolCatalog, SymbolCoverageLimits, SymbolizedCoverage,
};
use clap::{Parser, Subcommand};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};

const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Parser)]
#[command(about = "Validate catalogs, map coverage, and rank source locations")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate one symbol catalog.
    Validate {
        /// Catalog JSON file.
        catalog: PathBuf,
    },
    /// Map coverage observations through admitted catalogs.
    Map {
        /// JSON array of symbol catalogs.
        catalogs: PathBuf,
        /// JSON array of loaded modules.
        loads: PathBuf,
        /// JSON array of coverage observations.
        observations: PathBuf,
        /// Output report JSON file.
        output: PathBuf,
        /// Optional receipt JSON file.
        #[arg(long)]
        receipt: Option<PathBuf>,
        /// Observation source identity, 64 lowercase hex characters.
        #[arg(long)]
        source_id: Option<String>,
        /// Optional limits JSON file.
        #[arg(long)]
        limits: Option<PathBuf>,
    },
    /// Rank two symbolized reports by descriptive coverage delta.
    Rank {
        /// Failing-branch report JSON.
        failing: PathBuf,
        /// Control-branch report JSON.
        control: PathBuf,
        /// Output ranking JSON file.
        output: PathBuf,
        /// Optional limits JSON file.
        #[arg(long)]
        limits: Option<PathBuf>,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("symbolize-coverage: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { catalog } => {
            let catalog: SymbolCatalog = read_json(&catalog)?;
            admit_catalog(&catalog, &SymbolCoverageLimits::default())
                .map_err(|error| error.to_string())?;
            println!(
                "symbol catalog ok: {} {}",
                catalog.module.path,
                hex(&catalog.module.content_id)
            );
        }
        Command::Map {
            catalogs,
            loads,
            observations,
            output,
            receipt,
            source_id,
            limits,
        } => {
            let limits = match limits {
                Some(path) => read_json(&path)?,
                None => SymbolCoverageLimits::default(),
            };
            let catalogs: Vec<SymbolCatalog> = read_json(&catalogs)?;
            let loads: Vec<LoadedModule> = read_json(&loads)?;
            let observations: Vec<CoverageObservation> = read_json(&observations)?;
            let mapped = map_coverage(&catalogs, &loads, &observations, &limits)
                .map_err(|error| error.to_string())?;
            write_json(&output, &mapped)?;
            if let Some(receipt_path) = receipt {
                let source_id = source_id
                    .as_deref()
                    .ok_or_else(|| "--receipt requires --source-id".to_string())?;
                let source_id = parse_hex_32(source_id)?;
                let receipt = receipt_for(&mapped, &catalogs, &loads, source_id, &limits)
                    .map_err(|error| error.to_string())?;
                write_json(&receipt_path, &receipt)?;
            }
        }
        Command::Rank {
            failing,
            control,
            output,
            limits,
        } => {
            let limits = match limits {
                Some(path) => read_json(&path)?,
                None => SymbolCoverageLimits::default(),
            };
            let failing: SymbolizedCoverage = read_json(&failing)?;
            let control: SymbolizedCoverage = read_json(&control)?;
            let ranked = rank_differential(&failing, &control, &limits)
                .map_err(|error| error.to_string())?;
            write_json(&output, &ranked)?;
        }
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn parse_hex_32(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 {
        return Err("source identity must be 64 hex characters".to_string());
    }
    let mut bytes = [0_u8; 32];
    let encoded = value.as_bytes();
    for index in 0..bytes.len() {
        let high = hex_digit(encoded[index * 2])?;
        let low = hex_digit(encoded[index * 2 + 1])?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn hex_digit(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err("source identity must be lowercase hex".to_string()),
    }
}

fn hex(value: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in value {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
