use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chaoscontrol_evidence::kvm_release::{
    artifact_set_identity, classify, command_identity, matrix_identity, validate_matrix,
    validate_receipt, ArtifactIdentity, KvmReleaseDecision, KvmReleaseMatrix, KvmReleaseReceipt,
    ReleaseClass, RowReceipt, RowStatus, SourceFacts, WorkerFacts, MAX_MATRIX_ARTIFACT_BYTES,
    RECEIPT_SCHEMA_VERSION,
};
use kvm_ioctls::Kvm;

const PROGRAM_NAME: &str = "run-kvm-release-matrix";
const SUCCESS_EXIT_CODE: i32 = 0;
const BLOCKED_EXIT_CODE: i32 = 1;
const USAGE_EXIT_CODE: i32 = 2;
const WAIT_POLL_MILLISECONDS: u64 = 100;
const HASH_BUFFER_BYTES: usize = 64 * 1_024;
const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARTIFACT_SCAN_DEPTH: usize = 64;
const KVM_CAPABILITY: &str = "kvm";
const X86_64_LINUX_CAPABILITY: &str = "x86_64-linux";
const KERNEL_RELEASE_PATH: &str = "/proc/sys/kernel/osrelease";
const RECEIPT_FILE: &str = "release-receipt.json";
const SUMMARY_FILE: &str = "release-summary.md";

#[derive(Debug)]
struct Config {
    root: PathBuf,
    matrix: PathBuf,
    output: Option<PathBuf>,
    expected_revision: Option<String>,
    dry_run: bool,
}

#[derive(Debug)]
struct ExecutionResult {
    status: RowStatus,
    exit_code: Option<i32>,
    executed_argv: Vec<String>,
    notes: Vec<String>,
}

#[derive(Debug)]
struct ArtifactCollector {
    artifacts: Vec<ArtifactIdentity>,
    total_bytes: u64,
    visited_directories: BTreeSet<PathBuf>,
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
        Ok(true) => std::process::exit(SUCCESS_EXIT_CODE),
        Ok(false) => std::process::exit(BLOCKED_EXIT_CODE),
        Err(message) => {
            eprintln!("{PROGRAM_NAME}: {message}");
            std::process::exit(BLOCKED_EXIT_CODE);
        }
    }
}

fn usage() -> &'static str {
    "usage: run-kvm-release-matrix --root PATH --matrix PATH [--out PATH --expected-revision REV] [--dry-run]"
}

