use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use chaoscontrol_wasm_differential::{
    admission::{admit_module, AdmissionClass},
    rail::{admit_retention, CaseManifest},
};
use chaoscontrol_wasm_differential::{
    compare_case, encode_hex, finalize_report, generated_shrink_candidates, malformed_magic,
    normalize_spacewasm, normalize_tool, profile_identity, same_mismatch_predicate,
    validate_bundle_manifest, validate_manifest_digest, validate_producer_revision,
    validate_profile, validate_stable_evidence, BundleManifest, CaseComparison, CaseKind,
    ComparisonVerdict, DifferentialProfile, DifferentialReport, MantleRunnerReport,
    MinimizationEvidence, ToolObservation, REPORT_SCHEMA, STABLE_EVIDENCE_SUITES,
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
    artifacts: &'a Path,
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
    mantle_revision: String,
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
    validate_producer_revision(&profile, &arguments.mantle_revision).map_err(Error::Invalid)?;
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
        evidence: Vec::new(),
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
    if report.verdict == "match" {
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
    let mut mantle_revision = None;
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
            "--mantle-revision" => mantle_revision = Some(value),
            "--bundle" => bundle = Some(PathBuf::from(value)),
            "--wasmtime" => wasmtime = Some(PathBuf::from(value)),
            "--out" => output = Some(PathBuf::from(value)),
            "--artifacts" => artifacts = Some(PathBuf::from(value)),
            _ => return Err(Error::Arguments(format!("unknown option: {argument}"))),
        }
    }
    Ok(Arguments {
        profile: profile.ok_or_else(|| Error::Arguments(String::from("missing --profile")))?,
        mantle_revision: mantle_revision
            .ok_or_else(|| Error::Arguments(String::from("missing --mantle-revision")))?,
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
    let mut temporary =
        tempfile::NamedTempFile::new_in(directory).map_err(|source| io_error(&path, source))?;
    temporary
        .write_all(module)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| io_error(&path, source))?;
    temporary
        .persist_noclobber(&path)
        .map_err(|error| io_error(&path, error.error))?;
    sync_parent(&path)
}

