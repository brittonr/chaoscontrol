use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use chaoscontrol_evidence::kvm_release::{
    artifact_set_identity, command_identity, matrix_identity, validate_matrix, validate_receipt,
    ArtifactIdentity, Blocker, KvmReleaseMatrix, KvmReleaseReceipt, ReleaseClass, RowKind,
    RowReceipt, RowStatus, SourceFacts, WorkerFacts, RECEIPT_SCHEMA_VERSION, REQUIRED_WORKER_ARCH,
};
use serde::{Deserialize, Serialize};

// The portable and KVM workflow lanes call this checker separately.
// r[impl chaoscontrol.kvm_release_rail.ci]
const SUCCESS_EXIT_CODE: i32 = 0;
const FAILURE_EXIT_CODE: i32 = 1;
const USAGE_EXIT_CODE: i32 = 2;
const DEFAULT_MATRIX_SOURCE: &str = "contracts/kvm-release/matrix.ncl";
const DEFAULT_MATRIX_PROJECTION: &str = "contracts/kvm-release/matrix.json";
const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_EXPECTED_REVISION_BYTES: usize = 256;
const INVALID_NICKEL_FIXTURES: [&str; 2] = [
    "contracts/kvm-release/fixtures/invalid/missing-timeout.invalid.ncl",
    "contracts/kvm-release/fixtures/invalid/legacy-diagnostic-row.invalid.ncl",
];
const VALID_RECEIPT_FIXTURE: &str =
    "contracts/kvm-release/fixtures/valid/complete-receipt.valid.json";
const INVALID_RECEIPT_FIXTURES: [&str; 9] = [
    "contracts/kvm-release/fixtures/invalid/missing-row.invalid.json",
    "contracts/kvm-release/fixtures/invalid/stale.invalid.json",
    "contracts/kvm-release/fixtures/invalid/skipped.invalid.json",
    "contracts/kvm-release/fixtures/invalid/unsupported.invalid.json",
    "contracts/kvm-release/fixtures/invalid/timed-out.invalid.json",
    "contracts/kvm-release/fixtures/invalid/tampered.invalid.json",
    "contracts/kvm-release/fixtures/invalid/dirty.invalid.json",
    "contracts/kvm-release/fixtures/invalid/overclaim.invalid.json",
    "contracts/kvm-release/fixtures/invalid/legacy-diagnostic-output.invalid.json",
];
/// Diagnostic receipt a legacy-diagnostic exploration leaves in its output.
const LEGACY_DIAGNOSTIC_ARTIFACT: &str = "fresh/attempts/attempt-01/frontier-policy.json";
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
    curated_output: Option<PathBuf>,
    write_fixtures: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureCase {
    expected_revision: String,
    now_unix_seconds: u64,
    expected_blocker: Option<Blocker>,
    receipt: KvmReleaseReceipt,
}

