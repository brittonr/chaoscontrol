use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use chaoscontrol_wasm_differential::{
    compare_case, encode_hex, finalize_report, generate_mvp_module, generated_shrink_candidates,
    malformed_magic, normalize_spacewasm, normalize_tool, profile_identity,
    same_mismatch_predicate, validate_bundle_manifest, validate_profile, BundleManifest,
    CaseComparison, CaseKind, ComparisonVerdict, DifferentialProfile, DifferentialReport,
    MantleRunnerReport, MinimizationEvidence, ToolObservation, REPORT_SCHEMA,
};
use tempfile::TempDir;
use thiserror::Error;
use wait_timeout::ChildExt;

const MANIFEST_PATH: &str = "manifest.json";
const HOST_RUNNER_PATH: &str = "binaries/host/mantle-spacewasm-diagnostic-runner";
const FIXTURE_DIRECTORY: &str = "fixtures/wasm";
const SPACEWASM_REPORT_FILE: &str = "spacewasm-report.json";
const COMMAND_STDOUT_FILE: &str = "stdout.txt";
const COMMAND_STDERR_FILE: &str = "stderr.txt";
const STAGED_RUNNER_FILE: &str = "spacewasm-runner";
const STAGED_RUNNER_MODE: u32 = 0o500;
const HASH_BUFFER_BYTES: usize = 64 * 1024;
const MAXIMUM_PROFILE_BYTES: u64 = 1024 * 1024;
const WASMTIME_OUT_OF_FUEL_BUDGET: u64 = 1;
const WASMTIME_ENGINE: &str = "wasmtime";
const FIXTURE_FILES: [&str; 8] = [
    "allocation-failure.wasm",
    "mvp-negative.wasm",
    "mvp-positive.wasm",
    "out-of-fuel.wasm",
    "streaming-negative.wasm",
    "streaming-positive.wasm",
    "trap-unreachable.wasm",
    "unsupported-bulk-memory.wasm",
];
const FIXED_CASES: [FixedCase; 6] = [
    FixedCase {
        case_id: "mvp-positive",
        fixture_id: "mvp-positive",
        file_name: "mvp-positive.wasm",
        kind: CaseKind::Execute,
        fuel: FuelMode::Profile,
    },
    FixedCase {
        case_id: "mvp-negative",
        fixture_id: "mvp-negative",
        file_name: "mvp-negative.wasm",
        kind: CaseKind::Reject,
        fuel: FuelMode::Profile,
    },
    FixedCase {
        case_id: "streaming-positive",
        fixture_id: "streaming-positive",
        file_name: "streaming-positive.wasm",
        kind: CaseKind::Execute,
        fuel: FuelMode::Profile,
    },
    FixedCase {
        case_id: "streaming-negative",
        fixture_id: "streaming-negative",
        file_name: "streaming-negative.wasm",
        kind: CaseKind::Reject,
        fuel: FuelMode::Profile,
    },
    FixedCase {
        case_id: "trap-unreachable",
        fixture_id: "trap",
        file_name: "trap-unreachable.wasm",
        kind: CaseKind::Execute,
        fuel: FuelMode::Profile,
    },
    FixedCase {
        case_id: "out-of-fuel",
        fixture_id: "out-of-fuel",
        file_name: "out-of-fuel.wasm",
        kind: CaseKind::Execute,
        fuel: FuelMode::Exhaustion,
    },
];

#[derive(Clone, Copy)]
struct FixedCase {
    case_id: &'static str,
    fixture_id: &'static str,
    file_name: &'static str,
    kind: CaseKind,
    fuel: FuelMode,
}

#[derive(Clone, Copy)]
enum FuelMode {
    Profile,
    Exhaustion,
}

struct GeneratedCase<'a> {
    case_id: &'a str,
    case_index: usize,
    fixture_id: &'static str,
    fixture_file_name: &'static str,
    kind: CaseKind,
    mutation: GeneratedMutation,
    module: &'a [u8],
}

#[derive(Clone, Copy)]
enum GeneratedMutation {
    None,
    MalformedMagic,
}