fn admit_bundle(profile: &DifferentialProfile, bundle: &Path) -> Result<BundleManifest, Error> {
    let manifest_path = bundle.join(MANIFEST_PATH);
    ensure_regular_file(&manifest_path)?;
    let actual_manifest_digest =
        hash_file(&manifest_path, profile.bounds.maximum_bundle_member_bytes)?;
    validate_manifest_digest(profile, &actual_manifest_digest).map_err(Error::Invalid)?;
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
    for suite in &STABLE_EVIDENCE_SUITES {
        let receipt = read_bounded(
            &bundle.join(suite.receipt_path),
            profile.bounds.maximum_output_bytes,
        )?;
        let stable_report = read_bounded(
            &bundle.join(suite.stable_report_path),
            profile.bounds.maximum_output_bytes,
        )?;
        validate_stable_evidence(&manifest, suite, &receipt, &stable_report)
            .map_err(Error::Invalid)?;
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
    let bundle_fixtures = bundle.join(FIXTURE_DIRECTORY);
    let staged_fixtures = TempDir::new()
        .map_err(|source| io_error(Path::new("temporary fixture directory"), source))?;
    stage_fixture_set(
        &bundle_fixtures,
        staged_fixtures.path(),
        profile.bounds.maximum_input_bytes,
    )?;
    let fixture_root = staged_fixtures.path();
    // Admit every selected binary before invoking either runtime shell.
    for case in FIXED_CASES {
        let bytes = read_bounded(
            &fixture_root.join(case.file_name),
            profile.bounds.maximum_input_bytes,
        )?;
        admit_case_bytes(profile, &bytes, case.kind)?;
    }
    let retained_cases =
        FIXED_CASES.len() + profile.generated_valid_cases + profile.generated_malformed_cases;
    let per_case = profile
        .bounds
        .maximum_shrink_attempts
        .checked_mul(4)
        .and_then(|work| work.checked_add(4))
        .ok_or_else(|| Error::Invalid("retention work overflow".into()))?;
    admit_retention(
        profile,
        0,
        retained_cases
            .checked_mul(per_case)
            .ok_or_else(|| Error::Invalid("retention count overflow".into()))?,
    )
    .map_err(|class| Error::Invalid(format!("{class:?}: corpus retention exceeds profile")))?;
    let expected_runner_digest = manifest
        .members
        .iter()
        .find(|member| member.path == HOST_RUNNER_PATH)
        .ok_or_else(|| Error::Invalid(String::from("host runner is not in bundle manifest")))?
        .digest_blake3
        .as_str();
    let base_report = run_spacewasm(profile, bundle, fixture_root, expected_runner_digest)?;
    if base_report.source_revision != profile.spacewasm_revision {
        return Err(Error::Invalid(String::from(
            "SpaceWasm runner source revision drift",
        )));
    }
    let mut comparisons = Vec::with_capacity(retained_cases);
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
        let mut comparison = compare_case(
            String::from(case.case_id),
            case.kind,
            module_digest,
            spacewasm,
            wasmtime_observation,
        )
        .map_err(Error::Invalid)?;
        comparison.raw_observation_refs = vec![
            retain_raw(artifacts, &base_report)?,
            retain_raw(artifacts, &raw)?,
        ];
        comparisons.push(comparison);
    }

    for index in 0..profile.generated_valid_cases {
        let (case_manifest, module) =
            CaseManifest::generate(profile, index, CaseKind::Execute).map_err(Error::Invalid)?;
        case_manifest
            .verify(profile, &module)
            .map_err(Error::Invalid)?;
        let case_id = format!("generated-valid-{index:04}");
        write_generated_module(artifacts, &case_id, &module)?;
        write_new_json(
            &artifacts.join(format!("{case_id}.manifest.json")),
            &case_manifest,
        )?;
        let case = GeneratedCase {
            case_id: &case_id,
            case_index: index,
            fixture_id: "mvp-positive",
            fixture_file_name: "mvp-positive.wasm",
            kind: CaseKind::Execute,
            mutation: GeneratedMutation::None,
            module: &module,
            artifacts,
        };
        let comparison = run_generated_case(
            profile,
            bundle,
            wasmtime,
            fixture_root,
            expected_runner_digest,
            &case,
        )?;
        comparisons.push(minimize_generated_mismatch(
            profile,
            bundle,
            wasmtime,
            fixture_root,
            expected_runner_digest,
            &case,
            comparison,
        )?);
    }
    for index in 0..profile.generated_malformed_cases {
        let (case_manifest, module) =
            CaseManifest::generate(profile, index, CaseKind::Reject).map_err(Error::Invalid)?;
        case_manifest
            .verify(profile, &module)
            .map_err(Error::Invalid)?;
        let case_id = format!("generated-malformed-{index:04}");
        write_generated_module(artifacts, &case_id, &module)?;
        write_new_json(
            &artifacts.join(format!("{case_id}.manifest.json")),
            &case_manifest,
        )?;
        let case = GeneratedCase {
            case_id: &case_id,
            case_index: index,
            fixture_id: "mvp-negative",
            fixture_file_name: "mvp-negative.wasm",
            kind: CaseKind::Reject,
            mutation: GeneratedMutation::MalformedMagic,
            module: &module,
            artifacts,
        };
        let comparison = run_generated_case(
            profile,
            bundle,
            wasmtime,
            fixture_root,
            expected_runner_digest,
            &case,
        )?;
        comparisons.push(minimize_generated_mismatch(
            profile,
            bundle,
            wasmtime,
            fixture_root,
            expected_runner_digest,
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

fn admit_case_bytes(
    profile: &DifferentialProfile,
    bytes: &[u8],
    kind: CaseKind,
) -> Result<(), Error> {
    match (kind, admit_module(profile, bytes)) {
        (CaseKind::Execute, Ok(_)) => Ok(()),
        (CaseKind::Reject, Err(denial)) if denial.class == AdmissionClass::Malformed => Ok(()),
        (_, Err(denial)) => Err(Error::Invalid(format!("module admission: {denial:?}"))),
        (CaseKind::Reject, Ok(_)) => Err(Error::Invalid(
            "negative corpus module unexpectedly admitted".into(),
        )),
    }
}

fn run_generated_case(
    profile: &DifferentialProfile,
    bundle: &Path,
    wasmtime: &Path,
    fixture_root: &Path,
    expected_runner_digest: &str,
    case: &GeneratedCase<'_>,
) -> Result<CaseComparison, Error> {
    admit_case_bytes(profile, case.module, case.kind)?;
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
    let report = run_spacewasm(profile, bundle, temporary.path(), expected_runner_digest)?;
    let spacewasm = normalize_spacewasm(case.fixture_id, &report).map_err(Error::Invalid)?;
    let raw = run_wasmtime(
        profile,
        wasmtime,
        &module_path,
        profile.bounds.wasmtime_fuel,
    )?;
    let wasmtime_observation = normalize_tool(WASMTIME_ENGINE, case.kind, &raw);
    let mut comparison = compare_case(
        String::from(case.case_id),
        case.kind,
        blake3::hash(case.module).to_hex().to_string(),
        spacewasm,
        wasmtime_observation,
    )
    .map_err(Error::Invalid)?;
    comparison.raw_observation_refs = vec![
        retain_raw(case.artifacts, &report)?,
        retain_raw(case.artifacts, &raw)?,
    ];
    Ok(comparison)
}

fn retain_raw(directory: &Path, value: &impl serde::Serialize) -> Result<String, Error> {
    let bytes = serde_json::to_vec(value).map_err(|source| Error::Json {
        path: directory.to_path_buf(),
        source,
    })?;
    let digest = blake3::hash(&bytes).to_hex().to_string();
    let path = directory.join(format!("raw-{digest}.json"));
    let mut temporary =
        tempfile::NamedTempFile::new_in(directory).map_err(|source| io_error(&path, source))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| io_error(&path, source))?;
    match temporary.persist_noclobber(&path) {
        Ok(_) => sync_parent(&path)?,
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_bounded(&path, bytes.len() as u64)? != bytes {
                return Err(Error::Invalid("raw content-address collision".into()));
            }
        }
        Err(error) => return Err(io_error(&path, error.error)),
    }
    Ok(digest)
}

fn minimize_generated_mismatch(
    profile: &DifferentialProfile,
    bundle: &Path,
    wasmtime: &Path,
    fixture_root: &Path,
    expected_runner_digest: &str,
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
    let mut accepted_steps = Vec::new();
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
            artifacts: original_case.artifacts,
        };
        let candidate_comparison = run_generated_case(
            profile,
            bundle,
            wasmtime,
            fixture_root,
            expected_runner_digest,
            &candidate_case,
        )?;
        let replayed = run_generated_case(
            profile,
            bundle,
            wasmtime,
            fixture_root,
            expected_runner_digest,
            &candidate_case,
        )?;
        if candidate_module.len() < minimized_module.len()
            && same_mismatch_predicate(&minimized_comparison, &candidate_comparison)
            && same_mismatch_predicate(&candidate_comparison, &replayed)
        {
            let predicate = |comparison: &CaseComparison| {
                chaoscontrol_wasm_differential::rail::FailurePredicate {
                    profile_identity: profile_identity(profile).expect("admitted profile identity"),
                    spacewasm_runtime: profile.spacewasm_runner_blake3.clone(),
                    wasmtime_runtime: profile.wasmtime_version.clone(),
                    boundary: comparison.case_kind,
                    class: chaoscontrol_wasm_differential::rail::EvidenceClass::RuntimeDivergence,
                    first_difference: comparison
                        .first_difference
                        .clone()
                        .expect("mismatch difference"),
                }
            };
            if let Ok(step) = chaoscontrol_wasm_differential::rail::admit_shrink(
                profile,
                &minimized_module,
                &candidate_module,
                &predicate(&minimized_comparison),
                &predicate(&candidate_comparison),
                &predicate(&replayed),
            ) {
                accepted_steps.push(step);
                minimized_module = candidate_module;
                minimized_comparison = candidate_comparison;
            }
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
        accepted_steps,
        attempts,
        original_module_blake3: original_digest,
        minimized_module_blake3: blake3::hash(&minimized_module).to_hex().to_string(),
        minimized_module_hex: encode_hex(&minimized_module),
    });
    Ok(minimized_comparison)
}

