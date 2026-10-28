use std::env;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::frontier_policy_gate::{admit_output_directory, ReleaseGate};
use chaoscontrol_evidence::rust_automation::dogfood_receipt::{
    build_receipt, build_run_config, encode_run_config_compat, ArtifactFact, MaterializeInput,
};
use chaoscontrol_evidence::validate_receipt_evidence_boundary;
use serde_json::Value;
use sha2::{Digest, Sha256};

const DEFAULT_STATUS: &str = "known-gap";
const DEFAULT_MESSAGE: &str = "Bug NOT reproduced — assertion 1205943209 did not fail";
const DEFAULT_EXIT_STATUS: i64 = 1;
const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
const MAX_OUTPUT_DIRECTORY_ENTRIES: usize = 4_096;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const ACCEPTED_STATUSES: [&str; 5] = [
    "accepted",
    "partial",
    "known-gap",
    "invalid",
    "raw-log-only",
];
const FIXED_ARTIFACTS: [&str; 5] = [
    "report.txt",
    "checkpoint.json",
    "assertions.json",
    "receipt.md",
    "run-config.json",
];

#[derive(Debug)]
struct Options {
    output: PathBuf,
    git_revision: String,
    replay_status: String,
    replay_message: String,
    replay_exit_status: i64,
    replay_command: Option<String>,
}

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
    let options = parse_args(&args)?;
    // r[impl chaoscontrol.campaign_policy.rollback]
    // Legacy-diagnostic rollback outputs never become dogfood evidence; refuse
    // before writing anything into the output directory.
    admit_output_directory(ReleaseGate::DogfoodReceipt, &options.output)
        .map_err(|error| error.to_string())?;
    let checkpoint_path = options.output.join("checkpoint.json");
    let assertions_path = options.output.join("assertions.json");
    let checkpoint = load_json(&checkpoint_path)?;
    let assertions = load_json(&assertions_path)?;
    let output_name = options
        .output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| String::from("output has no UTF-8 name"))?
        .to_string();
    let run_config = build_run_config(&output_name, &checkpoint)?;
    let run_config_path = options.output.join("run-config.json");
    write_atomic_bytes(&run_config_path, &encode_run_config_compat(&run_config)?)
        .map_err(|error| format!("{}: {error}", run_config_path.display()))?;

    let replay_verdicts = matching_files(&options.output, "replay-verdict", ".json")?;
    let bugs = matching_files(&options.output, "bug_", ".json")?;
    let mut artifact_paths = FIXED_ARTIFACTS
        .iter()
        .map(|name| options.output.join(name))
        .collect::<Vec<_>>();
    artifact_paths.extend(replay_verdicts);
    artifact_paths.extend(bugs.iter().cloned());
    let artifacts = artifact_paths
        .iter()
        .map(|path| {
            Ok(ArtifactFact {
                path: path.display().to_string(),
                sha256: sha256(path)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bug_values = bugs
        .iter()
        .map(|path| load_json(path))
        .collect::<Result<Vec<_>, String>>()?;
    let bug_paths = bugs
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let input = MaterializeInput {
        output_name,
        output_path: options.output.display().to_string(),
        git_revision: options.git_revision,
        replay_status: options.replay_status,
        replay_message: options.replay_message,
        replay_exit_status: options.replay_exit_status,
        replay_command: options.replay_command,
        checkpoint,
        assertions,
        bugs: bug_values,
        bug_paths,
        artifacts,
        run_config_digest: sha256(&run_config_path)?,
        checkpoint_digest: sha256(&checkpoint_path)?,
    };
    let receipt = build_receipt(&input)?;
    validate_receipt_evidence_boundary(&receipt).map_err(|error| error.to_string())?;
    write_json(&options.output.join("receipt.json"), &receipt)
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let output = args
        .first()
        .filter(|value| !value.starts_with('-'))
        .map(PathBuf::from)
        .ok_or_else(|| String::from("output directory is required"))?;
    let mut git_revision = None;
    let mut replay_status = String::from(DEFAULT_STATUS);
    let mut replay_message = String::from(DEFAULT_MESSAGE);
    let mut replay_exit_status = DEFAULT_EXIT_STATUS;
    let mut replay_command = None;
    let mut index = 1;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{} requires a value", args[index]))?;
        match args[index].as_str() {
            "--git-revision" => git_revision = Some(value.clone()),
            "--replay-status" => replay_status = value.clone(),
            "--replay-message" => replay_message = value.clone(),
            "--replay-exit-status" => {
                replay_exit_status = value
                    .parse()
                    .map_err(|error| format!("invalid replay exit status: {error}"))?
            }
            "--replay-command" => replay_command = Some(value.clone()),
            _ => return Err(format!("unknown argument: {}", args[index])),
        }
        index += 2;
    }
    if !ACCEPTED_STATUSES.contains(&replay_status.as_str()) {
        return Err(format!("unsupported replay status: {replay_status}"));
    }
    Ok(Options {
        output,
        git_revision: git_revision.ok_or_else(|| String::from("--git-revision is required"))?,
        replay_status,
        replay_message,
        replay_exit_status,
        replay_command,
    })
}

fn load_json(path: &Path) -> Result<Value, String> {
    let bytes = read_bounded(path, MAX_JSON_BYTES)?;
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
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!("{} is not a bounded regular file", path.display()));
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

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("{}: encode failed: {error}", path.display()))?;
    bytes.push(b'\n');
    write_atomic_bytes(path, &bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = chaoscontrol_evidence::bounded_file::output_parent(path);
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    fs::File::open(parent)?.sync_all()
}

fn matching_files(root: &Path, prefix: &str, suffix: &str) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    let mut discovered = 0_usize;
    for entry in fs::read_dir(root).map_err(|error| format!("{}: {error}", root.display()))? {
        discovered += 1;
        if discovered > MAX_OUTPUT_DIRECTORY_ENTRIES {
            return Err(format!(
                "{}: output directory entry bound exceeded",
                root.display()
            ));
        }
        let entry = entry.map_err(|error| format!("{}: {error}", root.display()))?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn sha256(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn output_directory_scan_is_bounded() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..=MAX_OUTPUT_DIRECTORY_ENTRIES {
            fs::write(root.path().join(format!("bug_{index}.json")), b"{}").unwrap();
        }

        assert!(matching_files(root.path(), "bug_", ".json").is_err());
    }

    #[test]
    fn input_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("checkpoint.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded(&link, MAX_JSON_BYTES).is_err());

        let fifo = root.path().join("checkpoint.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded(&fifo, MAX_JSON_BYTES).is_err());
    }

    #[test]
    fn atomic_output_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"original").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_atomic_bytes(&link, b"replacement").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read(&link).unwrap(), b"replacement");
    }
}
