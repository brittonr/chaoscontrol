use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use chaoscontrol_evidence::kvm_release::{
    artifact_set_identity, command_identity, matrix_identity, validate_matrix, validate_receipt,
    ArtifactIdentity, Blocker, KvmReleaseMatrix, KvmReleaseReceipt, ReleaseClass, RowReceipt,
    RowStatus, SourceFacts, WorkerFacts, RECEIPT_SCHEMA_VERSION, REQUIRED_WORKER_ARCH,
};
use serde::{Deserialize, Serialize};

const SUCCESS_EXIT_CODE: i32 = 0;
const FAILURE_EXIT_CODE: i32 = 1;
const USAGE_EXIT_CODE: i32 = 2;
const DEFAULT_MATRIX_SOURCE: &str = "contracts/kvm-release/matrix.ncl";
const DEFAULT_MATRIX_PROJECTION: &str = "contracts/kvm-release/matrix.json";
const INVALID_NICKEL_FIXTURE: &str =
    "contracts/kvm-release/fixtures/invalid/missing-timeout.invalid.ncl";
const VALID_RECEIPT_FIXTURE: &str =
    "contracts/kvm-release/fixtures/valid/complete-receipt.valid.json";
const INVALID_RECEIPT_FIXTURES: [&str; 8] = [
    "contracts/kvm-release/fixtures/invalid/missing-row.invalid.json",
    "contracts/kvm-release/fixtures/invalid/stale.invalid.json",
    "contracts/kvm-release/fixtures/invalid/skipped.invalid.json",
    "contracts/kvm-release/fixtures/invalid/unsupported.invalid.json",
    "contracts/kvm-release/fixtures/invalid/timed-out.invalid.json",
    "contracts/kvm-release/fixtures/invalid/tampered.invalid.json",
    "contracts/kvm-release/fixtures/invalid/dirty.invalid.json",
    "contracts/kvm-release/fixtures/invalid/overclaim.invalid.json",
];
const FIXTURE_REVISION: &str = "fixture-revision";
const FIXTURE_RUNNER: &str = "fixture-runner";
const FIXTURE_KERNEL: &str = "fixture-kernel";
const FIXTURE_START_SECONDS: u64 = 100;
const FIXTURE_FINISH_SECONDS: u64 = 101;
const FIXTURE_KVM_API_VERSION: i32 = 12;
const EMPTY_BLAKE3: &str =
    "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

#[derive(Debug)]
struct Config {
    root: PathBuf,
    matrix: PathBuf,
    receipt: Option<PathBuf>,
    expected_revision: Option<String>,
    now_unix_seconds: Option<u64>,
    write_fixtures: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureCase {
    expected_revision: String,
    now_unix_seconds: u64,
    expected_blocker: Option<Blocker>,
    receipt: KvmReleaseReceipt,
}

fn main() {
    let config = match parse_args(std::env::args_os().skip(1).collect()) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}\n{}", usage());
            std::process::exit(USAGE_EXIT_CODE);
        }
    };
    match run(config) {
        Ok(()) => std::process::exit(SUCCESS_EXIT_CODE),
        Err(message) => {
            eprintln!("KVM release matrix check failed: {message}");
            std::process::exit(FAILURE_EXIT_CODE);
        }
    }
}

fn usage() -> &'static str {
    "usage: check-kvm-release-matrix [--root PATH] [--matrix PATH] [--write-fixtures] [--receipt PATH --expected-revision REV [--now UNIX_SECONDS]]"
}

fn parse_args(args: Vec<OsString>) -> Result<Config, String> {
    let mut root = PathBuf::from(".");
    let mut matrix = PathBuf::from(DEFAULT_MATRIX_PROJECTION);
    let mut receipt = None;
    let mut expected_revision = None;
    let mut now_unix_seconds = None;
    let mut write_fixtures = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].to_string_lossy().as_ref() {
            "--root" => {
                root = PathBuf::from(required_value(&args, index, "--root")?);
                index += 2;
            }
            "--matrix" => {
                matrix = PathBuf::from(required_value(&args, index, "--matrix")?);
                index += 2;
            }
            "--receipt" => {
                receipt = Some(PathBuf::from(required_value(&args, index, "--receipt")?));
                index += 2;
            }
            "--expected-revision" => {
                expected_revision = Some(
                    required_value(&args, index, "--expected-revision")?
                        .to_string_lossy()
                        .into_owned(),
                );
                index += 2;
            }
            "--now" => {
                now_unix_seconds = Some(
                    required_value(&args, index, "--now")?
                        .to_string_lossy()
                        .parse::<u64>()
                        .map_err(|error| format!("--now must be an unsigned integer: {error}"))?,
                );
                index += 2;
            }
            "--write-fixtures" => {
                write_fixtures = true;
                index += 1;
            }
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(SUCCESS_EXIT_CODE);
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    if receipt.is_some() != expected_revision.is_some() {
        return Err("--receipt and --expected-revision must be supplied together".to_string());
    }
    Ok(Config {
        root,
        matrix,
        receipt,
        expected_revision,
        now_unix_seconds,
        write_fixtures,
    })
}