fn parse_args(args: Vec<OsString>) -> Result<Config, String> {
    let mut root = None;
    let mut matrix = None;
    let mut output = None;
    let mut expected_revision = None;
    let mut dry_run = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].to_string_lossy().as_ref() {
            "--root" => {
                root = Some(PathBuf::from(required_value(&args, index, "--root")?));
                index += 2;
            }
            "--matrix" => {
                matrix = Some(PathBuf::from(required_value(&args, index, "--matrix")?));
                index += 2;
            }
            "--out" => {
                output = Some(PathBuf::from(required_value(&args, index, "--out")?));
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
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(SUCCESS_EXIT_CODE);
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    let config = Config {
        root: root.ok_or_else(|| "--root is required".to_string())?,
        matrix: matrix.ok_or_else(|| "--matrix is required".to_string())?,
        output,
        expected_revision,
        dry_run,
    };
    if config.root.as_os_str().is_empty() || config.matrix.as_os_str().is_empty() {
        return Err("--root and --matrix must not be empty".to_string());
    }
    if config
        .output
        .as_ref()
        .is_some_and(|output| output.as_os_str().is_empty())
    {
        return Err("--out must not be empty".to_string());
    }
    if !config.dry_run && (config.output.is_none() || config.expected_revision.is_none()) {
        return Err("--out and --expected-revision are required without --dry-run".to_string());
    }
    Ok(config)
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

// r[impl chaoscontrol.kvm_release_rail.worker]
// r[impl chaoscontrol.kvm_release_rail.required_rows]
// r[impl chaoscontrol.kvm_release_rail.receipt]
fn run(config: Config) -> Result<bool, String> {
    let root = fs::canonicalize(&config.root)
        .map_err(|error| format!("failed to resolve root: {error}"))?;
    let matrix_path = resolve_path(&root, &config.matrix);
    let matrix = read_matrix(&matrix_path)?;
    validate_matrix(&matrix).map_err(|issues| issues.join("; "))?;

    if config.dry_run {
        print_dry_run(&matrix);
        return Ok(true);
    }

    let output = resolve_path(
        &root,
        config
            .output
            .as_ref()
            .ok_or_else(|| "missing output after argument validation".to_string())?,
    );
    if output.exists() {
        return Err(format!("output already exists: {}", output.display()));
    }
    let expected_revision = config
        .expected_revision
        .as_deref()
        .ok_or_else(|| "missing expected revision after argument validation".to_string())?;
    let source = observe_source(&root)?;
    let worker = observe_worker();
    fs::create_dir_all(&output)
        .map_err(|error| format!("failed to create output directory: {error}"))?;

    let started_unix_seconds = unix_seconds()?;
    let mut rows = Vec::with_capacity(matrix.rows.len());
    for row in &matrix.rows {
        rows.push(run_row(&root, &output, &worker, row)?);
    }
    let finished_unix_seconds = unix_seconds()?;
    let mut receipt = KvmReleaseReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        matrix_profile: matrix.profile_id.clone(),
        matrix_identity: matrix_identity(&matrix),
        source,
        runner_revision: format!(
            "chaoscontrol-evidence-{}@{expected_revision}",
            env!("CARGO_PKG_VERSION")
        ),
        worker,
        started_unix_seconds,
        finished_unix_seconds,
        rows,
        bounded_claim: matrix.bounded_claim.clone(),
        non_claims: matrix.non_claims.clone(),
        terminal_class: ReleaseClass::Blocked,
    };
    receipt.terminal_class =
        classify(&matrix, expected_revision, &receipt, finished_unix_seconds).terminal_class;
    let decision = validate_receipt(&matrix, expected_revision, &receipt, finished_unix_seconds);
    write_receipt(&output, &receipt)?;
    write_summary(&output, &receipt, &decision)?;
    println!(
        "KVM release matrix: {:?}; receipt={}",
        decision.terminal_class,
        output.join(RECEIPT_FILE).display()
    );
    for reason in &decision.reasons {
        println!("  blocker: {reason}");
    }
    Ok(decision.terminal_class == ReleaseClass::ReleaseEligible)
}

fn read_matrix(path: &Path) -> Result<KvmReleaseMatrix, String> {
    let bytes = read_bytes(path, MAX_INPUT_BYTES)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse matrix {}: {error}", path.display()))
}

fn read_bytes(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
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

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn print_dry_run(matrix: &KvmReleaseMatrix) {
    println!(
        "KVM release matrix dry-run: profile={} identity={} rows={}",
        matrix.profile_id,
        matrix_identity(matrix),
        matrix.rows.len()
    );
    for row in &matrix.rows {
        println!(
            "  {}: {:?} timeout={}s command={} {:?}",
            row.id, row.kind, row.limits.timeout_seconds, row.command.program, row.command.args
        );
    }
}

fn observe_source(root: &Path) -> Result<SourceFacts, String> {
    let revision = git_output(root, &["rev-parse", "HEAD"])?;
    let status = git_output(root, &["status", "--porcelain", "--untracked-files=all"])?;
    Ok(SourceFacts {
        revision,
        dirty: !status.is_empty(),
    })
}

fn git_output(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to run git {:?}: {error}", args))?;
    if !output.status.success() {
        return Err(format!("git {:?} failed with {}", args, output.status));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_string())
        .map_err(|error| format!("git {:?} returned non-UTF-8 output: {error}", args))
}

fn observe_worker() -> WorkerFacts {
    let architecture = std::env::consts::ARCH.to_string();
    let kernel_release = fs::read_to_string(KERNEL_RELEASE_PATH)
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|error| format!("unavailable:{error}"));
    let kvm_api_version = Kvm::new().ok().map(|kvm| kvm.get_api_version());
    let mut capabilities = Vec::new();
    if architecture == "x86_64" && std::env::consts::OS == "linux" {
        capabilities.push(X86_64_LINUX_CAPABILITY.to_string());
    }
    if kvm_api_version.is_some() {
        capabilities.push(KVM_CAPABILITY.to_string());
    }
    capabilities.sort();
    WorkerFacts {
        architecture,
        kernel_release,
        kvm_api_version,
        capabilities,
    }
}

fn run_row(
    root: &Path,
    output: &Path,
    worker: &WorkerFacts,
    row: &chaoscontrol_evidence::kvm_release::MatrixRow,
) -> Result<RowReceipt, String> {
    let row_output = output.join(&row.id);
    fs::create_dir(&row_output)
        .map_err(|error| format!("failed to create row output {}: {error}", row.id))?;
    let started_unix_seconds = unix_seconds()?;
    let missing_capabilities = row
        .required_capabilities
        .iter()
        .filter(|capability| !worker.capabilities.contains(capability))
        .cloned()
        .collect::<Vec<_>>();
    let execution = if missing_capabilities.is_empty() {
        execute_command(root, &row_output, row)?
    } else {
        let note = format!(
            "worker lacks required capabilities: {}",
            missing_capabilities.join(",")
        );
        write_atomic(
            &row_output.join("stderr.log"),
            format!("{note}\n").as_bytes(),
        )
        .map_err(|error| format!("failed to write unsupported row log: {error}"))?;
        write_atomic(&row_output.join("stdout.log"), b"")
            .map_err(|error| format!("failed to write unsupported row output: {error}"))?;
        ExecutionResult {
            status: RowStatus::Unsupported,
            exit_code: None,
            executed_argv: configured_argv(row),
            notes: vec![note],
        }
    };
    let finished_unix_seconds = unix_seconds()?;
    let (artifacts, artifact_error) = collect_artifacts(&row_output, row);
    let mut notes = execution.notes;
    let mut status = execution.status;
    if let Some(error) = artifact_error {
        notes.push(error);
        status = RowStatus::Failed;
    }
    let artifact_set_identity = artifact_set_identity(&artifacts);
    Ok(RowReceipt {
        id: row.id.clone(),
        kind: row.kind,
        required_capabilities: row.required_capabilities.clone(),
        command: row.command.clone(),
        executed_argv: execution.executed_argv,
        command_identity: command_identity(&row.command),
        started_unix_seconds,
        finished_unix_seconds,
        status,
        exit_code: execution.exit_code,
        artifacts,
        artifact_set_identity,
        notes,
    })
}

fn execute_command(
    root: &Path,
    row_output: &Path,
    row: &chaoscontrol_evidence::kvm_release::MatrixRow,
) -> Result<ExecutionResult, String> {
    let args = row
        .command
        .args
        .iter()
        .map(|arg| expand_argument(arg, root, row_output))
        .collect::<Vec<_>>();
    let stdout = open_row_log(&row_output.join("stdout.log"))
        .map_err(|error| format!("failed to create row stdout: {error}"))?;
    let stderr = open_row_log(&row_output.join("stderr.log"))
        .map_err(|error| format!("failed to create row stderr: {error}"))?;
    let mut child = match Command::new(&row.command.program)
        .args(&args)
        .current_dir(root)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Ok(ExecutionResult {
                status: RowStatus::Failed,
                exit_code: None,
                executed_argv: executed_argv(&row.command.program, &args),
                notes: vec![format!("failed to start command: {error}")],
            });
        }
    };
    let deadline = Duration::from_secs(row.limits.timeout_seconds);
    let started = Instant::now();
    loop {
        match child
            .try_wait()
            .map_err(|error| format!("failed to observe row process: {error}"))?
        {
            Some(exit_status) => {
                return Ok(completed_execution(
                    &row.command.program,
                    &args,
                    exit_status,
                ));
            }
            None if started.elapsed() >= deadline => {
                child
                    .kill()
                    .map_err(|error| format!("failed to kill timed-out row: {error}"))?;
                let exit_status = child
                    .wait()
                    .map_err(|error| format!("failed to reap timed-out row: {error}"))?;
                return Ok(ExecutionResult {
                    status: RowStatus::TimedOut,
                    exit_code: exit_status.code(),
                    executed_argv: executed_argv(&row.command.program, &args),
                    notes: vec![format!(
                        "command exceeded {} second timeout",
                        row.limits.timeout_seconds
                    )],
                });
            }
            None => thread::sleep(Duration::from_millis(WAIT_POLL_MILLISECONDS)),
        }
    }
}

