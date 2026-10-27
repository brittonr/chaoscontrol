//! Bounded, diagnostic-only differential evidence for the SpaceWasm MVP cohort.
//!
//! Pure admission, generation, normalization, comparison, and receipt identity
//! logic lives here. Filesystem and process effects remain in the binary shell.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
pub mod admission;
pub mod rail;

pub const PROFILE_SCHEMA: &str = "chaoscontrol.spacewasm-mvp-differential-profile.v1";
pub const REPORT_SCHEMA: &str = "chaoscontrol.spacewasm-mvp-differential-report.v1";
pub const MANTLE_MANIFEST_SCHEMA: &str = "mantle-spacewasm-reference-bundle-v1";
pub const MANTLE_RUNNER_REPORT_SCHEMA: &str = "mantle-spacewasm-runner-report-v1";
/// Stable test-report schema emitted by the pinned Mantle producer (ADR 0079).
pub const MANTLE_STABLE_REPORT_SCHEMA: &str = "mantle-spacewasm-stable-report-v1";
/// Admitted libtest JSON grammar version of the stable report.
pub const MANTLE_STABLE_REPORT_ENCODING_VERSION: u32 = 1;
/// Mirrors Mantle `MAX_STABLE_TESTS`: the producer never admits more records.
const MAXIMUM_STABLE_TESTS: usize = 4_096;
/// Bundle role the producer assigns to test receipts and stable reports.
const STABLE_EVIDENCE_ROLE: &str = "check-receipt";
/// Receipt process status the producer writes after a passing test command.
const STABLE_RECEIPT_STATUS_PASSED: &str = "passed";
/// Mantle hashes the compact identity input as emitted by `jq -c`, which
/// terminates the document with one line feed.
const STABLE_IDENTITY_INPUT_TERMINATOR: &[u8] = b"\n";
/// Raw run captures and run records belong to the separate Mantle run archive
/// and must never be admitted as stable bundle members.
const RAW_CAPTURE_PATH_SUFFIXES: [&str; 3] = ["stdout.txt", "stderr.txt", "run-record.json"];
const RAW_CAPTURE_PATH_PREFIX: &str = "runs/";
const RAW_CAPTURE_ROLES: [&str; 3] = ["raw-capture", "run-archive", "run-record"];
// The owned resume probe cannot enforce a larger allocator ceiling.
const MAXIMUM_INTERPRETER_ALLOCATION_BYTES: u64 = 16_777_216;

/// One producer test suite whose stable report and receipt the bundle must carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StableEvidenceSuite {
    pub suite: &'static str,
    pub receipt_path: &'static str,
    pub stable_report_path: &'static str,
}

