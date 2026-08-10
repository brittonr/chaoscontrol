use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::bounded_input::validate_byte_length;
use chaoscontrol_evidence::rust_automation::readiness_receipt::{
    build_receipt, GateInput, ReceiptInput, GATE_SPECS,
};
use serde_json::Value;

const MAX_EXPECTATIONS_BYTES: u64 = 16 * 1_024 * 1_024;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    if args.len() != 1 {
        return Err(String::from(
            "usage: materialize-replay-readiness-receipt OUTPUT",
        ));
    }
    let output = PathBuf::from(&args[0]);
    let expectations = load_json(Path::new(&required("DOGFOOD_EXPECTATIONS")?))?;
    let dogfood_summary = optional("DOGFOOD_SUMMARY_JSON")
        .filter(|value| !value.is_empty())
        .map(|value| {
            serde_json::from_str::<Value>(&value)
                .map_err(|error| format!("invalid DOGFOOD_SUMMARY_JSON: {error}"))
        })
        .transpose()?
        .flatten_null();
    if dogfood_summary
        .as_ref()
        .is_some_and(|value| !value.is_object())
    {
        return Err(String::from(
            "DOGFOOD_SUMMARY_JSON must be an object or null",
        ));
    }
    let gates = GATE_SPECS
        .iter()
        .map(|(name, command, variable)| {
            Ok(GateInput {
                name: (*name).to_string(),
                command: (*command).to_string(),
                status: required(variable)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let input = ReceiptInput {
        status: required("STATUS")?,
        failed_phase: optional_nonempty("FAILED_PHASE"),
        exit_code: required("EXIT_CODE")?
            .parse()
            .map_err(|error| format!("EXIT_CODE is invalid: {error}"))?,
        started_at: required("STARTED_AT")?,
        finished_at: required("FINISHED_AT")?,
        dogfood: optional_nonempty("DOGFOOD"),
        dogfood_status: required("DOGFOOD_STATUS")?,
        dogfood_output: optional_nonempty("DOGFOOD_OUTPUT"),
        dogfood_summary,
        gates,
    };
    let receipt = build_receipt(&input, &expectations)?;
    write_atomic(&output, &receipt)
}

trait FlattenNull {
    fn flatten_null(self) -> Option<Value>;
}

impl FlattenNull for Option<Value> {
    fn flatten_null(self) -> Option<Value> {
        self.filter(|value| !value.is_null())
    }
}

fn required(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("{name} is required"))
}

fn optional(name: &str) -> Option<String> {
    env::var(name).ok()
}

fn optional_nonempty(name: &str) -> Option<String> {
    optional(name).filter(|value| !value.is_empty())
}

fn load_json(path: &Path) -> Result<Value, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    validate_byte_length(
        &format!("{}: expectations", path.display()),
        metadata.len(),
        MAX_EXPECTATIONS_BYTES,
    )?;
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))
}

fn write_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    let temporary = PathBuf::from(format!("{}.tmp", path.display()));
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("receipt encode failed: {error}"))?;
    bytes.push(b'\n');
    fs::write(&temporary, bytes).map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| format!("{}: {error}", path.display()))
}