fn completed_execution(program: &str, args: &[String], status: ExitStatus) -> ExecutionResult {
    let row_status = if status.success() {
        RowStatus::Passed
    } else {
        RowStatus::Failed
    };
    ExecutionResult {
        status: row_status,
        exit_code: status.code(),
        executed_argv: executed_argv(program, args),
        notes: Vec::new(),
    }
}

fn configured_argv(row: &chaoscontrol_evidence::kvm_release::MatrixRow) -> Vec<String> {
    executed_argv(&row.command.program, &row.command.args)
}

fn executed_argv(program: &str, args: &[String]) -> Vec<String> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(program.to_string());
    argv.extend(args.iter().cloned());
    argv
}

fn expand_argument(argument: &str, root: &Path, row_output: &Path) -> String {
    argument
        .replace("%ROOT%", &root.to_string_lossy())
        .replace("%ROW_OUT%", &row_output.to_string_lossy())
}

fn collect_artifacts(
    row_output: &Path,
    row: &chaoscontrol_evidence::kvm_release::MatrixRow,
) -> (Vec<ArtifactIdentity>, Option<String>) {
    let mut collector = ArtifactCollector {
        artifacts: Vec::new(),
        total_bytes: 0,
        visited_directories: BTreeSet::new(),
    };
    let root = match fs::canonicalize(row_output) {
        Ok(root) => root,
        Err(error) => {
            return (
                Vec::new(),
                Some(format!(
                    "failed to resolve artifact root {}: {error}",
                    row_output.display()
                )),
            )
        }
    };
    let result = collect_node(
        &root,
        row_output,
        Path::new("."),
        0,
        row.limits.max_artifacts,
        row.limits.max_artifact_bytes,
        &mut collector,
    );
    collector.artifacts.sort();
    (collector.artifacts, result.err())
}