#[derive(Debug)]
struct Arguments {
    profile: PathBuf,
    bundle: PathBuf,
    wasmtime: PathBuf,
    output: PathBuf,
    artifacts: PathBuf,
}

#[derive(Debug)]
struct CommandResult {
    status: ExitStatus,
    timed_out: bool,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Error)]
enum Error {
    #[error("invalid arguments: {0}")]
    Arguments(String),
    #[error("invalid evidence: {0}")]
    Invalid(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("JSON error at {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("process error for {program}: {source}")]
    Process {
        program: PathBuf,
        source: std::io::Error,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("spacewasm differential failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Error> {
    let arguments = parse_arguments(std::env::args().skip(1))?;
    let profile: DifferentialProfile = read_json(&arguments.profile, MAXIMUM_PROFILE_BYTES)?;
    validate_profile(&profile).map_err(Error::Invalid)?;
    let manifest = admit_bundle(&profile, &arguments.bundle)?;
    let wasmtime_version = read_wasmtime_version(&profile, &arguments.wasmtime)?;
    if wasmtime_version != profile.wasmtime_version {
        return Err(Error::Invalid(format!(
            "Wasmtime version drift: expected={:?} actual={wasmtime_version:?}",
            profile.wasmtime_version
        )));
    }
    create_artifact_directory(&arguments.artifacts)?;
    let comparisons = execute_corpus(
        &profile,
        &manifest,
        &arguments.bundle,
        &arguments.wasmtime,
        &arguments.artifacts,
    )?;
    let host_runner_digest = manifest
        .members
        .iter()
        .find(|member| member.path == HOST_RUNNER_PATH)
        .ok_or_else(|| {
            Error::Invalid(String::from("admitted bundle lacks exact host runner path"))
        })?
        .digest_blake3
        .clone();
    let report = finalize_report(DifferentialReport {
        schema: String::from(REPORT_SCHEMA),
        profile_id: profile.profile_id.clone(),
        profile_identity_blake3: profile_identity(&profile).map_err(Error::Invalid)?,
        bundle_identity_blake3: profile.bundle_identity_blake3.clone(),
        bundle_manifest_blake3: profile.bundle_manifest_blake3.clone(),
        spacewasm_runtime_blake3: host_runner_digest,
        wasmtime_version,
        seed: profile.seed,
        comparisons,
        mismatch_count: 0,
        verdict: String::new(),
        non_claims: profile.non_claims.clone(),
        report_identity_blake3: String::new(),
    })
    .map_err(Error::Invalid)?;
    write_new_json(&arguments.output, &report)?;
    println!(
        "spacewasm differential {}: cases={} mismatches={} report_identity_blake3={}",
        report.verdict,
        report.comparisons.len(),
        report.mismatch_count,
        report.report_identity_blake3
    );
    if report.mismatch_count == 0 {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "{} normalized comparison mismatch(es) retained in {}",
            report.mismatch_count,
            arguments.output.display()
        )))
    }
}

fn parse_arguments(arguments: impl Iterator<Item = String>) -> Result<Arguments, Error> {
    let mut profile = None;
    let mut bundle = None;
    let mut wasmtime = None;
    let mut output = None;
    let mut artifacts = None;
    let mut arguments = arguments;
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| Error::Arguments(format!("missing value after {argument}")))?;
        match argument.as_str() {
            "--profile" => profile = Some(PathBuf::from(value)),
            "--bundle" => bundle = Some(PathBuf::from(value)),
            "--wasmtime" => wasmtime = Some(PathBuf::from(value)),
            "--out" => output = Some(PathBuf::from(value)),
            "--artifacts" => artifacts = Some(PathBuf::from(value)),
            _ => return Err(Error::Arguments(format!("unknown option: {argument}"))),
        }
    }
    Ok(Arguments {
        profile: profile.ok_or_else(|| Error::Arguments(String::from("missing --profile")))?,
        bundle: bundle.ok_or_else(|| Error::Arguments(String::from("missing --bundle")))?,
        wasmtime: wasmtime.ok_or_else(|| Error::Arguments(String::from("missing --wasmtime")))?,
        output: output.ok_or_else(|| Error::Arguments(String::from("missing --out")))?,
        artifacts: artifacts
            .ok_or_else(|| Error::Arguments(String::from("missing --artifacts")))?,
    })
}