fn stage_fixture_set(source: &Path, destination: &Path, maximum_bytes: u64) -> Result<(), Error> {
    for file_name in FIXTURE_FILES {
        let bytes = read_bounded(&source.join(file_name), maximum_bytes)?;
        let path = destination.join(file_name);
        fs::write(&path, &bytes).map_err(|source| io_error(&path, source))?;
    }
    Ok(())
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

fn stage_bundled_runner(
    bundle: &Path,
    runner_path: &Path,
    expected_runner_digest: &str,
    maximum_bytes: u64,
) -> Result<(), Error> {
    let bytes = read_bounded(&bundle.join(HOST_RUNNER_PATH), maximum_bytes)?;
    let actual_digest = blake3::hash(&bytes).to_hex().to_string();
    if actual_digest != expected_runner_digest {
        return Err(Error::Invalid(String::from(
            "host runner changed after bundle admission",
        )));
    }
    std::fs::write(runner_path, &bytes).map_err(|source| io_error(runner_path, source))?;
    let permissions = fs::Permissions::from_mode(STAGED_RUNNER_MODE);
    fs::set_permissions(runner_path, permissions).map_err(|source| io_error(runner_path, source))
}

fn run_spacewasm(
    profile: &DifferentialProfile,
    bundle: &Path,
    fixture_root: &Path,
    expected_runner_digest: &str,
) -> Result<MantleRunnerReport, Error> {
    let temporary = TempDir::new()
        .map_err(|source| io_error(Path::new("temporary runner directory"), source))?;
    let report_path = temporary.path().join(SPACEWASM_REPORT_FILE);
    let runner_path = temporary.path().join(STAGED_RUNNER_FILE);
    stage_bundled_runner(
        bundle,
        &runner_path,
        expected_runner_digest,
        profile.bounds.maximum_bundle_member_bytes,
    )?;
    let result = run_command(
        &runner_path,
        &[fixture_root.as_os_str(), report_path.as_os_str()],
        profile,
        Some(profile.bounds.maximum_interpreter_allocation_bytes),
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
    let result = run_command(wasmtime, &arguments, profile, None)?;
    Ok(ToolObservation {
        success: result.status.success(),
        timed_out: result.timed_out,
        stderr: result.stderr,
        stdout: result.stdout,
    })
}

fn read_wasmtime_version(profile: &DifferentialProfile, wasmtime: &Path) -> Result<String, Error> {
    let result = run_command(
        wasmtime,
        &[std::ffi::OsStr::new("--version")],
        profile,
        None,
    )?;
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

fn apply_command_limits(
    command: &mut Command,
    maximum_output_bytes: u64,
    maximum_data_bytes: Option<u64>,
) -> Result<(), Error> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let maximum_output_bytes = libc::rlim_t::try_from(maximum_output_bytes)
            .map_err(|_| Error::Invalid(String::from("output limit exceeds rlim_t")))?;
        let data_limit = maximum_data_bytes
            .map(libc::rlim_t::try_from)
            .transpose()
            .map_err(|_| Error::Invalid(String::from("data limit exceeds rlim_t")))?;
        // The pinned runner allocates through the system allocator. On Linux,
        // RLIMIT_DATA also bounds anonymous mmap allocations. RLIMIT_FSIZE
        // keeps child stdout, stderr, and reports from filling the disk.
        // Only async-signal-safe syscalls run between fork and exec.
        unsafe {
            command.pre_exec(move || {
                let file_limit = libc::rlimit {
                    rlim_cur: maximum_output_bytes,
                    rlim_max: maximum_output_bytes,
                };
                if libc::setrlimit(libc::RLIMIT_FSIZE, &file_limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if let Some(limit) = data_limit {
                    let data_limit = libc::rlimit {
                        rlim_cur: limit,
                        rlim_max: limit,
                    };
                    if libc::setrlimit(libc::RLIMIT_DATA, &data_limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        if let Some(maximum_data_bytes) = maximum_data_bytes {
            return Err(Error::Invalid(format!(
                "interpreter data limit {maximum_data_bytes} is unsupported on this platform"
            )));
        }
        Ok(())
    }
}

fn run_command(
    program: &Path,
    arguments: &[&std::ffi::OsStr],
    profile: &DifferentialProfile,
    maximum_data_bytes: Option<u64>,
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
    apply_command_limits(
        &mut command,
        profile.bounds.maximum_output_bytes,
        maximum_data_bytes,
    )?;
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

fn open_regular_input(path: &Path) -> Result<(File, fs::Metadata), Error> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|source| io_error(path, source))?;
    let metadata = file.metadata().map_err(|source| io_error(path, source))?;
    if !metadata.file_type().is_file() {
        return Err(Error::Invalid(format!(
            "path is not one regular no-follow file: {}",
            path.display()
        )));
    }
    Ok((file, metadata))
}

fn read_bounded(path: &Path, maximum_bytes: u64) -> Result<Vec<u8>, Error> {
    let (file, metadata) = open_regular_input(path)?;
    if metadata.len() > maximum_bytes {
        return Err(Error::Invalid(format!(
            "file exceeds byte bound at {}: actual={} maximum={maximum_bytes}",
            path.display(),
            metadata.len()
        )));
    }
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(usize::MAX)
        .min(usize::try_from(maximum_bytes).unwrap_or(usize::MAX));
    let mut bytes = Vec::with_capacity(capacity);
    file.take(maximum_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| io_error(path, source))?;
    if bytes.len() as u64 > maximum_bytes {
        return Err(Error::Invalid(format!(
            "file exceeds byte bound at {}: maximum={maximum_bytes}",
            path.display()
        )));
    }
    Ok(bytes)
}

fn hash_file(path: &Path, maximum_bytes: u64) -> Result<String, Error> {
    let (mut input, metadata) = open_regular_input(path)?;
    if metadata.len() > maximum_bytes {
        return Err(Error::Invalid(format!(
            "file exceeds hash bound: {}",
            path.display()
        )));
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];
    let mut total = 0_u64;
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|source| io_error(path, source))?;
        if count == 0 {
            break;
        }
        total = total.checked_add(count as u64).ok_or_else(|| {
            Error::Invalid(format!("hash byte count overflow: {}", path.display()))
        })?;
        if total > maximum_bytes {
            return Err(Error::Invalid(format!(
                "file exceeds hash bound: {}",
                path.display()
            )));
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn sync_parent(path: &Path) -> Result<(), Error> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| io_error(parent, source))
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
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })?;
    bytes.push(b'\n');
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|source| io_error(path, source))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| io_error(path, source))?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| io_error(path, error.error))?;
    sync_parent(path)
}