fn collect_node(
    root: &Path,
    actual_path: &Path,
    logical_path: &Path,
    depth: usize,
    max_artifacts: usize,
    max_artifact_bytes: u64,
    collector: &mut ArtifactCollector,
) -> Result<(), String> {
    if depth > MAX_ARTIFACT_SCAN_DEPTH {
        return Err(format!(
            "artifact scan exceeds depth bound at {}",
            logical_path.display()
        ));
    }
    let link_metadata = fs::symlink_metadata(actual_path).map_err(|error| {
        format!(
            "failed to inspect artifact {}: {error}",
            actual_path.display()
        )
    })?;
    if link_metadata.file_type().is_symlink() {
        let target = fs::canonicalize(actual_path).map_err(|error| {
            format!(
                "failed to resolve artifact symlink {}: {error}",
                actual_path.display()
            )
        })?;
        if !target.starts_with(root) {
            return Err(format!(
                "artifact symlink escapes row output: {}",
                actual_path.display()
            ));
        }
        return collect_node(
            root,
            &target,
            logical_path,
            depth + 1,
            max_artifacts,
            max_artifact_bytes,
            collector,
        );
    }
    if link_metadata.is_dir() {
        let canonical = fs::canonicalize(actual_path).map_err(|error| {
            format!(
                "failed to resolve artifact directory {}: {error}",
                actual_path.display()
            )
        })?;
        if !canonical.starts_with(root) {
            return Err(format!(
                "artifact directory escapes row output: {}",
                actual_path.display()
            ));
        }
        if !collector.visited_directories.insert(canonical) {
            return Ok(());
        }
        let mut entries = fs::read_dir(actual_path)
            .map_err(|error| format!("failed to list artifact directory: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to read artifact directory entry: {error}"))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            collect_node(
                root,
                &entry.path(),
                &logical_path.join(entry.file_name()),
                depth + 1,
                max_artifacts,
                max_artifact_bytes,
                collector,
            )?;
        }
        return Ok(());
    }
    if !link_metadata.is_file() {
        return Err(format!(
            "artifact is not a regular file: {}",
            actual_path.display()
        ));
    }
    if collector.artifacts.len() >= max_artifacts {
        return Err(format!("artifact count exceeds bound of {max_artifacts}"));
    }
    let next_total = collector
        .total_bytes
        .checked_add(link_metadata.len())
        .ok_or_else(|| "artifact byte count overflowed".to_string())?;
    if next_total > max_artifact_bytes {
        return Err(format!(
            "artifact bytes exceed bound of {max_artifact_bytes}"
        ));
    }
    collector.total_bytes = next_total;
    collector.artifacts.push(ArtifactIdentity {
        path: normalized_artifact_path(logical_path),
        bytes: link_metadata.len(),
        blake3: hash_file(actual_path)?,
    });
    Ok(())
}

fn normalized_artifact_path(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(normalized.as_str())
        .to_string()
}

fn hash_file(path: &Path) -> Result<String, String> {
    let file = open_artifact_input(path)
        .map_err(|error| format!("failed to open artifact {}: {error}", path.display()))?;
    let mut reader = BufReader::with_capacity(HASH_BUFFER_BYTES, file);
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
    let mut hasher = blake3::Hasher::new();
    let mut total = 0_u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("failed to hash artifact {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| format!("hash byte count overflow for artifact {}", path.display()))?;
        if total > MAX_MATRIX_ARTIFACT_BYTES {
            return Err(format!("artifact exceeds hash bound: {}", path.display()));
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn open_artifact_input(path: &Path) -> std::io::Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "artifact is not a regular file",
        ));
    }
    if metadata.len() > MAX_MATRIX_ARTIFACT_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "artifact exceeds the hash bound",
        ));
    }
    Ok(file)
}

fn open_row_log(path: &Path) -> std::io::Result<File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = chaoscontrol_evidence::bounded_file::output_parent(path);
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    fs::File::open(parent)?.sync_all()
}