fn create_artifact_directory(path: &Path) -> Result<(), Error> {
    fs::create_dir(path).map_err(|source| io_error(path, source))
}

fn write_generated_module(directory: &Path, case_id: &str, module: &[u8]) -> Result<(), Error> {
    let path = directory.join(format!("{case_id}.wasm"));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|source| io_error(&path, source))?;
    output
        .write_all(module)
        .map_err(|source| io_error(&path, source))
}

fn admit_bundle(profile: &DifferentialProfile, bundle: &Path) -> Result<BundleManifest, Error> {
    let manifest_path = bundle.join(MANIFEST_PATH);
    ensure_regular_file(&manifest_path)?;
    let actual_manifest_digest =
        hash_file(&manifest_path, profile.bounds.maximum_bundle_member_bytes)?;
    if actual_manifest_digest != profile.bundle_manifest_blake3 {
        return Err(Error::Invalid(format!(
            "bundle manifest digest mismatch: expected={} actual={actual_manifest_digest}",
            profile.bundle_manifest_blake3
        )));
    }
    let manifest: BundleManifest = read_json(&manifest_path, profile.bounds.maximum_output_bytes)?;
    validate_bundle_manifest(profile, &manifest).map_err(Error::Invalid)?;
    for member in &manifest.members {
        let path = bundle.join(&member.path);
        ensure_regular_file(&path)?;
        let metadata = fs::metadata(&path).map_err(|source| io_error(&path, source))?;
        if metadata.len() != member.size_bytes {
            return Err(Error::Invalid(format!(
                "bundle member size mismatch for {}: expected={} actual={}",
                member.path,
                member.size_bytes,
                metadata.len()
            )));
        }
        let digest = hash_file(&path, profile.bounds.maximum_bundle_member_bytes)?;
        if digest != member.digest_blake3 {
            return Err(Error::Invalid(format!(
                "bundle member digest mismatch for {}: expected={} actual={digest}",
                member.path, member.digest_blake3
            )));
        }
    }
    Ok(manifest)
}