/// Every stable test producer of the pinned Mantle bundle.
pub const STABLE_EVIDENCE_SUITES: [StableEvidenceSuite; 2] = [
    StableEvidenceSuite {
        suite: "upstream-unit-tests",
        receipt_path: "reports/checks/upstream-unit-tests.json",
        stable_report_path: "reports/checks/upstream-unit-tests.stable-report.json",
    },
    StableEvidenceSuite {
        suite: "upstream-spectest-address",
        receipt_path: "reports/checks/upstream-spectest-address.json",
        stable_report_path: "reports/checks/upstream-spectest-address.stable-report.json",
    },
];
const GIT_REVISION_HEX_LENGTH: usize = 40;
const BLAKE3_HEX_LENGTH: usize = 64;
const MAXIMUM_IDENTIFIER_BYTES: usize = 128;
const FIXED_CASE_COUNT: usize = 6;
const GENERATOR_IDENTITY_INPUT_BYTES: usize = std::mem::size_of::<u64>() * 2;
const WASM_MODULE_PREFIX: [u8; 27] = [
    0x00, 0x61, 0x73, 0x6d, // magic
    0x01, 0x00, 0x00, 0x00, // version
    0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type: () -> ()
    0x03, 0x02, 0x01, 0x00, // function: type 0
    0x07, 0x07, 0x01, 0x03, b'r', b'u', b'n', 0x00, 0x00, // export run
];
const WASM_CODE_SECTION: u8 = 0x0a;
const WASM_ZERO_LOCAL_DECLARATIONS: u8 = 0x00;
const WASM_I32_CONST: u8 = 0x41;
const WASM_DROP: u8 = 0x1a;
const GENERATED_I32_VALUE_CARDINALITY: usize = 64;
const WASM_END: u8 = 0x0b;
const WASM_ONE_FUNCTION: u8 = 0x01;
const INVALID_MAGIC_BYTE: u8 = 0xff;
const LEB128_DATA_MASK: u32 = 0x7f;
const LEB128_CONTINUATION_BIT: u8 = 0x80;
const HEX_ALPHABET_LENGTH: usize = 16;
const HEX_CHARACTERS_PER_BYTE: usize = 2;
const HIGH_NIBBLE_SHIFT: u32 = 4;
const NIBBLE_MASK: u8 = 0x0f;
pub const REQUIRED_FEATURES: [&str; 2] = ["mutable-globals", "wasm1"];
const REQUIRED_CHUNK_SCHEDULES: [&str; 2] = ["complete", "one-byte"];
const REQUIRED_CORPUS_CLASSES: [&str; 8] = [
    "completion",
    "fuel-exhaustion",
    "generated-malformed",
    "generated-valid",
    "malformed",
    "streaming-malformed",
    "streaming-valid",
    "trap-unreachable",
];
const REQUIRED_OBSERVATION_FIELDS: [&str; 10] = [
    "outcome",
    "resource-class",
    "return-values",
    "state-identity-blake3",
    "trap-class",
    "validation-class",
    "link-class",
    "preserves-output",
    "hostcall-transcript",
    "final-memory-blake3",
];
const REQUIRED_EXPORT: &str = "run";
const REQUIRED_MEMORY_GROW_POLICY: &str = "not-admitted";
const REQUIRED_WASMTIME_PROPOSALS: &str = "all-proposals-disabled";
const REQUIRED_HOST_AUTHORITY: &str = "none";
pub const REQUIRED_NON_CLAIMS: [&str; 8] = [
    "not-flight-qualification",
    "not-memory-safety",
    "not-production-readiness",
    "not-release-eligibility",
    "not-sandbox-effectiveness",
    "not-spacewasm-correctness",
    "not-spacewasm-wasmtime-equivalence",
    "not-webassembly-conformance",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifferentialProfile {
    pub schema: String,
    pub profile_id: String,
    pub mantle_revision: String,
    pub mantle_profile_id: String,
    pub spacewasm_revision: String,
    pub spacewasm_runner_blake3: String,
    pub wasmtime_version: String,
    pub wasm_tools_version: String,
    pub retention: String,
    pub bundle_manifest_blake3: String,
    pub bundle_identity_blake3: String,
    pub generator_id: String,
    pub seed: u64,
    pub generated_valid_cases: usize,
    pub generated_malformed_cases: usize,
    pub feature_intersection: Vec<String>,
    pub module_abi: ModuleAbiProfile,
    pub runtime: RuntimeProfile,
    pub chunk_schedules: Vec<String>,
    pub corpus_classes: Vec<String>,
    pub observation_fields: Vec<String>,
    pub bounds: DifferentialBounds,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleAbiProfile {
    pub required_export: String,
    pub parameter_count: usize,
    pub result_count: usize,
    pub maximum_imports: usize,
    pub maximum_memories: usize,
    pub maximum_tables: usize,
    pub maximum_host_functions: usize,
    pub maximum_linear_memory_bytes: u64,
    pub memory_grow: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProfile {
    pub spacewasm_resume_segment_fuel: u64,
    pub maximum_resume_segments: usize,
    pub spacewasm_code_pages: usize,
    pub spacewasm_control_frames: usize,
    pub spacewasm_stack_words: usize,
    pub spacewasm_initialization_fuel: u64,
    pub spacewasm_run_fuel: u64,
    pub wasmtime_proposals: String,
    pub host_authority: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifferentialBounds {
    pub maximum_cases: usize,
    pub maximum_input_bytes: u64,
    pub maximum_bundle_members: usize,
    pub maximum_bundle_member_bytes: u64,
    pub maximum_bundle_total_bytes: u64,
    pub maximum_process_milliseconds: u64,
    pub maximum_output_bytes: u64,
    pub wasmtime_fuel: u64,
    pub maximum_generated_instruction_pairs: usize,
    pub maximum_shrink_attempts: usize,
    pub maximum_functions: usize,
    pub maximum_types: usize,
    pub maximum_exports: usize,
    pub maximum_globals: usize,
    pub maximum_interpreter_allocation_bytes: u64,
    pub maximum_hostcall_bytes: u64,
    pub maximum_chunks: usize,
    pub maximum_concurrent_cases: usize,
    pub maximum_samples: usize,
    pub maximum_retained_artifacts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub schema: String,
    pub profile_id: String,
    pub profile_identity_blake3: String,
    pub cohort_identity_blake3: String,
    pub members: Vec<BundleMember>,
    pub parent_edges: Vec<ParentEdge>,
    pub non_claims: Vec<String>,
    pub bundle_identity_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleMember {
    pub path: String,
    pub role: String,
    pub digest_blake3: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParentEdge {
    pub parent_path: String,
    pub child_path: String,
    pub relation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MantleRunnerReport {
    pub results: Vec<MantleFixtureResult>,
    pub schema: String,
    pub source_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MantleFixtureResult {
    pub fixture_id: String,
    pub result_code: String,
    pub status: String,
}

/// Canonical Mantle stable test report (`mantle-spacewasm-stable-report-v1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MantleStableReport {
    pub schema: String,
    pub suite: String,
    pub command: String,
    pub tests: Vec<MantleStableTest>,
    pub encoding_version: u32,
    pub stable_identity_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MantleStableTest {
    pub name: String,
    pub status: MantleStableTestStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MantleStableTestStatus {
    Passed,
    Failed,
    Skipped,
}

/// Mantle test receipt that binds the command status to its stable report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MantleStableReceipt {
    pub command: String,
    pub status: String,
    pub stable_report: MantleStableReport,
}

/// Identity input in the producer's field order; timing never enters it.
#[derive(Serialize)]
struct StableReportIdentityInput<'a> {
    schema: &'a str,
    suite: &'a str,
    command: &'a str,
    tests: &'a [MantleStableTest],
    encoding_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaseKind {
    Execute,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutcomeClass {
    Completed,
    Rejected,
    Trapped,
    ResourceExhausted,
    TimedOut,
    EngineError,
}

/// Canonical semantic surfaces. Empty transcripts/output and absent memory are
/// justified only by per-binary admission to the hostless void-export profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationSurfaces {
    pub validation_class: String,
    pub link_class: String,
    pub preserves_output: Vec<String>,
    pub hostcall_transcript: Vec<String>,
    pub final_memory_blake3: Option<String>,
}

fn hostless_surfaces(validation: &str, link: &str) -> ObservationSurfaces {
    ObservationSurfaces {
        validation_class: validation.into(),
        link_class: link.into(),
        preserves_output: vec![],
        hostcall_transcript: vec![],
        final_memory_blake3: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedObservation {
    pub engine: String,
    pub outcome: OutcomeClass,
    pub return_values: Vec<String>,
    pub trap_class: Option<String>,
    pub state_identity_blake3: Option<String>,
    pub resource_class: Option<String>,
    pub surfaces: ObservationSurfaces,
    /// Retained for diagnostics, deliberately excluded from semantic equality.
    pub raw_counters: std::collections::BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseComparison {
    pub case_id: String,
    pub case_kind: CaseKind,
    pub module_blake3: String,
    pub spacewasm: NormalizedObservation,
    pub raw_observation_refs: Vec<String>,
    pub wasmtime: NormalizedObservation,
    pub verdict: ComparisonVerdict,
    pub first_difference: Option<FirstDifference>,
    pub minimization: Option<MinimizationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComparisonVerdict {
    Match,
    Mismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirstDifference {
    pub field: String,
    pub spacewasm: String,
    pub wasmtime: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinimizationEvidence {
    pub predicate_field: String,
    pub accepted_steps: Vec<rail::ShrinkStep>,
    pub attempts: usize,
    pub original_module_blake3: String,
    pub minimized_module_blake3: String,
    pub minimized_module_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifferentialReport {
    pub schema: String,
    pub profile_id: String,
    pub profile_identity_blake3: String,
    pub bundle_identity_blake3: String,
    pub bundle_manifest_blake3: String,
    pub spacewasm_runtime_blake3: String,
    pub wasmtime_version: String,
    pub seed: u64,
    pub comparisons: Vec<CaseComparison>,
    pub mismatch_count: usize,
    pub evidence: Vec<rail::DiagnosticEvidence>,
    pub verdict: String,
    pub non_claims: Vec<String>,
    pub report_identity_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolObservation {
    pub success: bool,
    pub timed_out: bool,
    pub stderr: String,
    pub stdout: String,
}

// r[impl chaoscontrol.spacewasm_mvp.profile]
// r[impl chaoscontrol.spacewasm_mvp.bounds]
// r[impl chaoscontrol.spacewasm_mvp.functional_core]
pub fn validate_profile(profile: &DifferentialProfile) -> Result<(), String> {
    if profile.schema != PROFILE_SCHEMA {
        return Err(format!("unsupported profile schema: {}", profile.schema));
    }
    validate_identifier("profile_id", &profile.profile_id)?;
    validate_identifier("mantle_profile_id", &profile.mantle_profile_id)?;
    validate_identifier("generator_id", &profile.generator_id)?;
    validate_hex(
        "mantle_revision",
        &profile.mantle_revision,
        GIT_REVISION_HEX_LENGTH,
    )?;
    validate_hex(
        "spacewasm_revision",
        &profile.spacewasm_revision,
        GIT_REVISION_HEX_LENGTH,
    )?;
    validate_hex(
        "spacewasm_runner_blake3",
        &profile.spacewasm_runner_blake3,
        BLAKE3_HEX_LENGTH,
    )?;
    validate_bounded_text("wasmtime_version", &profile.wasmtime_version)?;
    validate_bounded_text("wasm_tools_version", &profile.wasm_tools_version)?;
    if profile.wasm_tools_version != "wasm-tools 1.245.0"
        || profile.wasmtime_version != "wasmtime 41.0.3"
        || profile.generator_id != "chaoscontrol-mvp-instruction-v1"
    {
        return Err("runtime, tool or generator cohort drift".into());
    }
    if profile.retention != "all-cases-and-raw-observations" {
        return Err("retention policy drift".into());
    }
    validate_hex(
        "bundle_manifest_blake3",
        &profile.bundle_manifest_blake3,
        BLAKE3_HEX_LENGTH,
    )?;
    validate_hex(
        "bundle_identity_blake3",
        &profile.bundle_identity_blake3,
        BLAKE3_HEX_LENGTH,
    )?;
    validate_exact_set(
        "feature_intersection",
        &profile.feature_intersection,
        &REQUIRED_FEATURES,
    )?;
    validate_exact_set(
        "chunk_schedules",
        &profile.chunk_schedules,
        &REQUIRED_CHUNK_SCHEDULES,
    )?;
    validate_exact_set(
        "corpus_classes",
        &profile.corpus_classes,
        &REQUIRED_CORPUS_CLASSES,
    )?;
    validate_exact_set(
        "observation_fields",
        &profile.observation_fields,
        &REQUIRED_OBSERVATION_FIELDS,
    )?;
    validate_module_abi(&profile.module_abi)?;
    validate_runtime(&profile.runtime)?;
    let non_claims = profile
        .non_claims
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for required in REQUIRED_NON_CLAIMS {
        if !non_claims.contains(required) {
            return Err(format!("missing required non-claim: {required}"));
        }
    }
    if non_claims.len() != profile.non_claims.len() {
        return Err(String::from("non_claims contains a duplicate"));
    }
    let generated_cases = profile
        .generated_valid_cases
        .checked_add(profile.generated_malformed_cases)
        .ok_or_else(|| String::from("generated case count overflow"))?;
    let total_cases = generated_cases
        .checked_add(FIXED_CASE_COUNT)
        .ok_or_else(|| String::from("total case count overflow"))?;
    if profile.generated_valid_cases == 0 || profile.generated_malformed_cases == 0 {
        return Err(String::from(
            "both generated positive and negative cases are required",
        ));
    }
    if total_cases > profile.bounds.maximum_cases {
        return Err(format!("case count {total_cases} exceeds maximum_cases"));
    }
    if total_cases > profile.bounds.maximum_samples {
        return Err("sample count exceeds maximum_samples".into());
    }
    validate_bounds(&profile.bounds)
}

// r[impl chaoscontrol.spacewasm_mvp.execution]
fn validate_module_abi(abi: &ModuleAbiProfile) -> Result<(), String> {
    let hostless = abi.required_export == REQUIRED_EXPORT
        && abi.parameter_count == 0
        && abi.result_count == 0
        && abi.maximum_imports == 0
        && abi.maximum_memories == 0
        && abi.maximum_tables == 0
        && abi.maximum_host_functions == 0
        && abi.maximum_linear_memory_bytes == 0
        && abi.memory_grow == REQUIRED_MEMORY_GROW_POLICY;
    if hostless {
        Ok(())
    } else {
        Err(String::from(
            "module_abi must be the exact hostless run-export profile",
        ))
    }
}

fn validate_runtime(runtime: &RuntimeProfile) -> Result<(), String> {
    if runtime.spacewasm_code_pages != 64
        || runtime.spacewasm_control_frames != 64
        || runtime.spacewasm_stack_words != 1024
        || runtime.spacewasm_initialization_fuel != 1024
        || runtime.spacewasm_run_fuel != 4096
        || runtime.spacewasm_resume_segment_fuel != 1
        || runtime.maximum_resume_segments != 4096
    {
        return Err("SpaceWasm probe resource configuration drift".into());
    }
    if runtime.spacewasm_resume_segment_fuel == 0 || runtime.maximum_resume_segments == 0 {
        return Err(String::from("resume bounds must be positive"));
    }
    if runtime.wasmtime_proposals != REQUIRED_WASMTIME_PROPOSALS {
        return Err(String::from("Wasmtime proposal policy drift"));
    }
    if runtime.host_authority != REQUIRED_HOST_AUTHORITY {
        return Err(String::from("host authority must remain none"));
    }
    Ok(())
}

fn validate_exact_set(field: &str, actual: &[String], required: &[&str]) -> Result<(), String> {
    let actual_set = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let required_set = required.iter().copied().collect::<BTreeSet<_>>();
    if actual_set == required_set && actual.len() == required.len() {
        Ok(())
    } else {
        Err(format!("{field} does not match the exact admitted set"))
    }
}

fn validate_bounded_text(field: &str, value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= MAXIMUM_IDENTIFIER_BYTES
        && !value.bytes().any(|byte| byte.is_ascii_control());
    if valid {
        Ok(())
    } else {
        Err(format!("{field} is not bounded printable text"))
    }
}

// r[impl chaoscontrol.spacewasm_mvp.bounds]
fn validate_bounds(bounds: &DifferentialBounds) -> Result<(), String> {
    let positive = [
        bounds.maximum_cases as u64,
        bounds.maximum_input_bytes,
        bounds.maximum_bundle_members as u64,
        bounds.maximum_bundle_member_bytes,
        bounds.maximum_bundle_total_bytes,
        bounds.maximum_process_milliseconds,
        bounds.maximum_output_bytes,
        bounds.wasmtime_fuel,
        bounds.maximum_generated_instruction_pairs as u64,
        bounds.maximum_shrink_attempts as u64,
        bounds.maximum_functions as u64,
        bounds.maximum_types as u64,
        bounds.maximum_exports as u64,
        bounds.maximum_globals as u64,
        bounds.maximum_interpreter_allocation_bytes,
        bounds.maximum_hostcall_bytes,
        bounds.maximum_chunks as u64,
        bounds.maximum_concurrent_cases as u64,
        bounds.maximum_samples as u64,
        bounds.maximum_retained_artifacts as u64,
    ];
    if positive.contains(&0) {
        return Err(String::from("all differential bounds must be positive"));
    }
    let capped = [
        (bounds.maximum_cases as u64, 1024),
        (bounds.maximum_input_bytes, 16777216),
        (bounds.maximum_bundle_members as u64, 4096),
        (bounds.maximum_bundle_member_bytes, 1073741824),
        (bounds.maximum_bundle_total_bytes, 4294967296),
        (bounds.maximum_process_milliseconds, 300000),
        (bounds.maximum_output_bytes, 16777216),
        (bounds.wasmtime_fuel, 1000000000),
        (bounds.maximum_generated_instruction_pairs as u64, 4096),
        (bounds.maximum_shrink_attempts as u64, 4096),
        (bounds.maximum_functions as u64, 65536),
        (bounds.maximum_types as u64, 65536),
        (bounds.maximum_exports as u64, 65536),
        (
            bounds.maximum_interpreter_allocation_bytes,
            MAXIMUM_INTERPRETER_ALLOCATION_BYTES,
        ),
        (bounds.maximum_hostcall_bytes, 16777216),
        (bounds.maximum_concurrent_cases as u64, 64),
        (bounds.maximum_samples as u64, 1024),
        (bounds.maximum_retained_artifacts as u64, 65536),
    ];
    if capped.iter().any(|(actual, maximum)| actual > maximum) {
        return Err("differential bound exceeds supported capacity".into());
    }
    if bounds.maximum_globals > 64 || bounds.maximum_chunks > 4096 {
        return Err("interpreter structural bound exceeds admitted runtime capacity".into());
    }
    if bounds.maximum_bundle_member_bytes > bounds.maximum_bundle_total_bytes {
        return Err(String::from(
            "maximum bundle member bytes exceeds total bytes",
        ));
    }
    Ok(())
}

// r[impl chaoscontrol.spacewasm_mvp.materialization]
pub fn validate_bundle_manifest(
    profile: &DifferentialProfile,
    manifest: &BundleManifest,
) -> Result<(), String> {
    if manifest.schema != MANTLE_MANIFEST_SCHEMA {
        return Err(format!(
            "unsupported Mantle bundle schema: {}",
            manifest.schema
        ));
    }
    if manifest.profile_id != profile.mantle_profile_id {
        return Err(String::from(
            "Mantle profile identity does not match the admitted profile",
        ));
    }
    if manifest.bundle_identity_blake3 != profile.bundle_identity_blake3 {
        return Err(String::from(
            "Mantle bundle identity does not match the admitted profile",
        ));
    }
    if manifest.members.is_empty() || manifest.members.len() > profile.bounds.maximum_bundle_members
    {
        return Err(String::from(
            "Mantle bundle member count is outside the admitted bound",
        ));
    }
    let mut paths = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for member in &manifest.members {
        validate_relative_path(&member.path)?;
        validate_identifier("bundle member role", &member.role)?;
        validate_hex(
            "bundle member digest",
            &member.digest_blake3,
            BLAKE3_HEX_LENGTH,
        )?;
        if !paths.insert(member.path.as_str()) {
            return Err(format!("duplicate bundle member path: {}", member.path));
        }
        if is_raw_capture_member(member) {
            return Err(format!(
                "Mantle bundle carries a raw run capture member: {}",
                member.path
            ));
        }
        if member.size_bytes > profile.bounds.maximum_bundle_member_bytes {
            return Err(format!("bundle member exceeds byte bound: {}", member.path));
        }
        total_bytes = total_bytes
            .checked_add(member.size_bytes)
            .ok_or_else(|| String::from("bundle total byte count overflow"))?;
    }
    if total_bytes > profile.bounds.maximum_bundle_total_bytes {
        return Err(String::from("bundle exceeds total byte bound"));
    }
    let runner = manifest
        .members
        .iter()
        .find(|member| member.role == "host-runner")
        .ok_or_else(|| String::from("Mantle bundle is missing required role: host-runner"))?;
    if runner.digest_blake3 != profile.spacewasm_runner_blake3 {
        return Err(String::from("SpaceWasm runner identity drift"));
    }
    for required_role in ["fixture-artifact", "profile-export", "source-archive"] {
        if !manifest
            .members
            .iter()
            .any(|member| member.role == required_role)
        {
            return Err(format!(
                "Mantle bundle is missing required role: {required_role}"
            ));
        }
    }
    for suite in STABLE_EVIDENCE_SUITES {
        for required_path in [suite.receipt_path, suite.stable_report_path] {
            let member = find_member(manifest, required_path).ok_or_else(|| {
                format!("Mantle bundle is missing required stable evidence member: {required_path}")
            })?;
            if member.role != STABLE_EVIDENCE_ROLE {
                return Err(format!(
                    "Mantle stable evidence member has role {} instead of {STABLE_EVIDENCE_ROLE}: {required_path}",
                    member.role
                ));
            }
        }
    }
    let has_license = manifest.members.iter().any(|member| {
        member.role.contains("license")
            || member.path.rsplit('/').next().is_some_and(|name| {
                name == "LICENSE" || name.starts_with("LICENSE.") || name == "COPYING"
            })
    });
    if !has_license {
        return Err("Mantle bundle is missing required license member".into());
    }
    let mut edges = BTreeSet::new();
    let mut incident = BTreeSet::new();
    let mut indegree = std::collections::BTreeMap::new();
    for path in &paths {
        indegree.insert(*path, 0usize);
    }
    for edge in &manifest.parent_edges {
        validate_identifier("parent edge relation", &edge.relation)?;
        if edge.parent_path == edge.child_path
            || !paths.contains(edge.parent_path.as_str())
            || !paths.contains(edge.child_path.as_str())
            || !edges.insert((
                edge.parent_path.as_str(),
                edge.child_path.as_str(),
                edge.relation.as_str(),
            ))
        {
            return Err("Mantle bundle parent edge is dangling, self-linked or duplicated".into());
        }
        incident.insert(edge.parent_path.as_str());
        incident.insert(edge.child_path.as_str());
        *indegree
            .get_mut(edge.child_path.as_str())
            .expect("validated child") += 1;
    }
    if manifest
        .members
        .iter()
        .any(|member| !incident.contains(member.path.as_str()))
    {
        return Err("Mantle bundle parent graph omits a required member".into());
    }
    let mut ready: Vec<_> = indegree
        .iter()
        .filter_map(|(path, degree)| (*degree == 0).then_some(*path))
        .collect();
    let mut visited = 0usize;
    while let Some(path) = ready.pop() {
        visited += 1;
        for (_, child, _) in edges
            .range((path, "", "")..)
            .take_while(|(parent, _, _)| *parent == path)
        {
            let degree = indegree.get_mut(*child).expect("validated child");
            *degree -= 1;
            if *degree == 0 {
                ready.push(*child);
            }
        }
    }
    if visited != paths.len() {
        return Err("Mantle bundle parent graph contains a cycle".into());
    }
    Ok(())
}

/// Binds the profile to the Mantle revision that actually produced the bundle.
// r[impl chaoscontrol.spacewasm_mvp.admission_refresh]
pub fn validate_producer_revision(
    profile: &DifferentialProfile,
    observed_mantle_revision: &str,
) -> Result<(), String> {
    validate_hex(
        "observed Mantle revision",
        observed_mantle_revision,
        GIT_REVISION_HEX_LENGTH,
    )?;
    if observed_mantle_revision != profile.mantle_revision {
        return Err(format!(
            "Mantle producer revision drift: profile={} observed={observed_mantle_revision}",
            profile.mantle_revision
        ));
    }
    Ok(())
}

/// Admits the exact manifest bytes measured for this profile.
// r[impl chaoscontrol.spacewasm_mvp.admission_refresh]
pub fn validate_manifest_digest(
    profile: &DifferentialProfile,
    observed_manifest_blake3: &str,
) -> Result<(), String> {
    if observed_manifest_blake3 != profile.bundle_manifest_blake3 {
        return Err(format!(
            "bundle manifest digest mismatch: expected={} actual={observed_manifest_blake3}",
            profile.bundle_manifest_blake3
        ));
    }
    Ok(())
}

/// Admits one suite's stable report and receipt from their exact member bytes.
///
/// The bytes must be the manifest-bound members, the report must follow the
/// pinned stable-report grammar with every required test passed, its identity
/// must recompute from its admitted facts, and the receipt must bind the same
/// report and command with a passing status.
// r[impl chaoscontrol.spacewasm_mvp.admission_refresh]
pub fn validate_stable_evidence(
    manifest: &BundleManifest,
    suite: &StableEvidenceSuite,
    receipt_bytes: &[u8],
    stable_report_bytes: &[u8],
) -> Result<MantleStableReport, String> {
    validate_member_bytes(manifest, suite.receipt_path, receipt_bytes)?;
    validate_member_bytes(manifest, suite.stable_report_path, stable_report_bytes)?;
    let report: MantleStableReport =
        serde_json::from_slice(stable_report_bytes).map_err(|error| {
            format!(
                "stable report is outside the pinned grammar at {}: {error}",
                suite.stable_report_path
            )
        })?;
    let receipt: MantleStableReceipt = serde_json::from_slice(receipt_bytes).map_err(|error| {
        format!(
            "stable receipt is outside the pinned grammar at {}: {error}",
            suite.receipt_path
        )
    })?;
    validate_stable_report(suite, &report)?;
    if receipt.status != STABLE_RECEIPT_STATUS_PASSED {
        return Err(format!(
            "stable receipt status is not passed for suite {}: {}",
            suite.suite, receipt.status
        ));
    }
    if receipt.command != report.command || receipt.stable_report != report {
        return Err(format!(
            "stable receipt and stable report disagree for suite {}",
            suite.suite
        ));
    }
    Ok(report)
}

fn validate_stable_report(
    suite: &StableEvidenceSuite,
    report: &MantleStableReport,
) -> Result<(), String> {
    if report.schema != MANTLE_STABLE_REPORT_SCHEMA {
        return Err(format!(
            "unsupported stable report schema: {}",
            report.schema
        ));
    }
    if report.encoding_version != MANTLE_STABLE_REPORT_ENCODING_VERSION {
        return Err(format!(
            "unsupported stable report encoding version: {}",
            report.encoding_version
        ));
    }
    if report.suite != suite.suite {
        return Err(format!(
            "stable report suite drift: expected={} actual={}",
            suite.suite, report.suite
        ));
    }
    if report.command.is_empty() {
        return Err(format!(
            "stable report command is empty for suite {}",
            suite.suite
        ));
    }
    if report.tests.is_empty() || report.tests.len() > MAXIMUM_STABLE_TESTS {
        return Err(format!(
            "stable report test count is outside the admitted bound for suite {}",
            suite.suite
        ));
    }
    let canonical_order = report
        .tests
        .windows(2)
        .all(|pair| pair[0].name < pair[1].name);
    if report.tests.iter().any(|test| test.name.is_empty()) || !canonical_order {
        return Err(format!(
            "stable report tests are not unique, named, and canonically ordered for suite {}",
            suite.suite
        ));
    }
    if let Some(test) = report
        .tests
        .iter()
        .find(|test| test.status != MantleStableTestStatus::Passed)
    {
        return Err(format!(
            "stable report records a non-passing test in suite {}: {} is {:?}",
            suite.suite, test.name, test.status
        ));
    }
    validate_hex(
        "stable report identity",
        &report.stable_identity_blake3,
        BLAKE3_HEX_LENGTH,
    )?;
    let recomputed = stable_report_identity(report)?;
    if recomputed != report.stable_identity_blake3 {
        return Err(format!(
            "stable report identity does not match its admitted facts for suite {}: recorded={} recomputed={recomputed}",
            suite.suite, report.stable_identity_blake3
        ));
    }
    Ok(())
}

fn stable_report_identity(report: &MantleStableReport) -> Result<String, String> {
    let mut input = serde_json::to_vec(&StableReportIdentityInput {
        schema: &report.schema,
        suite: &report.suite,
        command: &report.command,
        tests: &report.tests,
        encoding_version: report.encoding_version,
    })
    .map_err(|error| format!("stable report identity input cannot serialize: {error}"))?;
    input.extend_from_slice(STABLE_IDENTITY_INPUT_TERMINATOR);
    Ok(blake3::hash(&input).to_hex().to_string())
}

fn validate_member_bytes(
    manifest: &BundleManifest,
    path: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let member = find_member(manifest, path).ok_or_else(|| {
        format!("Mantle bundle is missing required stable evidence member: {path}")
    })?;
    let size_matches = u64::try_from(bytes.len()).is_ok_and(|size| size == member.size_bytes);
    if !size_matches || blake3::hash(bytes).to_hex().as_str() != member.digest_blake3 {
        return Err(format!(
            "stable evidence bytes do not match the bundle manifest: {path}"
        ));
    }
    Ok(())
}

fn find_member<'a>(manifest: &'a BundleManifest, path: &str) -> Option<&'a BundleMember> {
    manifest.members.iter().find(|member| member.path == path)
}

fn is_raw_capture_member(member: &BundleMember) -> bool {
    RAW_CAPTURE_ROLES.contains(&member.role.as_str())
        || member.path.starts_with(RAW_CAPTURE_PATH_PREFIX)
        || RAW_CAPTURE_PATH_SUFFIXES
            .iter()
            .any(|suffix| member.path.ends_with(suffix))
}

// r[impl chaoscontrol.spacewasm_mvp.corpus]
pub fn generate_mvp_module(
    seed: u64,
    case_index: usize,
    maximum_instruction_pairs: usize,
) -> Vec<u8> {
    assert!(
        maximum_instruction_pairs > 0,
        "maximum_instruction_pairs must be positive"
    );
    let mut input = Vec::with_capacity(GENERATOR_IDENTITY_INPUT_BYTES);
    input.extend_from_slice(&seed.to_le_bytes());
    input.extend_from_slice(&(case_index as u64).to_le_bytes());
    let digest = blake3::hash(&input);
    let instruction_pair_count = usize::from(digest.as_bytes()[0]) % maximum_instruction_pairs + 1;

    let mut body = vec![WASM_ZERO_LOCAL_DECLARATIONS];
    for pair_index in 0..instruction_pair_count {
        let value = u8::try_from(pair_index % GENERATED_I32_VALUE_CARDINALITY)
            .expect("bounded generated i32 value fits one signed LEB byte");
        body.extend_from_slice(&[WASM_I32_CONST, value, WASM_DROP]);
    }
    body.push(WASM_END);

    let mut module = WASM_MODULE_PREFIX.to_vec();
    module.push(WASM_CODE_SECTION);
    let mut code_payload = vec![WASM_ONE_FUNCTION];
    encode_u32(
        u32::try_from(body.len()).expect("generated body length fits u32"),
        &mut code_payload,
    );
    code_payload.extend_from_slice(&body);
    encode_u32(
        u32::try_from(code_payload.len()).expect("generated code payload length fits u32"),
        &mut module,
    );
    module.extend_from_slice(&code_payload);
    module
}

pub fn malformed_magic(module: &[u8]) -> Vec<u8> {
    let mut malformed = module.to_vec();
    if let Some(first) = malformed.first_mut() {
        *first = INVALID_MAGIC_BYTE;
    }
    malformed
}

fn encode_u32(mut value: u32, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & LEB128_DATA_MASK) as u8;
        value >>= 7;
        if value != 0 {
            byte |= LEB128_CONTINUATION_BIT;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

// r[impl chaoscontrol.spacewasm_mvp.differential]
pub fn normalize_spacewasm(
    fixture_id: &str,
    report: &MantleRunnerReport,
) -> Result<NormalizedObservation, String> {
    if report.schema != MANTLE_RUNNER_REPORT_SCHEMA {
        return Err(format!(
            "unsupported Mantle runner report schema: {}",
            report.schema
        ));
    }
    let result = report
        .results
        .iter()
        .find(|result| result.fixture_id == fixture_id)
        .ok_or_else(|| format!("Mantle runner report is missing fixture: {fixture_id}"))?;
    if result.status != "passed" {
        return Ok(engine_error("spacewasm", &result.result_code));
    }
    let observation = match result.result_code.as_str() {
        "finished" | "stream-finished" => completed("spacewasm"),
        "malformed-magic" | "stream-unexpected-eof" | "unsupported-bulk-memory" => {
            rejected("spacewasm", &result.result_code)
        }
        "trap-unreachable" => trapped("spacewasm", "unreachable"),
        "out-of-fuel" => exhausted("spacewasm", "fuel"),
        code => return Err(format!("unknown SpaceWasm result code: {code}")),
    };
    Ok(observation)
}

pub fn normalize_tool(
    engine: &str,
    case_kind: CaseKind,
    raw: &ToolObservation,
) -> NormalizedObservation {
    if raw.timed_out {
        return NormalizedObservation {
            engine: engine.to_owned(),
            outcome: OutcomeClass::TimedOut,
            return_values: Vec::new(),
            trap_class: None,
            state_identity_blake3: None,
            resource_class: Some(String::from("wall-time")),
            surfaces: hostless_surfaces("unknown", "unknown"),
            raw_counters: std::collections::BTreeMap::new(),
        };
    }
    if raw.success {
        return completed(engine);
    }
    let stderr = raw.stderr.to_ascii_lowercase();
    if stderr.contains("fuel") {
        return exhausted(engine, "fuel");
    }
    if stderr.contains("unreachable") {
        return trapped(engine, "unreachable");
    }
    if case_kind == CaseKind::Reject {
        return rejected(engine, "module-admission");
    }
    engine_error(engine, "nonzero-exit")
}

// r[impl chaoscontrol.spacewasm_mvp.differential]
// r[impl chaoscontrol.spacewasm_mvp.functional_core]
pub fn compare_case(
    case_id: String,
    case_kind: CaseKind,
    module_blake3: String,
    spacewasm: NormalizedObservation,
    wasmtime: NormalizedObservation,
) -> Result<CaseComparison, String> {
    let harness = compare_through_harness(&module_blake3, case_kind, &spacewasm, &wasmtime)?;
    let first_difference = match harness.verdict {
        differential_execution_core::ComparisonVerdict::Agreement => None,
        _ => first_difference(&spacewasm, &wasmtime),
    };
    let verdict = if first_difference.is_none() {
        ComparisonVerdict::Match
    } else {
        ComparisonVerdict::Mismatch
    };
    Ok(CaseComparison {
        case_id,
        case_kind,
        module_blake3,
        spacewasm,
        wasmtime,
        verdict,
        first_difference,
        minimization: None,
        raw_observation_refs: Vec::new(),
    })
}

// r[impl chaoscontrol.spacewasm_mvp.shrink]
pub fn same_mismatch_predicate(left: &CaseComparison, right: &CaseComparison) -> bool {
    match (&left.first_difference, &right.first_difference) {
        (Some(left), Some(right)) => {
            left.field == right.field
                && left.spacewasm == right.spacewasm
                && left.wasmtime == right.wasmtime
        }
        _ => false,
    }
}

// r[impl chaoscontrol.spacewasm_mvp.shrink]
pub fn generated_shrink_candidates(
    seed: u64,
    case_index: usize,
    maximum_instruction_pairs: usize,
) -> Vec<Vec<u8>> {
    assert!(
        maximum_instruction_pairs > 0,
        "maximum_instruction_pairs must be positive"
    );
    let original = generate_mvp_module(seed, case_index, maximum_instruction_pairs);
    let mut unique = BTreeSet::new();
    for candidate_bound in 1..maximum_instruction_pairs {
        let candidate = generate_mvp_module(seed, case_index, candidate_bound);
        if candidate.len() < original.len() {
            unique.insert(candidate);
        }
    }
    let mut candidates = unique.into_iter().collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)));
    candidates
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; HEX_ALPHABET_LENGTH] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(HEX_CHARACTERS_PER_BYTE));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> HIGH_NIBBLE_SHIFT)]));
        output.push(char::from(HEX[usize::from(byte & NIBBLE_MASK)]));
    }
    output
}

// r[impl chaoscontrol.spacewasm_mvp.evidence]
pub fn finalize_report(mut report: DifferentialReport) -> Result<DifferentialReport, String> {
    if report.schema != REPORT_SCHEMA {
        return Err("report schema drift".into());
    }
    validate_exact_set(
        "report non_claims",
        &report.non_claims,
        &REQUIRED_NON_CLAIMS,
    )?;
    report.evidence.clear();
    for comparison in &report.comparisons {
        if comparison.raw_observation_refs.len() != 2 {
            return Err("report must retain both actual raw runtime observations".into());
        }
        let record = rail::DiagnosticEvidence {
            class: if [&comparison.spacewasm, &comparison.wasmtime]
                .iter()
                .any(|observation| observation.outcome == OutcomeClass::EngineError)
            {
                rail::EvidenceClass::HarnessError
            } else if [&comparison.spacewasm, &comparison.wasmtime]
                .iter()
                .any(|observation| {
                    matches!(
                        observation.outcome,
                        OutcomeClass::TimedOut | OutcomeClass::ResourceExhausted
                    )
                })
            {
                rail::EvidenceClass::BoundSkip
            } else if comparison.verdict == ComparisonVerdict::Match {
                rail::EvidenceClass::RuntimeMatch
            } else {
                rail::EvidenceClass::RuntimeDivergence
            },
            profile_identity: report.profile_identity_blake3.clone(),
            case_blake3: comparison.module_blake3.clone(),
            raw_observation_refs: comparison.raw_observation_refs.clone(),
            role: "host-side-diagnostic".into(),
            non_claims: report.non_claims.clone(),
        };
        record.validate()?;
        report.evidence.push(record);
    }
    for comparison in &report.comparisons {
        let recomputed = compare_case(
            comparison.case_id.clone(),
            comparison.case_kind,
            comparison.module_blake3.clone(),
            comparison.spacewasm.clone(),
            comparison.wasmtime.clone(),
        )?;
        if recomputed.verdict != comparison.verdict
            || recomputed.first_difference != comparison.first_difference
        {
            return Err("report comparison verdict drift".into());
        }
    }
    report.mismatch_count = report
        .comparisons
        .iter()
        .filter(|comparison| comparison.verdict == ComparisonVerdict::Mismatch)
        .count();
    report.verdict = if report
        .evidence
        .iter()
        .any(|record| record.class == rail::EvidenceClass::HarnessError)
    {
        String::from("harness-error")
    } else if report.mismatch_count == 0 {
        String::from("match")
    } else {
        String::from("mismatch")
    };
    report.report_identity_blake3.clear();
    let bytes = serde_json::to_vec(&report)
        .map_err(|error| format!("serialize report identity: {error}"))?;
    report.report_identity_blake3 = blake3::hash(&bytes).to_hex().to_string();
    Ok(report)
}

pub fn profile_identity(profile: &DifferentialProfile) -> Result<String, String> {
    let bytes = serde_json::to_vec(profile)
        .map_err(|error| format!("serialize profile identity: {error}"))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

// --- differential-execution-core adapter --------------------------------
//
// The SpaceWasm rail routes its per-case verdict through the reviewed
// `differential-execution-core` boundary: case + observation admission with
// explicit identities and bounds, independent-backend admission, pairwise
// comparison, and optional oracle classification. The crate keeps its own
// `NormalizedObservation`/`CaseComparison` types as the shell-facing report
// projection; this adapter is the only DEH-touching surface.

const DIFF_DOMAIN: &[u8] = b"chaoscontrol.spacewasm.differential.v1";
const DIFF_VALUE_SCHEMA: &[u8] = b"spacewasm-normalized-outcome-v1";
const DIFF_ADAPTER: &[u8] = b"chaoscontrol-wasm-differential-adapter-v1";
const DIFF_BACKEND_COHORT: &[u8] = b"wasm-engines";
const DIFF_RUNTIME_COHORT: &[u8] = b"spacewasm-differential-v1";
const DIFF_RESOURCES: &[u8] = b"bounded-host";
const DIFF_EFFECTS: &[u8] = b"wasm-core-exec";
const DIFF_NORMALIZATION: &[u8] = b"spacewasm-normalization-v1";
const DIFF_NO_FINAL_STATE: &[u8] = b"no-final-state";
const DIFF_MAX_EVENTS: u32 = 64;
const DIFF_MAX_VALUE_BYTES: u32 = 1_048_576;
const DIFF_MAX_SET_METADATA: u32 = 16;

/// One serializable canonical value frame for the harness value surface.
#[derive(serde::Serialize)]
struct CanonicalOutcome {
    outcome: String,
    return_values: Vec<String>,
    trap_class: Option<String>,
    resource_class: Option<String>,
    surfaces: ObservationSurfaces,
}

/// Admits one complete differential case from the module identity.
pub fn admit_diff_case(
    module_blake3: &str,
    case_kind: CaseKind,
) -> Result<differential_execution_core::ExecutionCase, String> {
    let module = module_blake3.as_bytes();
    let kind = match case_kind {
        CaseKind::Execute => b"execute".as_slice(),
        CaseKind::Reject => b"reject".as_slice(),
    };
    let identity = |label: &[u8], parts: &[&[u8]]| {
        differential_execution_core::Identity::derive(DIFF_DOMAIN, parts)
            .map_err(|error| format!("derive {label:?}: {error:?}"))
    };
    let draft = differential_execution_core::CaseDraft {
        program: Some(identity(b"program", &[module])?),
        arguments: Some(identity(b"arguments", &[module, kind])?),
        initial_state: Some(identity(b"initial-state", &[module])?),
        effects: Some(identity(b"effects", &[DIFF_EFFECTS])?),
        backend_cohort: Some(identity(b"backend-cohort", &[DIFF_BACKEND_COHORT])?),
        runtime_cohort: Some(identity(b"runtime-cohort", &[DIFF_RUNTIME_COHORT])?),
        resources: Some(identity(b"resources", &[DIFF_RESOURCES])?),
        adapter_contract: Some(identity(b"adapter-contract", &[DIFF_ADAPTER])?),
        normalization: Some(identity(b"normalization", &[DIFF_NORMALIZATION])?),
    };
    differential_execution_core::admit_case(draft).map_err(|error| format!("admit case: {error:?}"))
}

/// Admits one engine observation for an already-admitted case.
pub fn admit_diff_observation(
    case: &differential_execution_core::ExecutionCase,
    engine: &str,
    observation: &NormalizedObservation,
) -> Result<differential_execution_core::AdmittedObservation, String> {
    let identity = |label: &[u8], parts: &[&[u8]]| {
        differential_execution_core::Identity::derive(DIFF_DOMAIN, parts)
            .map_err(|error| format!("derive {label:?}: {error:?}"))
    };
    let canonical = CanonicalOutcome {
        outcome: format!("{:?}", observation.outcome),
        return_values: observation.return_values.clone(),
        trap_class: observation.trap_class.clone(),
        resource_class: observation.resource_class.clone(),
        surfaces: observation.surfaces.clone(),
    };
    let value_bytes = serde_json::to_vec(&canonical)
        .map_err(|error| format!("serialize canonical outcome: {error}"))?;
    let final_state = match &observation.state_identity_blake3 {
        Some(state) => identity(b"final-state", &[state.as_bytes()])?,
        None => identity(b"final-state", &[DIFF_NO_FINAL_STATE])?,
    };
    let resource = match &observation.resource_class {
        Some(class) => differential_execution_core::ResourceOutcome {
            class: identity(b"resource-class", &[class.as_bytes()])?,
            detail: identity(b"resource-detail", &[b"bounded-host"])?,
        },
        None => differential_execution_core::ResourceOutcome {
            class: identity(b"resource-class", &[b"none"])?,
            detail: identity(b"resource-detail", &[b"bounded-host"])?,
        },
    };
    let profiles = differential_execution_core::ObservationProfiles::from(case.profiles());
    let raw = differential_execution_core::NormalizedObservation {
        case: case.identity(),
        profiles,
        backend_implementation: identity(b"backend-implementation", &[engine.as_bytes()])?,
        adapter_implementation: identity(b"adapter-implementation", &[DIFF_ADAPTER])?,
        value: differential_execution_core::CanonicalValue {
            schema: identity(b"value-schema", &[DIFF_VALUE_SCHEMA])?,
            bytes: value_bytes,
        },
        events: Vec::new(),
        trap: match &observation.trap_class {
            Some(class) => differential_execution_core::OptionalOutcome::Present(identity(
                b"trap",
                &[class.as_bytes()],
            )?),
            None => differential_execution_core::OptionalOutcome::Absent,
        },
        interrupt: differential_execution_core::OptionalOutcome::Absent,
        resource,
        final_state,
        set_metadata: Vec::new(),
    };
    let bounds = differential_execution_core::ObservationBounds::new(
        DIFF_MAX_EVENTS,
        DIFF_MAX_VALUE_BYTES,
        DIFF_MAX_SET_METADATA,
    )
    .map_err(|error| format!("observation bounds rejected: {error:?}"))?;
    differential_execution_core::admit_observation(case, raw, bounds)
        .map_err(|error| format!("admit observation: {error:?}"))
}

/// One reviewed harness verdict for a case; the crate report projection stays
/// the shell-facing surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessVerdict {
    /// Lowercase harness case identity.
    pub case_identity_hex: String,
    /// Pairwise agreement or divergence from the reviewed core.
    pub verdict: differential_execution_core::ComparisonVerdict,
    /// First bounded mismatch surface, when divergent.
    pub mismatch: Option<differential_execution_core::MismatchPath>,
}

/// Admits both engine observations and returns the reviewed pairwise verdict.
pub fn compare_through_harness(
    module_blake3: &str,
    case_kind: CaseKind,
    spacewasm: &NormalizedObservation,
    wasmtime: &NormalizedObservation,
) -> Result<HarnessVerdict, String> {
    let case = admit_diff_case(module_blake3, case_kind)?;
    let left = admit_diff_observation(&case, "spacewasm", spacewasm)?;
    let right = admit_diff_observation(&case, "wasmtime", wasmtime)?;
    differential_execution_core::admit_independent_backends(Some(&left), Some(&right))
        .map_err(|error| format!("independence admission failed: {error:?}"))?;
    let report = differential_execution_core::compare_pairwise(&left, &right);
    Ok(HarnessVerdict {
        case_identity_hex: case.identity().to_hex(),
        verdict: report.verdict,
        mismatch: report.mismatch,
    })
}

/// Classifies both engine observations against an exact external fixture.
pub fn classify_through_harness(
    module_blake3: &str,
    case_kind: CaseKind,
    expected: &NormalizedObservation,
    left: &NormalizedObservation,
    right: &NormalizedObservation,
) -> Result<differential_execution_core::OracleReport, String> {
    let case = admit_diff_case(module_blake3, case_kind)?;
    let expected_obs = admit_diff_observation(&case, "oracle", expected)?;
    let left_obs = admit_diff_observation(&case, "spacewasm", left)?;
    let right_obs = admit_diff_observation(&case, "wasmtime", right)?;
    let fixture = differential_execution_core::OracleFixture {
        case: case.identity(),
        normalization: case.profiles().normalization,
        expected: expected_obs,
    };
    differential_execution_core::classify_with_oracle(&left_obs, &right_obs, &fixture)
        .map_err(|error| format!("oracle classification rejected: {error:?}"))
}

fn first_difference(
    spacewasm: &NormalizedObservation,
    wasmtime: &NormalizedObservation,
) -> Option<FirstDifference> {
    if spacewasm.outcome != wasmtime.outcome {
        return Some(FirstDifference {
            field: String::from("outcome"),
            spacewasm: format!("{:?}", spacewasm.outcome),
            wasmtime: format!("{:?}", wasmtime.outcome),
        });
    }
    if spacewasm.return_values != wasmtime.return_values {
        return Some(FirstDifference {
            field: String::from("return_values"),
            spacewasm: format!("{:?}", spacewasm.return_values),
            wasmtime: format!("{:?}", wasmtime.return_values),
        });
    }
    if spacewasm.trap_class != wasmtime.trap_class {
        return Some(FirstDifference {
            field: String::from("trap_class"),
            spacewasm: format!("{:?}", spacewasm.trap_class),
            wasmtime: format!("{:?}", wasmtime.trap_class),
        });
    }
    if spacewasm.state_identity_blake3 != wasmtime.state_identity_blake3 {
        return Some(FirstDifference {
            field: String::from("state_identity_blake3"),
            spacewasm: format!("{:?}", spacewasm.state_identity_blake3),
            wasmtime: format!("{:?}", wasmtime.state_identity_blake3),
        });
    }
    if spacewasm.resource_class != wasmtime.resource_class {
        return Some(FirstDifference {
            field: String::from("resource_class"),
            spacewasm: format!("{:?}", spacewasm.resource_class),
            wasmtime: format!("{:?}", wasmtime.resource_class),
        });
    }
    if spacewasm.surfaces != wasmtime.surfaces {
        return Some(FirstDifference {
            field: "observation-surfaces".into(),
            spacewasm: format!("{:?}", spacewasm.surfaces),
            wasmtime: format!("{:?}", wasmtime.surfaces),
        });
    }
    None
}

fn completed(engine: &str) -> NormalizedObservation {
    NormalizedObservation {
        engine: engine.to_owned(),
        outcome: OutcomeClass::Completed,
        return_values: Vec::new(),
        trap_class: None,
        state_identity_blake3: None,
        resource_class: None,
        surfaces: hostless_surfaces("accepted", "linked"),
        raw_counters: std::collections::BTreeMap::new(),
    }
}

fn rejected(engine: &str, _engine_class: &str) -> NormalizedObservation {
    NormalizedObservation {
        engine: engine.to_owned(),
        outcome: OutcomeClass::Rejected,
        return_values: Vec::new(),
        trap_class: None,
        state_identity_blake3: None,
        resource_class: None,
        surfaces: hostless_surfaces("rejected", "not-reached"),
        raw_counters: std::collections::BTreeMap::new(),
    }
}

fn trapped(engine: &str, class: &str) -> NormalizedObservation {
    NormalizedObservation {
        engine: engine.to_owned(),
        outcome: OutcomeClass::Trapped,
        return_values: Vec::new(),
        trap_class: Some(class.to_owned()),
        state_identity_blake3: None,
        resource_class: None,
        surfaces: hostless_surfaces("accepted", "linked"),
        raw_counters: std::collections::BTreeMap::new(),
    }
}

fn exhausted(engine: &str, class: &str) -> NormalizedObservation {
    NormalizedObservation {
        engine: engine.to_owned(),
        outcome: OutcomeClass::ResourceExhausted,
        return_values: Vec::new(),
        trap_class: None,
        state_identity_blake3: None,
        resource_class: Some(class.to_owned()),
        surfaces: hostless_surfaces("accepted", "linked"),
        raw_counters: std::collections::BTreeMap::new(),
    }
}

fn engine_error(engine: &str, class: &str) -> NormalizedObservation {
    NormalizedObservation {
        engine: engine.to_owned(),
        outcome: OutcomeClass::EngineError,
        return_values: Vec::new(),
        trap_class: Some(class.to_owned()),
        state_identity_blake3: None,
        resource_class: None,
        surfaces: hostless_surfaces("unknown", "unknown"),
        raw_counters: std::collections::BTreeMap::new(),
    }
}

fn validate_identifier(field: &str, value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= MAXIMUM_IDENTIFIER_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        });
    if valid {
        Ok(())
    } else {
        Err(format!("{field} is not a bounded identifier"))
    }
}

fn validate_hex(field: &str, value: &str, expected_len: usize) -> Result<(), String> {
    if value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(format!(
            "{field} must be {expected_len} lowercase hexadecimal characters"
        ))
    }
}

fn validate_relative_path(value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && !value.starts_with('/')
        && !value
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "bundle member path is not a safe relative path: {value}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const MANTLE_REVISION: &str = "57fa0f5c2f8fb1edeed6d6113b57d32357612e34";
    const STALE_MANTLE_REVISION: &str = "a141fcbaafe41f9a413a81275a33fe915bfca370";
    const SPACEWASM_REVISION: &str = "e24cf09355a90497148eb5029fdb8e3400bd63e3";
    const MANIFEST_DIGEST: &str =
        "39e4790a7b9d0b14fcafffe5810e268cd8af342d38d7e952a6ede923e33882b2";
    const BUNDLE_IDENTITY: &str =
        "c4826bb63fa9eef1fa619e0f0c4c2c35dd10ca92a8d4999fec10c55e92b692b7";
    const STALE_BUNDLE_IDENTITY: &str =
        "cee7190f2f78321b07f3d1f493baaa5b2cb74d517eb4f229c7e7a6094b877342";
    const RUNNER_DIGEST: &str = "732da9b7276cb2c36dc0e5e308e8fd8e6bc88ddd665346c5afb1f351eda2a579";
    const STALE_RUNNER_DIGEST: &str =
        "be8aeb698afdecf6fb608910980292517ed952f122b6447705d4bdae485b0221";
    const STALE_MANIFEST_DIGEST: &str =
        "13058ea2d9913348a203cceff7b58d98b6446610ac80518dc3359b8d7ee57472";
    /// Exact `reports/checks/upstream-spectest-address*.json` members of the
    /// Mantle 57fa0f5c2 bundle measured for this consumer.
    const PINNED_SPECTEST_RECEIPT: &[u8] = include_bytes!(
        "../../../contracts/evidence/fixtures/valid/spacewasm-stable-receipt.upstream-spectest-address.valid.json"
    );
    const PINNED_SPECTEST_STABLE_REPORT: &[u8] = include_bytes!(
        "../../../contracts/evidence/fixtures/valid/spacewasm-stable-report.upstream-spectest-address.valid.json"
    );
    const PINNED_SPECTEST_STABLE_IDENTITY: &str =
        "46e88a64a9a2aca1e6eeca915a386c433fa97876e17b29fea81b1df2efad8097";
    const SYNTHETIC_UNIT_TEST_COMMAND: &str =
        "RUSTC_BOOTSTRAP=1 cargo test --locked --offline --no-default-features --lib -- --format json -Z unstable-options ";
    const UNIT_SUITE: StableEvidenceSuite = STABLE_EVIDENCE_SUITES[0];
    const SPECTEST_SUITE: StableEvidenceSuite = STABLE_EVIDENCE_SUITES[1];
    const MAX_INSTRUCTION_PAIRS: usize = 32;
    const MAX_GENERATED_MODULE_BYTES: usize = 256;
    const PROPERTY_CASE_INDEX_BOUND: usize = 1_000;

    pub(super) fn profile() -> DifferentialProfile {
        DifferentialProfile {
            schema: String::from(PROFILE_SCHEMA),
            profile_id: String::from("spacewasm-mvp-differential-v1"),
            mantle_revision: String::from(MANTLE_REVISION),
            mantle_profile_id: String::from("nasa-spacewasm-e24cf093-reference-v1"),
            spacewasm_revision: String::from(SPACEWASM_REVISION),
            spacewasm_runner_blake3: String::from(RUNNER_DIGEST),
            wasmtime_version: String::from("wasmtime 41.0.3"),
            wasm_tools_version: String::from("wasm-tools 1.245.0"),
            retention: String::from("all-cases-and-raw-observations"),
            bundle_manifest_blake3: String::from(MANIFEST_DIGEST),
            bundle_identity_blake3: String::from(BUNDLE_IDENTITY),
            generator_id: String::from("chaoscontrol-mvp-instruction-v1"),
            seed: 7,
            generated_valid_cases: 4,
            generated_malformed_cases: 4,
            feature_intersection: REQUIRED_FEATURES.into_iter().map(String::from).collect(),
            module_abi: ModuleAbiProfile {
                required_export: String::from(REQUIRED_EXPORT),
                parameter_count: 0,
                result_count: 0,
                maximum_imports: 0,
                maximum_memories: 0,
                maximum_tables: 0,
                maximum_host_functions: 0,
                maximum_linear_memory_bytes: 0,
                memory_grow: String::from(REQUIRED_MEMORY_GROW_POLICY),
            },
            runtime: RuntimeProfile {
                spacewasm_resume_segment_fuel: 1,
                maximum_resume_segments: 4_096,
                spacewasm_code_pages: 64,
                spacewasm_control_frames: 64,
                spacewasm_stack_words: 1024,
                spacewasm_initialization_fuel: 1024,
                spacewasm_run_fuel: 4096,
                wasmtime_proposals: String::from(REQUIRED_WASMTIME_PROPOSALS),
                host_authority: String::from(REQUIRED_HOST_AUTHORITY),
            },
            chunk_schedules: REQUIRED_CHUNK_SCHEDULES
                .into_iter()
                .map(String::from)
                .collect(),
            corpus_classes: REQUIRED_CORPUS_CLASSES
                .into_iter()
                .map(String::from)
                .collect(),
            observation_fields: REQUIRED_OBSERVATION_FIELDS
                .into_iter()
                .map(String::from)
                .collect(),
            bounds: DifferentialBounds {
                maximum_cases: 16,
                maximum_input_bytes: 1_048_576,
                maximum_bundle_members: 512,
                maximum_bundle_member_bytes: 805_306_368,
                maximum_bundle_total_bytes: 2_147_483_648,
                maximum_process_milliseconds: 30_000,
                maximum_output_bytes: 1_048_576,
                wasmtime_fuel: 1_024,
                maximum_generated_instruction_pairs: MAX_INSTRUCTION_PAIRS,
                maximum_shrink_attempts: MAX_INSTRUCTION_PAIRS,
                maximum_functions: 1024,
                maximum_types: 1024,
                maximum_exports: 1024,
                maximum_globals: 64,
                maximum_interpreter_allocation_bytes: 16777216,
                maximum_hostcall_bytes: 1048576,
                maximum_chunks: 4096,
                maximum_concurrent_cases: 1,
                maximum_samples: 1024,
                maximum_retained_artifacts: 4096,
            },
            non_claims: REQUIRED_NON_CLAIMS.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn interpreter_allocation_limit_must_fit_the_executed_probe() {
        let mut profile = profile();
        profile.bounds.maximum_interpreter_allocation_bytes = MAXIMUM_INTERPRETER_ALLOCATION_BYTES;
        assert!(validate_profile(&profile).is_ok());
        profile.bounds.maximum_interpreter_allocation_bytes += 1;
        assert!(validate_profile(&profile).is_err());
    }

    // r[verify chaoscontrol.spacewasm_mvp.profile]
    // r[verify chaoscontrol.spacewasm_mvp.corpus]
    // r[verify chaoscontrol.spacewasm_mvp.bounds]
    // r[verify chaoscontrol.spacewasm_mvp.validation]
    #[test]
    fn valid_profile_and_generated_modules_are_deterministic() {
        let profile = profile();
        assert_eq!(validate_profile(&profile), Ok(()));
        let first = generate_mvp_module(
            profile.seed,
            2,
            profile.bounds.maximum_generated_instruction_pairs,
        );
        let second = generate_mvp_module(
            profile.seed,
            2,
            profile.bounds.maximum_generated_instruction_pairs,
        );
        assert_eq!(first, second);
        assert_ne!(
            first,
            generate_mvp_module(
                profile.seed,
                3,
                profile.bounds.maximum_generated_instruction_pairs,
            )
        );
        assert_eq!(&first[..8], b"\0asm\x01\0\0\0");
    }

    // r[verify chaoscontrol.spacewasm_mvp.shrink]
    #[test]
    fn generated_shrink_plan_is_deterministic_and_strictly_smaller() {
        let original = generate_mvp_module(0, 0, MAX_INSTRUCTION_PAIRS);
        let first = generated_shrink_candidates(0, 0, MAX_INSTRUCTION_PAIRS);
        let second = generated_shrink_candidates(0, 0, MAX_INSTRUCTION_PAIRS);
        assert_eq!(first, second);
        assert!(!first.is_empty());
        assert!(first
            .iter()
            .all(|candidate| candidate.len() < original.len()));
        assert_eq!(
            encode_hex(&[WASM_ZERO_LOCAL_DECLARATIONS, INVALID_MAGIC_BYTE]),
            "00ff"
        );
    }

    // r[verify chaoscontrol.spacewasm_mvp.differential]
    // r[verify chaoscontrol.spacewasm_mvp.execution]
    // r[verify chaoscontrol.spacewasm_mvp.validation]
    #[test]
    fn invalid_profile_and_mismatch_fail_closed_with_first_difference() {
        let mut invalid = profile();
        invalid
            .feature_intersection
            .push(String::from("component-model"));
        assert!(validate_profile(&invalid)
            .unwrap_err()
            .contains("feature_intersection"));

        let mut ambient_authority = profile();
        ambient_authority.runtime.host_authority = String::from("filesystem");
        assert!(validate_profile(&ambient_authority)
            .unwrap_err()
            .contains("host authority"));

        let comparison = compare_case(
            String::from("negative-divergence"),
            CaseKind::Execute,
            String::from(MANIFEST_DIGEST),
            completed("spacewasm"),
            trapped("wasmtime", "unreachable"),
        )
        .expect("harness comparison");
        assert_eq!(comparison.verdict, ComparisonVerdict::Mismatch);
        let same_predicate = compare_case(
            String::from("same-predicate"),
            CaseKind::Execute,
            String::from(MANIFEST_DIGEST),
            completed("spacewasm"),
            trapped("wasmtime", "unreachable"),
        )
        .expect("harness comparison");
        assert!(same_mismatch_predicate(&comparison, &same_predicate));
        assert_eq!(
            comparison.first_difference.expect("difference").field,
            "outcome"
        );
    }

    #[test]
    fn normalization_rejects_unknown_spacewasm_result_codes() {
        let report = MantleRunnerReport {
            results: vec![MantleFixtureResult {
                fixture_id: String::from("fixture"),
                result_code: String::from("future-result"),
                status: String::from("passed"),
            }],
            schema: String::from(MANTLE_RUNNER_REPORT_SCHEMA),
            source_revision: String::from(SPACEWASM_REVISION),
        };
        assert!(normalize_spacewasm("fixture", &report)
            .unwrap_err()
            .contains("unknown"));
    }

    /// Stable evidence members keyed by bundle path.
    type StableEvidence = Vec<(&'static str, Vec<u8>)>;
    /// One targeted drift applied to an otherwise admitted stable report.
    type ReportMutation = fn(&mut MantleStableReport);

    fn synthetic_unit_evidence(mutate: impl FnOnce(&mut MantleStableReport)) -> (Vec<u8>, Vec<u8>) {
        let mut report = MantleStableReport {
            schema: String::from(MANTLE_STABLE_REPORT_SCHEMA),
            suite: String::from(UNIT_SUITE.suite),
            command: String::from(SYNTHETIC_UNIT_TEST_COMMAND),
            tests: ["tests::alpha", "tests::bravo"]
                .into_iter()
                .map(|name| MantleStableTest {
                    name: String::from(name),
                    status: MantleStableTestStatus::Passed,
                })
                .collect(),
            encoding_version: MANTLE_STABLE_REPORT_ENCODING_VERSION,
            stable_identity_blake3: String::new(),
        };
        mutate(&mut report);
        report.stable_identity_blake3 = stable_report_identity(&report).expect("identity");
        let receipt = MantleStableReceipt {
            command: report.command.clone(),
            status: String::from(STABLE_RECEIPT_STATUS_PASSED),
            stable_report: report.clone(),
        };
        (
            serde_json::to_vec_pretty(&receipt).expect("receipt bytes"),
            serde_json::to_vec_pretty(&report).expect("report bytes"),
        )
    }

    fn stable_evidence_with_unit(unit: (Vec<u8>, Vec<u8>)) -> StableEvidence {
        vec![
            (UNIT_SUITE.receipt_path, unit.0),
            (UNIT_SUITE.stable_report_path, unit.1),
            (
                SPECTEST_SUITE.receipt_path,
                PINNED_SPECTEST_RECEIPT.to_vec(),
            ),
            (
                SPECTEST_SUITE.stable_report_path,
                PINNED_SPECTEST_STABLE_REPORT.to_vec(),
            ),
        ]
    }

    fn stable_evidence() -> StableEvidence {
        stable_evidence_with_unit(synthetic_unit_evidence(|_| {}))
    }

    fn evidence_bytes<'a>(evidence: &'a StableEvidence, path: &str) -> &'a [u8] {
        evidence
            .iter()
            .find(|(member_path, _)| *member_path == path)
            .map(|(_, bytes)| bytes.as_slice())
            .expect("stable evidence member")
    }

    fn replace_evidence(evidence: &mut StableEvidence, path: &str, bytes: Vec<u8>) {
        let entry = evidence
            .iter_mut()
            .find(|(member_path, _)| *member_path == path)
            .expect("stable evidence member");
        entry.1 = bytes;
    }

    fn member(path: &str, role: &str, digest: &str) -> BundleMember {
        BundleMember {
            path: String::from(path),
            role: String::from(role),
            digest_blake3: String::from(digest),
            size_bytes: 1,
        }
    }

    fn manifest(evidence: &StableEvidence) -> BundleManifest {
        let mut members = vec![
            member(
                "binaries/host/mantle-spacewasm-diagnostic-runner",
                "host-runner",
                RUNNER_DIGEST,
            ),
            member(
                "fixtures/wasm/mvp-positive.wasm",
                "fixture-artifact",
                MANIFEST_DIGEST,
            ),
            member("profile/profile.json", "profile-export", MANIFEST_DIGEST),
            member("source/spacewasm.tar.gz", "source-archive", MANIFEST_DIGEST),
            member("licenses/LICENSE", "license", MANIFEST_DIGEST),
        ];
        members.extend(evidence.iter().map(|(path, bytes)| BundleMember {
            path: String::from(*path),
            role: String::from(STABLE_EVIDENCE_ROLE),
            digest_blake3: blake3::hash(bytes).to_hex().to_string(),
            size_bytes: u64::try_from(bytes.len()).expect("fixture size"),
        }));
        // Synthetic graph exercises consumer graph admission, not producer provenance.
        let parent_edges = members
            .iter()
            .filter(|member| member.path != "source/spacewasm.tar.gz")
            .map(|member| ParentEdge {
                parent_path: "source/spacewasm.tar.gz".into(),
                child_path: member.path.clone(),
                relation: "derived-from".into(),
            })
            .collect();
        BundleManifest {
            schema: String::from(MANTLE_MANIFEST_SCHEMA),
            profile_id: String::from("nasa-spacewasm-e24cf093-reference-v1"),
            profile_identity_blake3: String::from(MANIFEST_DIGEST),
            cohort_identity_blake3: String::from(MANIFEST_DIGEST),
            members,
            parent_edges,
            non_claims: Vec::new(),
            bundle_identity_blake3: String::from(BUNDLE_IDENTITY),
        }
    }

    fn admit_stable_evidence(
        manifest: &BundleManifest,
        evidence: &StableEvidence,
    ) -> Result<Vec<MantleStableReport>, String> {
        STABLE_EVIDENCE_SUITES
            .iter()
            .map(|suite| {
                validate_stable_evidence(
                    manifest,
                    suite,
                    evidence_bytes(evidence, suite.receipt_path),
                    evidence_bytes(evidence, suite.stable_report_path),
                )
            })
            .collect()
    }

    // r[verify chaoscontrol.spacewasm_mvp.materialization]
    // r[verify chaoscontrol.spacewasm_mvp.validation]
    #[test]
    fn materialization_rejects_missing_license_and_invalid_parent_graphs() {
        let profile = profile();
        let admitted = manifest(&stable_evidence());
        let mut no_license = admitted.clone();
        no_license.members.retain(|member| member.role != "license");
        no_license
            .parent_edges
            .retain(|edge| edge.child_path != "licenses/LICENSE");
        assert!(validate_bundle_manifest(&profile, &no_license)
            .unwrap_err()
            .contains("license"));
        let mut dangling = admitted.clone();
        dangling.parent_edges[0].parent_path = "absent/source".into();
        assert!(validate_bundle_manifest(&profile, &dangling)
            .unwrap_err()
            .contains("dangling"));
        let mut omitted = admitted.clone();
        omitted.parent_edges.clear();
        assert!(validate_bundle_manifest(&profile, &omitted)
            .unwrap_err()
            .contains("omits"));
        let mut cycle = admitted.clone();
        cycle.parent_edges.push(ParentEdge {
            parent_path: admitted.parent_edges[0].child_path.clone(),
            child_path: "source/spacewasm.tar.gz".into(),
            relation: "cycle".into(),
        });
        assert!(validate_bundle_manifest(&profile, &cycle)
            .unwrap_err()
            .contains("cycle"));
        let mut duplicate = admitted;
        duplicate
            .parent_edges
            .push(duplicate.parent_edges[0].clone());
        assert!(validate_bundle_manifest(&profile, &duplicate)
            .unwrap_err()
            .contains("duplicated"));
    }

    // r[verify chaoscontrol.spacewasm_mvp.evidence]
    // r[verify chaoscontrol.spacewasm_mvp.validation]
    #[test]
    fn bundle_drift_and_evidence_overclaim_are_rejected() {
        let profile = profile();
        let manifest = manifest(&stable_evidence());
        assert_eq!(validate_bundle_manifest(&profile, &manifest), Ok(()));

        let mut stale_profile = profile.clone();
        stale_profile.bundle_identity_blake3 = String::from(STALE_BUNDLE_IDENTITY);
        assert!(validate_bundle_manifest(&stale_profile, &manifest)
            .unwrap_err()
            .contains("identity"));

        let mut stale = manifest;
        stale.bundle_identity_blake3 = String::from(MANIFEST_DIGEST);
        assert!(validate_bundle_manifest(&profile, &stale)
            .unwrap_err()
            .contains("identity"));

        let mut overclaim = profile;
        overclaim
            .non_claims
            .retain(|claim| claim != "not-spacewasm-wasmtime-equivalence");
        assert!(validate_profile(&overclaim)
            .unwrap_err()
            .contains("non-claim"));
    }

    // r[verify chaoscontrol.spacewasm_mvp.admission_refresh]
    #[test]
    fn pinned_producer_stable_evidence_is_admitted() {
        let profile = profile();
        let evidence = stable_evidence();
        let manifest = manifest(&evidence);
        assert_eq!(
            validate_producer_revision(&profile, MANTLE_REVISION),
            Ok(())
        );
        assert_eq!(validate_manifest_digest(&profile, MANIFEST_DIGEST), Ok(()));
        assert_eq!(validate_bundle_manifest(&profile, &manifest), Ok(()));
        let reports = admit_stable_evidence(&manifest, &evidence).expect("stable evidence");
        let spectest = &reports[1];
        assert_eq!(spectest.suite, SPECTEST_SUITE.suite);
        assert_eq!(
            spectest.stable_identity_blake3,
            PINNED_SPECTEST_STABLE_IDENTITY
        );
        assert_eq!(
            stable_report_identity(spectest).expect("identity"),
            PINNED_SPECTEST_STABLE_IDENTITY
        );
        assert!(spectest
            .tests
            .iter()
            .all(|test| test.status == MantleStableTestStatus::Passed));
    }

    // r[verify chaoscontrol.spacewasm_mvp.admission_refresh]
    #[test]
    fn stale_producer_pin_and_profile_digests_are_rejected() {
        let profile = profile();
        let manifest = manifest(&stable_evidence());
        assert!(validate_producer_revision(&profile, STALE_MANTLE_REVISION)
            .unwrap_err()
            .contains("Mantle producer revision drift"));
        assert!(validate_producer_revision(&profile, &MANTLE_REVISION[..9])
            .unwrap_err()
            .contains("observed Mantle revision"));
        assert!(validate_manifest_digest(&profile, STALE_MANIFEST_DIGEST)
            .unwrap_err()
            .contains("bundle manifest digest mismatch"));

        let mut stale_identity = profile.clone();
        stale_identity.bundle_identity_blake3 = String::from(STALE_BUNDLE_IDENTITY);
        assert!(validate_bundle_manifest(&stale_identity, &manifest)
            .unwrap_err()
            .contains("Mantle bundle identity does not match"));

        let mut stale_runner = profile;
        stale_runner.spacewasm_runner_blake3 = String::from(STALE_RUNNER_DIGEST);
        assert_eq!(
            validate_bundle_manifest(&stale_runner, &manifest),
            Err(String::from("SpaceWasm runner identity drift"))
        );
    }

    // r[verify chaoscontrol.spacewasm_mvp.materialization]
    // r[verify chaoscontrol.spacewasm_mvp.validation]
    #[test]
    fn tampered_or_missing_stable_reports_are_rejected() {
        let profile = profile();
        let evidence = stable_evidence();
        let admitted_manifest = manifest(&evidence);
        let renamed = String::from_utf8(PINNED_SPECTEST_STABLE_REPORT.to_vec())
            .expect("utf-8 report")
            .replacen("\"address\"", "\"addresz\"", 1)
            .into_bytes();
        assert_ne!(renamed, PINNED_SPECTEST_STABLE_REPORT);
        let mut tampered = evidence.clone();
        replace_evidence(&mut tampered, SPECTEST_SUITE.stable_report_path, renamed);
        assert!(admit_stable_evidence(&admitted_manifest, &tampered)
            .unwrap_err()
            .contains("stable evidence bytes do not match the bundle manifest"));
        assert!(admit_stable_evidence(&manifest(&tampered), &tampered)
            .unwrap_err()
            .contains("stable report identity does not match its admitted facts"));

        for suite in STABLE_EVIDENCE_SUITES {
            for missing_path in [suite.receipt_path, suite.stable_report_path] {
                let mut missing = admitted_manifest.clone();
                missing.members.retain(|member| member.path != missing_path);
                assert_eq!(
                    validate_bundle_manifest(&profile, &missing),
                    Err(format!(
                        "Mantle bundle is missing required stable evidence member: {missing_path}"
                    ))
                );
                assert!(validate_stable_evidence(
                    &missing,
                    &suite,
                    evidence_bytes(&evidence, suite.receipt_path),
                    evidence_bytes(&evidence, suite.stable_report_path),
                )
                .unwrap_err()
                .contains("missing required stable evidence member"));
            }
        }

        let mut wrong_role = admitted_manifest;
        wrong_role
            .members
            .iter_mut()
            .filter(|member| member.path == UNIT_SUITE.stable_report_path)
            .for_each(|member| member.role = String::from("result-report"));
        assert!(validate_bundle_manifest(&profile, &wrong_role)
            .unwrap_err()
            .contains("instead of check-receipt"));
    }

    #[test]
    fn receipts_must_bind_the_same_passing_stable_report() {
        let (_, report) = synthetic_unit_evidence(|_| {});
        let admitted: MantleStableReport = serde_json::from_slice(&report).expect("report");
        let failed_status = MantleStableReceipt {
            command: admitted.command.clone(),
            status: String::from("failed"),
            stable_report: admitted.clone(),
        };
        let mut other_report = admitted.clone();
        other_report.stable_identity_blake3 = String::from(PINNED_SPECTEST_STABLE_IDENTITY);
        let crossed_report = MantleStableReceipt {
            command: admitted.command.clone(),
            status: String::from(STABLE_RECEIPT_STATUS_PASSED),
            stable_report: other_report,
        };
        let crossed_command = MantleStableReceipt {
            command: String::from("cargo test"),
            status: String::from(STABLE_RECEIPT_STATUS_PASSED),
            stable_report: admitted,
        };
        for (receipt, expected) in [
            (failed_status, "stable receipt status is not passed"),
            (crossed_report, "stable receipt and stable report disagree"),
            (crossed_command, "stable receipt and stable report disagree"),
        ] {
            let evidence = stable_evidence_with_unit((
                serde_json::to_vec_pretty(&receipt).expect("receipt bytes"),
                report.clone(),
            ));
            assert!(admit_stable_evidence(&manifest(&evidence), &evidence)
                .unwrap_err()
                .contains(expected));
        }
    }

    #[test]
    fn raw_capture_members_are_rejected() {
        let profile = profile();
        for (path, role) in [
            (
                "reports/checks/upstream-unit-tests.stdout.txt",
                "check-receipt",
            ),
            (
                "reports/checks/upstream-spectest-address.stderr.txt",
                "check-receipt",
            ),
            ("runs/upstream-unit-tests/run-record.json", "check-receipt"),
            ("runs/upstream-unit-tests/stdout.txt", "check-receipt"),
            ("reports/checks/unit-run.json", "run-record"),
            ("reports/checks/unit-capture.json", "raw-capture"),
        ] {
            let mut raw = manifest(&stable_evidence());
            raw.members.push(member(path, role, MANIFEST_DIGEST));
            assert_eq!(
                validate_bundle_manifest(&profile, &raw),
                Err(format!(
                    "Mantle bundle carries a raw run capture member: {path}"
                ))
            );
        }
    }

    #[test]
    fn failed_or_skipped_tests_are_rejected() {
        for status in [
            MantleStableTestStatus::Failed,
            MantleStableTestStatus::Skipped,
        ] {
            let evidence = stable_evidence_with_unit(synthetic_unit_evidence(|report| {
                report.tests[1].status = status;
            }));
            assert!(admit_stable_evidence(&manifest(&evidence), &evidence)
                .unwrap_err()
                .contains("non-passing test in suite upstream-unit-tests: tests::bravo"));
        }
    }

    #[test]
    fn stable_report_grammar_drift_is_rejected() {
        let cases: [(ReportMutation, &str); 5] = [
            (
                |report| report.schema = String::from("mantle-spacewasm-stable-report-v2"),
                "unsupported stable report schema",
            ),
            (
                |report| report.encoding_version = MANTLE_STABLE_REPORT_ENCODING_VERSION + 1,
                "unsupported stable report encoding version",
            ),
            (
                |report| report.suite = String::from(SPECTEST_SUITE.suite),
                "stable report suite drift",
            ),
            (|report| report.tests.reverse(), "canonically ordered"),
            (|report| report.tests.clear(), "test count is outside"),
        ];
        for (mutate, expected) in cases {
            let evidence = stable_evidence_with_unit(synthetic_unit_evidence(mutate));
            assert!(admit_stable_evidence(&manifest(&evidence), &evidence)
                .unwrap_err()
                .contains(expected));
        }

        let (receipt, report) = synthetic_unit_evidence(|_| {});
        let mut extended: serde_json::Value = serde_json::from_slice(&report).expect("report");
        extended["elapsed_seconds"] = serde_json::Value::from(1);
        let evidence = stable_evidence_with_unit((
            receipt,
            serde_json::to_vec_pretty(&extended).expect("report bytes"),
        ));
        assert!(admit_stable_evidence(&manifest(&evidence), &evidence)
            .unwrap_err()
            .contains("stable report is outside the pinned grammar"));
    }

    // r[verify chaoscontrol.spacewasm_mvp.validation]
    #[test]
    fn timeout_and_resource_exhaustion_remain_distinct() {
        let timeout = normalize_tool(
            "wasmtime",
            CaseKind::Execute,
            &ToolObservation {
                success: false,
                timed_out: true,
                stderr: String::new(),
                stdout: String::new(),
            },
        );
        let exhausted = normalize_tool(
            "wasmtime",
            CaseKind::Execute,
            &ToolObservation {
                success: false,
                timed_out: false,
                stderr: String::from("all fuel consumed"),
                stdout: String::new(),
            },
        );
        assert_eq!(timeout.outcome, OutcomeClass::TimedOut);
        assert_eq!(exhausted.outcome, OutcomeClass::ResourceExhausted);
        assert_ne!(timeout, exhausted);
    }

    proptest! {
        #[test]
        fn generated_modules_are_bounded_and_magic_corruption_is_exact(seed in any::<u64>(), index in 0_usize..PROPERTY_CASE_INDEX_BOUND) {
            let module = generate_mvp_module(seed, index, MAX_INSTRUCTION_PAIRS);
            prop_assert!(module.len() < MAX_GENERATED_MODULE_BYTES);
            prop_assert_eq!(&module[..8], b"\0asm\x01\0\0\0");
            let malformed = malformed_magic(&module);
            prop_assert_eq!(malformed.len(), module.len());
            prop_assert_eq!(malformed[0], INVALID_MAGIC_BYTE);
            prop_assert_eq!(&malformed[1..], &module[1..]);
        }
    }

    // --- differential-execution-core adoption fixtures ---

    const DIFF_MODULE_BLAKE3: &str =
        "39e4790a7b9d0b14fcafffe5810e268cd8af342d38d7e952a6ede923e33882b2";

    fn matched_pair() -> (NormalizedObservation, NormalizedObservation) {
        (completed("spacewasm"), completed("wasmtime"))
    }

    fn diverging_pair() -> (NormalizedObservation, NormalizedObservation) {
        (completed("spacewasm"), trapped("wasmtime", "unreachable"))
    }

    #[test]
    fn harness_independence_gate_admits_two_distinct_engines() {
        let (spacewasm, wasmtime) = matched_pair();
        let case = admit_diff_case(DIFF_MODULE_BLAKE3, CaseKind::Execute)
            .expect("admit differential case");
        let left = admit_diff_observation(&case, "spacewasm", &spacewasm)
            .expect("admit spacewasm observation");
        let right = admit_diff_observation(&case, "wasmtime", &wasmtime)
            .expect("admit wasmtime observation");
        differential_execution_core::admit_independent_backends(Some(&left), Some(&right))
            .expect("two distinct engines must admit");
    }

    #[test]
    fn harness_independence_gate_rejects_duplicate_and_single_backend() {
        let (spacewasm, _) = matched_pair();
        let case = admit_diff_case(DIFF_MODULE_BLAKE3, CaseKind::Execute)
            .expect("admit differential case");
        let left = admit_diff_observation(&case, "spacewasm", &spacewasm)
            .expect("admit spacewasm observation");
        let duplicate = admit_diff_observation(&case, "spacewasm", &spacewasm)
            .expect("admit duplicate spacewasm observation");

        let duplicate_error =
            differential_execution_core::admit_independent_backends(Some(&left), Some(&duplicate))
                .expect_err("duplicate implementation must fail");
        assert!(matches!(
            duplicate_error,
            differential_execution_core::IndependenceError::DuplicateImplementation
        ));

        let single_error =
            differential_execution_core::admit_independent_backends(Some(&left), None)
                .expect_err("single backend must fail");
        assert!(matches!(
            single_error,
            differential_execution_core::IndependenceError::OneBackend
        ));
    }

    #[test]
    fn harness_case_identity_is_bound_and_reproducible() {
        let (spacewasm, wasmtime) = matched_pair();
        let first =
            compare_through_harness(DIFF_MODULE_BLAKE3, CaseKind::Execute, &spacewasm, &wasmtime)
                .expect("first comparison");
        let second =
            compare_through_harness(DIFF_MODULE_BLAKE3, CaseKind::Execute, &spacewasm, &wasmtime)
                .expect("second comparison");
        assert_eq!(first.case_identity_hex.len(), 64);
        assert_eq!(first.case_identity_hex, second.case_identity_hex);
    }

    // r[verify chaoscontrol.spacewasm_mvp.differential]
    #[test]
    fn harness_verdicts_preserve_prior_agreement_and_divergence() {
        let (spacewasm, wasmtime) = matched_pair();
        let agreement =
            compare_through_harness(DIFF_MODULE_BLAKE3, CaseKind::Execute, &spacewasm, &wasmtime)
                .expect("agreement comparison");
        assert!(matches!(
            agreement.verdict,
            differential_execution_core::ComparisonVerdict::Agreement
        ));
        assert!(agreement.mismatch.is_none());

        let (spacewasm_d, wasmtime_d) = diverging_pair();
        let divergence = compare_through_harness(
            DIFF_MODULE_BLAKE3,
            CaseKind::Execute,
            &spacewasm_d,
            &wasmtime_d,
        )
        .expect("divergence comparison");
        assert!(matches!(
            divergence.verdict,
            differential_execution_core::ComparisonVerdict::Divergence
        ));
        let mismatch = divergence
            .mismatch
            .expect("forced divergence must name a mismatch surface");
        assert!(matches!(
            mismatch.surface,
            differential_execution_core::MismatchSurface::Trap
                | differential_execution_core::MismatchSurface::ValueBytes
        ));
    }

    #[test]
    fn harness_oracle_classifies_exact_and_drifted_fixtures() {
        let (matched, _) = matched_pair();
        let oracle = classify_through_harness(
            DIFF_MODULE_BLAKE3,
            CaseKind::Execute,
            &matched,
            &matched,
            &matched,
        )
        .expect("exact oracle classification");
        assert!(matches!(
            oracle.left,
            differential_execution_core::OracleClassification::ExactMatch
        ));
        assert!(matches!(
            oracle.right,
            differential_execution_core::OracleClassification::ExactMatch
        ));

        let (_, diverging_wasmtime) = diverging_pair();
        let drifted = classify_through_harness(
            DIFF_MODULE_BLAKE3,
            CaseKind::Execute,
            &matched,
            &matched,
            &diverging_wasmtime,
        )
        .expect("drifted oracle classification");
        assert!(matches!(
            drifted.right,
            differential_execution_core::OracleClassification::Mismatch
        ));
    }
}