fn required_value<'a>(
    args: &'a [OsString],
    index: usize,
    option: &str,
) -> Result<&'a OsString, String> {
    args.get(index + 1)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{option} requires a non-empty value"))
}

fn run(config: Config) -> Result<(), String> {
    let root = fs::canonicalize(&config.root)
        .map_err(|error| format!("failed to resolve root: {error}"))?;
    let matrix_path = resolve_path(&root, &config.matrix);
    let matrix = read_json::<KvmReleaseMatrix>(&matrix_path)?;
    validate_matrix(&matrix).map_err(|issues| issues.join("; "))?;

    if config.write_fixtures {
        write_fixture_cases(&root, &matrix)?;
    }
    check_projection(&root, &matrix)?;
    check_fixture_cases(&root, &matrix)?;

    if let (Some(receipt_path), Some(expected_revision)) =
        (config.receipt.as_ref(), config.expected_revision.as_deref())
    {
        let receipt = read_json::<KvmReleaseReceipt>(&resolve_path(&root, receipt_path))?;
        let now = match config.now_unix_seconds {
            Some(now) => now,
            None => unix_seconds()?,
        };
        let decision = validate_receipt(&matrix, expected_revision, &receipt, now);
        if decision.terminal_class != ReleaseClass::ReleaseEligible {
            return Err(format!(
                "receipt is blocked: {}",
                decision.reasons.join("; ")
            ));
        }
    }

    println!(
        "KVM release matrix ok: profile={} rows={} positive=1 negative={}",
        matrix.profile_id,
        matrix.rows.len(),
        INVALID_RECEIPT_FIXTURES.len() + 1
    );
    Ok(())
}

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn check_projection(root: &Path, matrix: &KvmReleaseMatrix) -> Result<(), String> {
    let output = nickel_export(root, Path::new(DEFAULT_MATRIX_SOURCE), true)?;
    let exported = serde_json::from_slice::<KvmReleaseMatrix>(&output)
        .map_err(|error| format!("Nickel matrix projection is invalid JSON: {error}"))?;
    if exported != *matrix {
        return Err(format!(
            "stale matrix projection: {DEFAULT_MATRIX_PROJECTION}"
        ));
    }
    let negative_status = nickel_status(root, Path::new(INVALID_NICKEL_FIXTURE))?;
    if negative_status.success() {
        return Err(format!(
            "negative Nickel fixture unexpectedly passed: {INVALID_NICKEL_FIXTURE}"
        ));
    }
    Ok(())
}

fn nickel_export(root: &Path, source: &Path, json: bool) -> Result<Vec<u8>, String> {
    let mut command = nickel_command();
    if json {
        command.args(["--format", "json"]);
    }
    let output = command
        .arg(root.join(source))
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to export {}: {error}", source.display()))?;
    if !output.status.success() {
        return Err(format!(
            "Nickel export failed for {}: {}",
            source.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

fn nickel_status(root: &Path, source: &Path) -> Result<std::process::ExitStatus, String> {
    nickel_command()
        .arg(root.join(source))
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("failed to export {}: {error}", source.display()))
}

fn nickel_command() -> Command {
    if command_exists("nickel") {
        let mut command = Command::new("nickel");
        command.arg("export");
        command
    } else {
        let mut command = Command::new("nix");
        command.args(["run", "nixpkgs#nickel", "--", "export"]);
        command
    }
}

fn command_exists(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|directory| directory.join(name).is_file())
}

fn check_fixture_cases(root: &Path, matrix: &KvmReleaseMatrix) -> Result<(), String> {
    let valid = read_json::<FixtureCase>(&root.join(VALID_RECEIPT_FIXTURE))?;
    let valid_decision = validate_receipt(
        matrix,
        &valid.expected_revision,
        &valid.receipt,
        valid.now_unix_seconds,
    );
    if valid.expected_blocker.is_some()
        || valid_decision.terminal_class != ReleaseClass::ReleaseEligible
    {
        return Err(format!(
            "valid receipt fixture is blocked: {}",
            valid_decision.reasons.join("; ")
        ));
    }

    for path in INVALID_RECEIPT_FIXTURES {
        let fixture = read_json::<FixtureCase>(&root.join(path))?;
        let expected = fixture
            .expected_blocker
            .ok_or_else(|| format!("invalid fixture lacks expected blocker: {path}"))?;
        let decision = validate_receipt(
            matrix,
            &fixture.expected_revision,
            &fixture.receipt,
            fixture.now_unix_seconds,
        );
        if decision.terminal_class != ReleaseClass::Blocked
            || !decision.blockers.contains(&expected)
        {
            return Err(format!(
                "invalid fixture did not produce {:?}: {path}; blockers={:?}",
                expected, decision.blockers
            ));
        }
    }
    Ok(())
}

// Fixture generation is an imperative shell around the same pure classifier.
fn write_fixture_cases(root: &Path, matrix: &KvmReleaseMatrix) -> Result<(), String> {
    let valid = fixture_case(matrix);
    write_json(root.join(VALID_RECEIPT_FIXTURE), &valid)?;

    let mut missing = valid.clone();
    missing.receipt.rows.pop();
    set_blocked(&mut missing, Blocker::MissingRow);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[0]), &missing)?;

    let mut stale = valid.clone();
    stale.now_unix_seconds =
        stale.receipt.finished_unix_seconds + matrix.max_receipt_age_seconds + 1;
    set_blocked(&mut stale, Blocker::StaleReceipt);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[1]), &stale)?;

    let mut skipped = valid.clone();
    skipped.receipt.rows[0].status = RowStatus::Skipped;
    set_blocked(&mut skipped, Blocker::RowNotPassed);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[2]), &skipped)?;

    let mut unsupported = valid.clone();
    unsupported.receipt.rows[0].status = RowStatus::Unsupported;
    set_blocked(&mut unsupported, Blocker::RowNotPassed);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[3]), &unsupported)?;

    let mut timed_out = valid.clone();
    timed_out.receipt.rows[0].status = RowStatus::TimedOut;
    set_blocked(&mut timed_out, Blocker::RowNotPassed);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[4]), &timed_out)?;

    let mut tampered = valid.clone();
    tampered.receipt.rows[0].artifacts[0].bytes = 1;
    set_blocked(&mut tampered, Blocker::ArtifactSetMismatch);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[5]), &tampered)?;

    let mut dirty = valid.clone();
    dirty.receipt.source.dirty = true;
    set_blocked(&mut dirty, Blocker::DirtySource);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[6]), &dirty)?;

    let mut overclaim = valid;
    overclaim.receipt.bounded_claim = "This proves universal determinism.".to_string();
    set_blocked(&mut overclaim, Blocker::Overclaim);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[7]), &overclaim)
}

