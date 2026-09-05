use std::env;
use std::fs;
use std::io::{BufReader, Read};

use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::dogfood_receipt::{
    build_receipt, build_run_config, encode_run_config_compat, ArtifactFact, MaterializeInput,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const DEFAULT_STATUS: &str = "known-gap";
const DEFAULT_MESSAGE: &str = "Bug NOT reproduced — assertion 1205943209 did not fail";
const DEFAULT_EXIT_STATUS: i64 = 1;
const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
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
    output: std::path::PathBuf,
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
    fs::write(&run_config_path, encode_run_config_compat(&run_config)?)
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
    write_json(
        &options.output.join("receipt.json"),
        &build_receipt(&input)?,
    )
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let output = args
        .first()
        .filter(|value| !value.starts_with('-'))
        .map(std::path::PathBuf::from)
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

fn load_json(path: &std::path::Path) -> Result<Value, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.len() > MAX_JSON_BYTES {
        return Err(format!("{} exceeds JSON byte bound", path.display()));
    }
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))
}

fn write_json(path: &std::path::Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("{}: encode failed: {error}", path.display()))?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn matching_files(
    root: &std::path::Path,
    prefix: &str,
    suffix: &str,
) -> Result<Vec<std::path::PathBuf>, String> {
    let mut paths = fs::read_dir(root)
        .map_err(|error| format!("{}: {error}", root.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn sha256(path: &std::path::Path) -> Result<String, String> {
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