fn execute_corpus(
    profile: &DifferentialProfile,
    manifest: &BundleManifest,
    bundle: &Path,
    wasmtime: &Path,
    artifacts: &Path,
) -> Result<Vec<CaseComparison>, Error> {
    let fixture_root = bundle.join(FIXTURE_DIRECTORY);
    let base_report = run_spacewasm(profile, bundle, &fixture_root)?;
    if base_report.source_revision != profile.spacewasm_revision {
        return Err(Error::Invalid(String::from(
            "SpaceWasm runner source revision drift",
        )));
    }
    let mut comparisons = Vec::with_capacity(profile.bounds.maximum_cases);
    for case in FIXED_CASES {
        let module_path = fixture_root.join(case.file_name);
        let module_digest = hash_file(&module_path, profile.bounds.maximum_input_bytes)?;
        let spacewasm =
            normalize_spacewasm(case.fixture_id, &base_report).map_err(Error::Invalid)?;
        let fuel = match case.fuel {
            FuelMode::Profile => profile.bounds.wasmtime_fuel,
            FuelMode::Exhaustion => WASMTIME_OUT_OF_FUEL_BUDGET,
        };
        let raw = run_wasmtime(profile, wasmtime, &module_path, fuel)?;
        let wasmtime_observation = normalize_tool(WASMTIME_ENGINE, case.kind, &raw);
        let comparison = compare_case(
            String::from(case.case_id),
            case.kind,
            module_digest,
            spacewasm,
            wasmtime_observation,
        )
        .map_err(Error::Invalid)?;
        comparisons.push(comparison);
    }

    let runner_path = bundle.join(HOST_RUNNER_PATH);
    let expected_runner_digest = manifest
        .members
        .iter()
        .find(|member| member.path == HOST_RUNNER_PATH)
        .ok_or_else(|| Error::Invalid(String::from("host runner is not in bundle manifest")))?
        .digest_blake3
        .as_str();
    let actual_runner_digest = hash_file(&runner_path, profile.bounds.maximum_bundle_member_bytes)?;
    if actual_runner_digest != expected_runner_digest {
        return Err(Error::Invalid(String::from(
            "host runner changed after bundle admission",
        )));
    }

    for index in 0..profile.generated_valid_cases {
        let module = generate_mvp_module(
            profile.seed,
            index,
            profile.bounds.maximum_generated_instruction_pairs,
        );
        let case_id = format!("generated-valid-{index:04}");
        write_generated_module(artifacts, &case_id, &module)?;
        let case = GeneratedCase {
            case_id: &case_id,
            case_index: index,
            fixture_id: "mvp-positive",
            fixture_file_name: "mvp-positive.wasm",
            kind: CaseKind::Execute,
            mutation: GeneratedMutation::None,
            module: &module,
        };
        let comparison = run_generated_case(profile, bundle, wasmtime, &fixture_root, &case)?;
        comparisons.push(minimize_generated_mismatch(
            profile,
            bundle,
            wasmtime,
            &fixture_root,
            &case,
            comparison,
        )?);
    }
    for index in 0..profile.generated_malformed_cases {
        let generated = generate_mvp_module(
            profile.seed,
            index,
            profile.bounds.maximum_generated_instruction_pairs,
        );
        let module = malformed_magic(&generated);
        let case_id = format!("generated-malformed-{index:04}");
        write_generated_module(artifacts, &case_id, &module)?;
        let case = GeneratedCase {
            case_id: &case_id,
            case_index: index,
            fixture_id: "mvp-negative",
            fixture_file_name: "mvp-negative.wasm",
            kind: CaseKind::Reject,
            mutation: GeneratedMutation::MalformedMagic,
            module: &module,
        };
        let comparison = run_generated_case(profile, bundle, wasmtime, &fixture_root, &case)?;
        comparisons.push(minimize_generated_mismatch(
            profile,
            bundle,
            wasmtime,
            &fixture_root,
            &case,
            comparison,
        )?);
    }
    if comparisons.len() > profile.bounds.maximum_cases {
        return Err(Error::Invalid(String::from(
            "executed corpus exceeds maximum_cases",
        )));
    }
    Ok(comparisons)
}

fn run_generated_case(
    profile: &DifferentialProfile,
    bundle: &Path,
    wasmtime: &Path,
    fixture_root: &Path,
    case: &GeneratedCase<'_>,
) -> Result<CaseComparison, Error> {
    if case.module.len() as u64 > profile.bounds.maximum_input_bytes {
        return Err(Error::Invalid(format!(
            "generated module exceeds input bound: {}",
            case.case_id
        )));
    }
    let temporary = TempDir::new()
        .map_err(|source| io_error(Path::new("temporary fixture directory"), source))?;
    copy_fixture_set(
        fixture_root,
        temporary.path(),
        profile.bounds.maximum_input_bytes,
    )?;
    let module_path = temporary.path().join(case.fixture_file_name);
    fs::write(&module_path, case.module).map_err(|source| io_error(&module_path, source))?;
    let report = run_spacewasm(profile, bundle, temporary.path())?;
    let spacewasm = normalize_spacewasm(case.fixture_id, &report).map_err(Error::Invalid)?;
    let raw = run_wasmtime(
        profile,
        wasmtime,
        &module_path,
        profile.bounds.wasmtime_fuel,
    )?;
    let wasmtime_observation = normalize_tool(WASMTIME_ENGINE, case.kind, &raw);
    compare_case(
        String::from(case.case_id),
        case.kind,
        blake3::hash(case.module).to_hex().to_string(),
        spacewasm,
        wasmtime_observation,
    )
    .map_err(Error::Invalid)
}

