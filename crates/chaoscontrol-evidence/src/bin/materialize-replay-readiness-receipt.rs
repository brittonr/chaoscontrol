use std::env;
use std::fs;
use std::io::{Read, Write};
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
    let bytes = read_bounded(path, MAX_EXPECTATIONS_BYTES)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    validate_byte_length(
        &format!("{}: expectations", path.display()),
        metadata.len(),
        limit,
    )?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    Ok(bytes)
}

fn write_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("receipt encode failed: {error}"))?;
    bytes.push(b'\n');
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| error.to_string())?;
    temporary
        .persist(path)
        .map_err(|error| error.error.to_string())?;
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn input_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("expectations.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded(&link, MAX_EXPECTATIONS_BYTES).is_err());

        let fifo = root.path().join("expectations.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded(&fifo, MAX_EXPECTATIONS_BYTES).is_err());
    }

    #[test]
    fn atomic_receipt_replacement_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"original").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_atomic(&link, &serde_json::json!({"status": "passed"})).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&link).unwrap()).unwrap()["status"],
            "passed"
        );
    }
}