#[derive(Debug, Serialize)]
struct CuratedValidationReceipt {
    schema: &'static str,
    source: SourceFacts,
    matrix_profile: String,
    matrix_identity: String,
    runner_revision: String,
    worker: WorkerFacts,
    started_unix_seconds: u64,
    finished_unix_seconds: u64,
    checked_unix_seconds: u64,
    terminal_class: ReleaseClass,
    rows: Vec<CuratedRow>,
    full_receipt_identity: String,
    raw_receipt_retention: &'static str,
    bounded_claim: String,
    non_claims: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CuratedRow {
    id: String,
    kind: RowKind,
    status: RowStatus,
    command_identity: String,
    artifact_set_identity: String,
    artifact_count: usize,
    artifact_bytes: u64,
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
    "usage: check-kvm-release-matrix [--root PATH] [--matrix PATH] [--write-fixtures] [--receipt PATH --expected-revision REV [--now UNIX_SECONDS] [--curated-out PATH]]"
}

fn parse_args(args: Vec<OsString>) -> Result<Config, String> {
    let mut root = PathBuf::from(".");
    let mut matrix = PathBuf::from(DEFAULT_MATRIX_PROJECTION);
    let mut receipt = None;
    let mut expected_revision = None;
    let mut now_unix_seconds = None;
    let mut curated_output = None;
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
            "--curated-out" => {
                curated_output = Some(PathBuf::from(required_value(
                    &args,
                    index,
                    "--curated-out",
                )?));
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
    if root.as_os_str().is_empty() || matrix.as_os_str().is_empty() {
        return Err("--root and --matrix must not be empty".to_string());
    }
    if receipt
        .as_ref()
        .is_some_and(|path| path.as_os_str().is_empty())
        || curated_output
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
    {
        return Err("--receipt and --curated-out must not be empty".to_string());
    }
    if expected_revision.as_deref().is_some_and(|revision| {
        revision.is_empty()
            || revision.len() > MAX_EXPECTED_REVISION_BYTES
            || revision.chars().any(char::is_control)
    }) {
        return Err("--expected-revision is empty, oversized, or malformed".to_string());
    }
    if receipt.is_some() != expected_revision.is_some() {
        return Err("--receipt and --expected-revision must be supplied together".to_string());
    }
    if curated_output.is_some() && receipt.is_none() {
        return Err("--curated-out requires --receipt and --expected-revision".to_string());
    }
    Ok(Config {
        root,
        matrix,
        receipt,
        expected_revision,
        now_unix_seconds,
        curated_output,
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
        let resolved_receipt = resolve_path(&root, receipt_path);
        let receipt = read_json::<KvmReleaseReceipt>(&resolved_receipt)?;
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
        if let Some(curated_output) = config.curated_output.as_ref() {
            write_curated_receipt(
                &resolve_path(&root, curated_output),
                &resolved_receipt,
                &receipt,
                now,
            )?;
        }
    }

    println!(
        "KVM release matrix ok: profile={} rows={} positive=1 negative={}",
        matrix.profile_id,
        matrix.rows.len(),
        INVALID_RECEIPT_FIXTURES.len() + INVALID_NICKEL_FIXTURES.len()
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
    let bytes = read_bytes(path, MAX_INPUT_BYTES)?;
    chaoscontrol_evidence::preflight_json_document(&String::from_utf8_lossy(&bytes))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn read_bytes(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(format!("{} is not a bounded regular file", path.display()));
    }
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(usize::MAX)
        .min(usize::try_from(limit).unwrap_or(usize::MAX));
    let mut bytes = Vec::with_capacity(capacity);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds the input byte bound", path.display()));
    }
    Ok(bytes)
}

fn check_projection(root: &Path, matrix: &KvmReleaseMatrix) -> Result<(), String> {
    chaoscontrol_evidence::profile_projection::admit_nickel_evaluator()
        .map_err(|error| error.to_string())?;
    let output = nickel_export(root, Path::new(DEFAULT_MATRIX_SOURCE), true)?;
    let exported = serde_json::from_slice::<KvmReleaseMatrix>(&output)
        .map_err(|error| format!("Nickel matrix projection is invalid JSON: {error}"))?;
    if exported != *matrix {
        return Err(format!(
            "stale matrix projection: {DEFAULT_MATRIX_PROJECTION}"
        ));
    }
    for fixture in INVALID_NICKEL_FIXTURES {
        let negative_status = nickel_status(root, Path::new(fixture))?;
        if negative_status.success() {
            return Err(format!(
                "negative Nickel fixture unexpectedly passed: {fixture}"
            ));
        }
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

// r[impl chaoscontrol.nickel_toolchain.cohort]
fn nickel_command() -> Command {
    let mut command = Command::new("nickel");
    command.arg("export");
    command
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

    let mut overclaim = valid.clone();
    overclaim.receipt.bounded_claim = "This proves universal determinism.".to_string();
    set_blocked(&mut overclaim, Blocker::Overclaim);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[7]), &overclaim)?;

    // r[verify chaoscontrol.campaign_policy.rollback.valid]
    let mut diagnostic = valid;
    let row = diagnostic
        .receipt
        .rows
        .iter_mut()
        .find(|row| row.kind == RowKind::FreshWorkloadReplay)
        .ok_or_else(|| "matrix lacks the fresh workload replay row".to_string())?;
    row.artifacts.push(ArtifactIdentity {
        path: LEGACY_DIAGNOSTIC_ARTIFACT.to_string(),
        bytes: 0,
        blake3: EMPTY_BLAKE3.to_string(),
    });
    row.artifacts.sort();
    row.artifact_set_identity = artifact_set_identity(&row.artifacts);
    set_blocked(&mut diagnostic, Blocker::LegacyDiagnosticOutput);
    write_json(root.join(INVALID_RECEIPT_FIXTURES[8]), &diagnostic)
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

fn write_curated_receipt(
    output: &Path,
    full_receipt_path: &Path,
    receipt: &KvmReleaseReceipt,
    checked_unix_seconds: u64,
) -> Result<(), String> {
    let full_receipt = read_bytes(full_receipt_path, MAX_INPUT_BYTES).map_err(|error| {
        format!(
            "failed to read full receipt {}: {error}",
            full_receipt_path.display()
        )
    })?;
    let rows = receipt
        .rows
        .iter()
        .map(|row| {
            let artifact_bytes = row
                .artifacts
                .iter()
                .try_fold(0_u64, |total, artifact| total.checked_add(artifact.bytes));
            artifact_bytes
                .map(|artifact_bytes| CuratedRow {
                    id: row.id.clone(),
                    kind: row.kind,
                    status: row.status,
                    command_identity: row.command_identity.clone(),
                    artifact_set_identity: row.artifact_set_identity.clone(),
                    artifact_count: row.artifacts.len(),
                    artifact_bytes,
                })
                .ok_or_else(|| format!("artifact bytes overflow for row {}", row.id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let curated = CuratedValidationReceipt {
        schema: "chaoscontrol.kvm-release-validation.v1",
        source: receipt.source.clone(),
        matrix_profile: receipt.matrix_profile.clone(),
        matrix_identity: receipt.matrix_identity.clone(),
        runner_revision: receipt.runner_revision.clone(),
        worker: receipt.worker.clone(),
        started_unix_seconds: receipt.started_unix_seconds,
        finished_unix_seconds: receipt.finished_unix_seconds,
        checked_unix_seconds,
        terminal_class: receipt.terminal_class,
        rows,
        full_receipt_identity: format!("blake3:{}", blake3::hash(&full_receipt).to_hex()),
        raw_receipt_retention: "primary-worktree .pi evidence; raw row artifacts are not committed",
        bounded_claim: receipt.bounded_claim.clone(),
        non_claims: receipt.non_claims.clone(),
    };
    write_json(output.to_path_buf(), &curated)
}

fn write_json(path: PathBuf, value: &impl Serialize) -> Result<(), String> {
    let parent = chaoscontrol_evidence::bounded_file::output_parent(&path);
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create fixture directory: {error}"))?;
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize fixture: {error}"))?;
    bytes.push(b'\n');
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| error.to_string())?;
    temporary
        .persist(&path)
        .map_err(|error| error.error.to_string())?;
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

fn unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn read_json_rejects_excessive_nesting() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("receipt.json");
        fs::write(&path, format!("{}{}", "[".repeat(512), "]".repeat(512))).unwrap();
        let error = read_json::<serde_json::Value>(&path).unwrap_err();
        assert!(error.contains("JSON nesting depth exceeded"));
    }

    #[test]
    fn empty_paths_are_rejected() {
        assert!(parse_args(vec![OsString::from("--root"), OsString::from("")]).is_err());
        assert!(parse_args(vec![OsString::from("--matrix"), OsString::from("")]).is_err());
        assert!(parse_args(vec![
            OsString::from("--receipt"),
            OsString::from(""),
            OsString::from("--expected-revision"),
            OsString::from("rev"),
        ])
        .is_err());
        assert!(parse_args(vec![
            OsString::from("--receipt"),
            OsString::from("receipt.json"),
            OsString::from("--expected-revision"),
            OsString::from("rev"),
            OsString::from("--curated-out"),
            OsString::from(""),
        ])
        .is_err());
        assert!(parse_args(vec![
            OsString::from("--receipt"),
            OsString::from("receipt.json"),
            OsString::from("--expected-revision"),
            OsString::from(""),
        ])
        .is_err());
        assert!(parse_args(vec![
            OsString::from("--receipt"),
            OsString::from("receipt.json"),
            OsString::from("--expected-revision"),
            OsString::from("x".repeat(MAX_EXPECTED_REVISION_BYTES + 1)),
        ])
        .is_err());
        assert!(parse_args(vec![
            OsString::from("--receipt"),
            OsString::from("receipt.json"),
            OsString::from("--expected-revision"),
            OsString::from("bad\nrev"),
        ])
        .is_err());
        assert!(parse_args(vec![
            OsString::from("--receipt"),
            OsString::from("receipt.json"),
            OsString::from("--expected-revision"),
            OsString::from("x".repeat(MAX_EXPECTED_REVISION_BYTES)),
        ])
        .is_ok());
    }

    #[test]
    fn input_reads_enforce_the_exact_byte_bound() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("input.json");
        fs::write(&path, b"abcd").unwrap();

        assert_eq!(read_bytes(&path, 4).unwrap(), b"abcd");
        assert!(read_bytes(&path, 3).is_err());
    }

    #[test]
    fn reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bytes(&link, MAX_INPUT_BYTES).is_err());

        let fifo = root.path().join("receipt.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bytes(&fifo, MAX_INPUT_BYTES).is_err());
    }

    #[test]
    fn failed_fixture_publication_leaves_no_temporary_files() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let before = fs::read_dir(root.path()).unwrap().count();

        assert!(write_json(target, &serde_json::json!({"status": "passed"})).is_err());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), before);
    }

    #[test]
    fn atomic_fixture_replacement_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"original").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_json(link.clone(), &serde_json::json!({"status": "passed"})).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&link).unwrap()).unwrap()
                ["status"],
            "passed"
        );
    }
}
