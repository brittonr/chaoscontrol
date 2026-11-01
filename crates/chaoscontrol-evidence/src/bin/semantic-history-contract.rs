use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::Path;

use chaoscontrol_evidence::semantic_history::contract::{self, ImportedHistory};
use chaoscontrol_evidence::{history_digest, validate_consistency_history, OperationHistory};

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

fn read(path: &str) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err("input must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err("input exceeds 16 MiB contract bound".into());
    }
    Ok(bytes)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("semantic-history-contract: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command] if command == "selftest" => {
            contract::selftest()?;
            println!("semantic-history contract mutation corpus passed");
        }
        [command, directory] if command == "fixtures" => fixtures(Path::new(directory))?,
        [command, input, output] if command == "import" || command == "check" => {
            let history = contract::import_json(&read(input)?)?;
            let bytes = if command == "import" {
                contract::export_json(&history)?
            } else {
                contract::check_json(&history)?
            };
            fs::write(output, &bytes)?;
            let readback = read(output)?;
            if command == "import" {
                if contract::import_json(&readback)? != history {
                    return Err("history changed on persisted readback".into());
                }
            } else {
                contract::validate_json(&history, &readback)?;
            }
        }
        [command, input, report] if command == "validate" => {
            let history = contract::import_json(&read(input)?)?;
            contract::validate_json(&history, &read(report)?)?;
            println!("source-bound semantic report validated");
        }
        _ => return Err("usage: semantic-history-contract selftest | fixtures DIR | import INPUT OUTPUT | check HISTORY REPORT | validate HISTORY REPORT".into()),
    }
    Ok(())
}

fn fixtures(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)?;
    for (name, history) in contract::fixtures() {
        let history = ImportedHistory::Linearizability(history);
        fs::write(
            directory.join(format!("{name}-history.json")),
            contract::export_json(&history)?,
        )?;
        let report = contract::check_json(&history)?;
        contract::validate_json(&history, &report)?;
        fs::write(directory.join(format!("{name}-report.json")), report)?;
    }
    let legacy: OperationHistory = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "history_id": "semantic-contract-legacy",
        "workload": "synthetic-register", "model": "single-register-sequential",
        "source_artifact": "synthetic-contract-source", "limitations": ["synthetic checker input"],
        "operations": [{"operation_id": "read", "process": "client", "invoked_at": 1,
            "completed_at": 2, "invocation": {"kind": "read"},
            "completion": {"kind": "ok", "value": null}}]
    }))?;
    validate_consistency_history(&legacy)?;
    let digest = history_digest(&legacy)?;
    let imported = ImportedHistory::CompletionOrder(legacy);
    let bytes = contract::export_json(&imported)?;
    let readback = contract::import_json(&bytes)?;
    if let ImportedHistory::CompletionOrder(history) = &readback {
        if history_digest(history)? != digest {
            return Err("legacy digest changed during readback".into());
        }
    } else {
        return Err("legacy history promoted during readback".into());
    }
    let report = contract::check_json(&readback)?;
    contract::validate_json(&readback, &report)?;
    fs::write(directory.join("legacy-history.json"), bytes)?;
    fs::write(directory.join("legacy-report.json"), report)?;
    fs::write(directory.join("legacy-digest.txt"), format!("{digest}\n"))?;
    println!("synthetic fixtures written; no guest execution or replay claim");
    Ok(())
}