fn io_error(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn wasm_publication_rejects_non_regular_targets() {
        use std::os::unix::ffi::OsStrExt;

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        std::fs::write(&target, b"original").unwrap();
        let link = root.path().join("report.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(write_new_json(&link, &serde_json::json!({"status": "passed"})).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original");

        let fifo = root.path().join("report.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(write_new_json(&fifo, &serde_json::json!({"status": "passed"})).is_err());

        let module_target = root.path().join("target.wasm");
        std::fs::write(&module_target, b"original").unwrap();
        std::os::unix::fs::symlink(&module_target, root.path().join("case.wasm")).unwrap();
        assert!(write_generated_module(root.path(), "case", b"wasm").is_err());
        assert_eq!(std::fs::read(&module_target).unwrap(), b"original");
    }

    #[test]
    fn failed_wasm_publications_leave_no_temporary_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("case.wasm")).unwrap();
        std::fs::create_dir(root.path().join("report.json")).unwrap();
        let before = std::fs::read_dir(root.path()).unwrap().count();

        assert!(write_generated_module(root.path(), "case", b"wasm").is_err());
        assert!(write_new_json(
            &root.path().join("report.json"),
            &serde_json::json!({"status": "passed"})
        )
        .is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), before);
    }

    #[test]
    fn generated_module_publication_is_atomic_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        write_generated_module(root.path(), "case", b"wasm").unwrap();
        assert_eq!(
            std::fs::read(root.path().join("case.wasm")).unwrap(),
            b"wasm"
        );
        assert!(write_generated_module(root.path(), "case", b"other").is_err());
    }

    #[test]
    fn raw_retention_is_atomic_and_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let value = serde_json::json!({"status": "passed"});
        let first = retain_raw(root.path(), &value).unwrap();
        let second = retain_raw(root.path(), &value).unwrap();
        assert_eq!(first, second);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn new_json_publication_is_atomic_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("report.json");
        write_new_json(&path, &serde_json::json!({"status": "passed"})).unwrap();
        assert!(write_new_json(&path, &serde_json::json!({"status": "failed"})).is_err());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap()
                ["status"],
            "passed"
        );
    }

    #[test]
    fn fixture_set_is_staged_and_rejects_symlink_members() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        std::fs::create_dir(&source).unwrap();
        for (index, file_name) in FIXTURE_FILES.iter().enumerate() {
            std::fs::write(source.join(file_name), format!("fixture-{index}")).unwrap();
        }
        let destination = root.path().join("destination");
        std::fs::create_dir(&destination).unwrap();
        stage_fixture_set(&source, &destination, 1024).unwrap();
        for (index, file_name) in FIXTURE_FILES.iter().enumerate() {
            assert_eq!(
                std::fs::read(destination.join(file_name)).unwrap(),
                format!("fixture-{index}").into_bytes()
            );
        }

        let target = root.path().join("target");
        std::fs::write(&target, b"target").unwrap();
        let member = source.join(FIXTURE_FILES[0]);
        std::fs::remove_file(&member).unwrap();
        std::os::unix::fs::symlink(&target, &member).unwrap();
        assert!(stage_fixture_set(&source, &destination, 1024).is_err());
    }

    #[test]
    fn command_limits_bound_child_file_output() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("stdout");
        let file = File::create(&output).unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "head -c 4096 /dev/zero"])
            .stdout(Stdio::from(file));
        apply_command_limits(&mut command, 16, None).unwrap();
        let status = command.status().unwrap();
        assert!(!status.success());
        assert!(std::fs::metadata(&output).unwrap().len() <= 16);
    }

    #[test]
    fn staged_runner_requires_matching_regular_bundle_member() {
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("bundle");
        std::fs::create_dir(&bundle).unwrap();
        let runner = bundle.join(HOST_RUNNER_PATH);
        std::fs::create_dir_all(runner.parent().unwrap()).unwrap();
        std::fs::write(&runner, b"runner").unwrap();
        let expected = blake3::hash(b"runner").to_hex().to_string();
        let staged = root.path().join("staged-runner");
        stage_bundled_runner(&bundle, &staged, &expected, 1024).unwrap();
        assert_eq!(std::fs::read(&staged).unwrap(), b"runner");

        let target = root.path().join("target");
        std::fs::write(&target, b"runner").unwrap();
        std::fs::remove_file(&runner).unwrap();
        std::os::unix::fs::symlink(&target, &runner).unwrap();
        assert!(stage_bundled_runner(&bundle, &staged, &expected, 1024).is_err());

        std::fs::remove_file(&runner).unwrap();
        std::fs::write(&runner, b"different").unwrap();
        assert!(stage_bundled_runner(&bundle, &staged, &expected, 1024).is_err());
    }

    #[test]
    fn input_reads_accept_the_maximum_limit() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("input.json");
        fs::write(&path, b"{}").unwrap();
        assert_eq!(read_bounded(&path, u64::MAX).unwrap(), b"{}");
    }

    #[test]
    fn input_reads_reject_symlinks_and_fifos() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("input.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(read_bounded(&link, 16).is_err());
        assert!(hash_file(&link, 16).is_err());

        let fifo = root.path().join("input.fifo");
        let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(read_bounded(&fifo, 16).is_err());
    }
}