fn minimize_generated_mismatch(
    profile: &DifferentialProfile,
    bundle: &Path,
    wasmtime: &Path,
    fixture_root: &Path,
    original_case: &GeneratedCase<'_>,
    original_comparison: CaseComparison,
) -> Result<CaseComparison, Error> {
    if original_comparison.verdict != ComparisonVerdict::Mismatch {
        return Ok(original_comparison);
    }
    let original_digest = original_comparison.module_blake3.clone();
    let mut minimized_module = original_case.module.to_vec();
    let mut minimized_comparison = original_comparison;
    let mut attempts = 0_usize;
    let candidates = generated_shrink_candidates(
        profile.seed,
        original_case.case_index,
        profile.bounds.maximum_generated_instruction_pairs,
    );
    for valid_candidate in candidates
        .into_iter()
        .take(profile.bounds.maximum_shrink_attempts)
    {
        attempts = attempts.saturating_add(1);
        let candidate_module = match original_case.mutation {
            GeneratedMutation::None => valid_candidate,
            GeneratedMutation::MalformedMagic => malformed_magic(&valid_candidate),
        };
        let candidate_case = GeneratedCase {
            case_id: original_case.case_id,
            case_index: original_case.case_index,
            fixture_id: original_case.fixture_id,
            fixture_file_name: original_case.fixture_file_name,
            kind: original_case.kind,
            mutation: original_case.mutation,
            module: &candidate_module,
        };
        let candidate_comparison =
            run_generated_case(profile, bundle, wasmtime, fixture_root, &candidate_case)?;
        if candidate_module.len() < minimized_module.len()
            && same_mismatch_predicate(&minimized_comparison, &candidate_comparison)
        {
            minimized_module = candidate_module;
            minimized_comparison = candidate_comparison;
        }
    }
    let predicate_field = minimized_comparison
        .first_difference
        .as_ref()
        .expect("mismatch comparison has a first difference")
        .field
        .clone();
    minimized_comparison.minimization = Some(MinimizationEvidence {
        predicate_field,
        attempts,
        original_module_blake3: original_digest,
        minimized_module_blake3: blake3::hash(&minimized_module).to_hex().to_string(),
        minimized_module_hex: encode_hex(&minimized_module),
    });
    Ok(minimized_comparison)
}

fn copy_fixture_set(source: &Path, destination: &Path, maximum_bytes: u64) -> Result<(), Error> {
    for file_name in FIXTURE_FILES {
        let source_path = source.join(file_name);
        ensure_regular_file(&source_path)?;
        let bytes = read_bounded(&source_path, maximum_bytes)?;
        let destination_path = destination.join(file_name);
        fs::write(&destination_path, bytes).map_err(|error| io_error(&destination_path, error))?;
    }
    Ok(())
}

fn run_spacewasm(
    profile: &DifferentialProfile,
    bundle: &Path,
    fixture_root: &Path,
) -> Result<MantleRunnerReport, Error> {
    let temporary = TempDir::new()
        .map_err(|source| io_error(Path::new("temporary runner directory"), source))?;
    let report_path = temporary.path().join(SPACEWASM_REPORT_FILE);
    let bundled_runner_path = bundle.join(HOST_RUNNER_PATH);
    let runner_path = temporary.path().join(STAGED_RUNNER_FILE);
    fs::copy(&bundled_runner_path, &runner_path)
        .map_err(|source| io_error(&runner_path, source))?;
    let permissions = fs::Permissions::from_mode(STAGED_RUNNER_MODE);
    fs::set_permissions(&runner_path, permissions)
        .map_err(|source| io_error(&runner_path, source))?;
    let result = run_command(
        &runner_path,
        &[fixture_root.as_os_str(), report_path.as_os_str()],
        profile,
    )?;
    if result.timed_out || !result.status.success() {
        return Err(Error::Invalid(format!(
            "SpaceWasm runner failed: timed_out={} status={} stdout={:?} stderr={:?}",
            result.timed_out, result.status, result.stdout, result.stderr
        )));
    }
    read_json(&report_path, profile.bounds.maximum_output_bytes)
}

fn run_wasmtime(
    profile: &DifferentialProfile,
    wasmtime: &Path,
    module: &Path,
    fuel: u64,
) -> Result<ToolObservation, Error> {
    let wasm_options = format!(
        "all-proposals=n,fuel={fuel},timeout={}ms",
        profile.bounds.maximum_process_milliseconds
    );
    let arguments = [
        std::ffi::OsStr::new("run"),
        std::ffi::OsStr::new("-W"),
        std::ffi::OsStr::new(&wasm_options),
        std::ffi::OsStr::new("--invoke"),
        std::ffi::OsStr::new("run"),
        module.as_os_str(),
    ];
    let result = run_command(wasmtime, &arguments, profile)?;
    Ok(ToolObservation {
        success: result.status.success(),
        timed_out: result.timed_out,
        stderr: result.stderr,
    })
}