fn fixture_case(matrix: &KvmReleaseMatrix) -> FixtureCase {
    let rows = matrix
        .rows
        .iter()
        .map(|row| {
            let artifacts = vec![ArtifactIdentity {
                path: "stdout.log".to_string(),
                bytes: 0,
                blake3: EMPTY_BLAKE3.to_string(),
            }];
            let mut executed_argv = Vec::with_capacity(row.command.args.len() + 1);
            executed_argv.push(row.command.program.clone());
            executed_argv.extend(row.command.args.iter().cloned());
            RowReceipt {
                id: row.id.clone(),
                kind: row.kind,
                required_capabilities: row.required_capabilities.clone(),
                command: row.command.clone(),
                executed_argv,
                command_identity: command_identity(&row.command),
                started_unix_seconds: FIXTURE_START_SECONDS,
                finished_unix_seconds: FIXTURE_FINISH_SECONDS,
                status: RowStatus::Passed,
                exit_code: Some(0),
                artifact_set_identity: artifact_set_identity(&artifacts),
                artifacts,
                notes: Vec::new(),
            }
        })
        .collect();
    FixtureCase {
        expected_revision: FIXTURE_REVISION.to_string(),
        now_unix_seconds: FIXTURE_FINISH_SECONDS,
        expected_blocker: None,
        receipt: KvmReleaseReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            matrix_profile: matrix.profile_id.clone(),
            matrix_identity: matrix_identity(matrix),
            source: SourceFacts {
                revision: FIXTURE_REVISION.to_string(),
                dirty: false,
            },
            runner_revision: FIXTURE_RUNNER.to_string(),
            worker: WorkerFacts {
                architecture: REQUIRED_WORKER_ARCH.to_string(),
                kernel_release: FIXTURE_KERNEL.to_string(),
                kvm_api_version: Some(FIXTURE_KVM_API_VERSION),
                capabilities: matrix.required_worker_capabilities.clone(),
            },
            started_unix_seconds: FIXTURE_START_SECONDS,
            finished_unix_seconds: FIXTURE_FINISH_SECONDS,
            rows,
            bounded_claim: matrix.bounded_claim.clone(),
            non_claims: matrix.non_claims.clone(),
            terminal_class: ReleaseClass::ReleaseEligible,
        },
    }
}

fn set_blocked(fixture: &mut FixtureCase, blocker: Blocker) {
    fixture.expected_blocker = Some(blocker);
    fixture.receipt.terminal_class = ReleaseClass::Blocked;
}

fn write_json(path: PathBuf, value: &impl Serialize) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("fixture path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create fixture directory: {error}"))?;
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize fixture: {error}"))?;
    bytes.push(b'\n');
    fs::write(&path, bytes)
        .map_err(|error| format!("failed to write fixture {}: {error}", path.display()))
}

fn unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))
}