fn write_receipt(output: &Path, receipt: &KvmReleaseReceipt) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|error| format!("failed to serialize release receipt: {error}"))?;
    bytes.push(b'\n');
    write_atomic(&output.join(RECEIPT_FILE), &bytes)
        .map_err(|error| format!("failed to write release receipt: {error}"))
}

fn write_summary(
    output: &Path,
    receipt: &KvmReleaseReceipt,
    decision: &KvmReleaseDecision,
) -> Result<(), String> {
    let mut summary = String::new();
    summary.push_str("# KVM Release Matrix Summary\n\n");
    summary.push_str(&format!(
        "- Terminal class: `{:?}`\n- Source revision: `{}`\n- Matrix: `{}`\n- Worker: `{}` / `{}`\n",
        decision.terminal_class,
        receipt.source.revision,
        receipt.matrix_identity,
        receipt.worker.architecture,
        receipt.worker.kernel_release
    ));
    summary.push_str("\n## Rows\n\n");
    summary.push_str("| Row | Kind | Status | Artifacts |\n| --- | --- | --- | ---: |\n");
    for row in &receipt.rows {
        summary.push_str(&format!(
            "| `{}` | `{:?}` | `{:?}` | {} |\n",
            row.id,
            row.kind,
            row.status,
            row.artifacts.len()
        ));
    }
    summary.push_str("\n## Blockers\n\n");
    if decision.reasons.is_empty() {
        summary.push_str("None.\n");
    } else {
        for reason in &decision.reasons {
            summary.push_str(&format!("- {reason}\n"));
        }
    }
    summary.push_str("\n## Claim Boundary\n\n");
    summary.push_str(&format!("{}\n", receipt.bounded_claim));
    for non_claim in &receipt.non_claims {
        summary.push_str(&format!("- {non_claim}\n"));
    }
    write_atomic(&output.join(SUMMARY_FILE), summary.as_bytes())
        .map_err(|error| format!("failed to write release summary: {error}"))
}

fn unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    use chaoscontrol_evidence::kvm_release::{CommandSpec, MatrixRow, RowKind, RowLimits};
    use std::os::unix::ffi::OsStrExt;

    fn artifact_row() -> MatrixRow {
        MatrixRow {
            id: "row".to_string(),
            kind: RowKind::DeterministicSmp,
            required: true,
            required_capabilities: vec!["kvm".to_string()],
            command: CommandSpec {
                program: "true".to_string(),
                args: vec!["--version".to_string()],
            },
            limits: RowLimits {
                timeout_seconds: 1,
                max_artifacts: 8,
                max_artifact_bytes: 1_024,
            },
            retained_artifacts: vec!["stdout.log".to_string()],
        }
    }

    #[test]
    fn artifact_scan_rejects_symlinks_outside_row_output() {
        let root = tempfile::tempdir().unwrap();
        let row_output = root.path().join("row");
        fs::create_dir(&row_output).unwrap();
        let outside = root.path().join("outside.log");
        fs::write(&outside, b"outside").unwrap();
        std::os::unix::fs::symlink(&outside, row_output.join("escape.log")).unwrap();

        let (artifacts, error) = collect_artifacts(&row_output, &artifact_row());
        assert!(artifacts.is_empty());
        assert!(error.unwrap().contains("escapes row output"));
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
    }

    #[test]
    fn atomic_output_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"original").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_atomic(&link, b"replacement").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read(&link).unwrap(), b"replacement");
    }

    #[test]
    fn empty_output_path_is_rejected() {
        let args = vec![
            OsString::from("--root"),
            OsString::from("."),
            OsString::from("--matrix"),
            OsString::from("matrix.json"),
            OsString::from("--out"),
            OsString::from(""),
            OsString::from("--expected-revision"),
            OsString::from("rev"),
        ];
        assert!(parse_args(args).is_err());
    }

    #[test]
    fn artifact_hashes_reject_oversized_inputs_before_reading() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("artifact.log");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_MATRIX_ARTIFACT_BYTES + 1).unwrap();

        assert!(hash_file(&path).is_err());
    }

    #[test]
    fn artifact_hashes_reject_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.log");
        fs::write(&target, b"log").unwrap();
        let link = root.path().join("artifact.log");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(hash_file(&link).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"log");
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
    fn input_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("matrix.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bytes(&link, MAX_INPUT_BYTES).is_err());

        let fifo = root.path().join("matrix.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bytes(&fifo, MAX_INPUT_BYTES).is_err());
    }

    #[test]
    fn row_log_open_rejects_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.log");
        fs::write(&target, b"original").unwrap();
        let link = root.path().join("stdout.log");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(open_row_log(&link).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
    }
}