fn read_wasmtime_version(profile: &DifferentialProfile, wasmtime: &Path) -> Result<String, Error> {
    let result = run_command(wasmtime, &[std::ffi::OsStr::new("--version")], profile)?;
    if result.timed_out || !result.status.success() {
        return Err(Error::Invalid(String::from(
            "Wasmtime version command failed",
        )));
    }
    let version = result.stdout.trim();
    if version.is_empty() {
        return Err(Error::Invalid(String::from(
            "Wasmtime version output is empty",
        )));
    }
    Ok(version.to_owned())
}

fn run_command(
    program: &Path,
    arguments: &[&std::ffi::OsStr],
    profile: &DifferentialProfile,
) -> Result<CommandResult, Error> {
    ensure_regular_file(program)?;
    let temporary = TempDir::new()
        .map_err(|source| io_error(Path::new("temporary command directory"), source))?;
    let stdout_path = temporary.path().join(COMMAND_STDOUT_FILE);
    let stderr_path = temporary.path().join(COMMAND_STDERR_FILE);
    let stdout = File::create(&stdout_path).map_err(|source| io_error(&stdout_path, source))?;
    let stderr = File::create(&stderr_path).map_err(|source| io_error(&stderr_path, source))?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let mut child = command.spawn().map_err(|source| Error::Process {
        program: program.to_path_buf(),
        source,
    })?;
    let timeout = Duration::from_millis(profile.bounds.maximum_process_milliseconds);
    let maybe_status = child
        .wait_timeout(timeout)
        .map_err(|source| Error::Process {
            program: program.to_path_buf(),
            source,
        })?;
    let (status, timed_out) = match maybe_status {
        Some(status) => (status, false),
        None => {
            child.kill().map_err(|source| Error::Process {
                program: program.to_path_buf(),
                source,
            })?;
            let status = child.wait().map_err(|source| Error::Process {
                program: program.to_path_buf(),
                source,
            })?;
            (status, true)
        }
    };
    let stdout = String::from_utf8_lossy(&read_bounded(
        &stdout_path,
        profile.bounds.maximum_output_bytes,
    )?)
    .into_owned();
    let stderr = String::from_utf8_lossy(&read_bounded(
        &stderr_path,
        profile.bounds.maximum_output_bytes,
    )?)
    .into_owned();
    Ok(CommandResult {
        status,
        timed_out,
        stdout,
        stderr,
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, maximum_bytes: u64) -> Result<T, Error> {
    let bytes = read_bounded(path, maximum_bytes)?;
    serde_json::from_slice(&bytes).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn read_bounded(path: &Path, maximum_bytes: u64) -> Result<Vec<u8>, Error> {
    ensure_regular_file(path)?;
    let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
    if metadata.len() > maximum_bytes {
        return Err(Error::Invalid(format!(
            "file exceeds byte bound at {}: actual={} maximum={maximum_bytes}",
            path.display(),
            metadata.len()
        )));
    }
    fs::read(path).map_err(|source| io_error(path, source))
}

fn hash_file(path: &Path, maximum_bytes: u64) -> Result<String, Error> {
    ensure_regular_file(path)?;
    let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
    if metadata.len() > maximum_bytes {
        return Err(Error::Invalid(format!(
            "file exceeds hash bound: {}",
            path.display()
        )));
    }
    let mut input = File::open(path).map_err(|source| io_error(path, source))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|source| io_error(path, source))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn ensure_regular_file(path: &Path) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(path).map_err(|source| io_error(path, source))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(Error::Invalid(format!(
            "path is not one regular no-follow file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn write_new_json(path: &Path, value: &impl serde::Serialize) -> Result<(), Error> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| io_error(path, source))?;
    output
        .write_all(&bytes)
        .map_err(|source| io_error(path, source))?;
    output
        .write_all(b"\n")
        .map_err(|source| io_error(path, source))
}

fn io_error(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}
