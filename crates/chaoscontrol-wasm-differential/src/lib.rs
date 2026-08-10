//! Bounded, diagnostic-only differential evidence for the SpaceWasm MVP cohort.
//!
//! Pure admission, generation, normalization, comparison, and receipt identity
//! logic lives here. Filesystem and process effects remain in the binary shell.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const PROFILE_SCHEMA: &str = "chaoscontrol.spacewasm-mvp-differential-profile.v1";
pub const REPORT_SCHEMA: &str = "chaoscontrol.spacewasm-mvp-differential-report.v1";
pub const MANTLE_MANIFEST_SCHEMA: &str = "mantle-spacewasm-reference-bundle-v1";
pub const MANTLE_RUNNER_REPORT_SCHEMA: &str = "mantle-spacewasm-runner-report-v1";
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
const REQUIRED_OBSERVATION_FIELDS: [&str; 5] = [
    "outcome",
    "resource-class",
    "return-values",
    "state-identity-blake3",
    "trap-class",
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedObservation {
    pub engine: String,
    pub outcome: OutcomeClass,
    pub return_values: Vec<String>,
    pub trap_class: Option<String>,
    pub state_identity_blake3: Option<String>,
    pub resource_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseComparison {
    pub case_id: String,
    pub case_kind: CaseKind,
    pub module_blake3: String,
    pub spacewasm: NormalizedObservation,
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
    pub verdict: String,
    pub non_claims: Vec<String>,
    pub report_identity_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolObservation {
    pub success: bool,
    pub timed_out: bool,
    pub stderr: String,
}

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
    validate_bounds(&profile.bounds)
}

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
    ];
    if positive.contains(&0) {
        return Err(String::from("all differential bounds must be positive"));
    }
    if bounds.maximum_bundle_member_bytes > bounds.maximum_bundle_total_bytes {
        return Err(String::from(
            "maximum bundle member bytes exceeds total bytes",
        ));
    }
    Ok(())
}

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
    Ok(())
}

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

pub fn compare_case(
    case_id: String,
    case_kind: CaseKind,
    module_blake3: String,
    spacewasm: NormalizedObservation,
    wasmtime: NormalizedObservation,
) -> CaseComparison {
    let first_difference = first_difference(&spacewasm, &wasmtime);
    let verdict = if first_difference.is_none() {
        ComparisonVerdict::Match
    } else {
        ComparisonVerdict::Mismatch
    };
    CaseComparison {
        case_id,
        case_kind,
        module_blake3,
        spacewasm,
        wasmtime,
        verdict,
        first_difference,
        minimization: None,
    }
}

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

pub fn finalize_report(mut report: DifferentialReport) -> Result<DifferentialReport, String> {
    report.mismatch_count = report
        .comparisons
        .iter()
        .filter(|comparison| comparison.verdict == ComparisonVerdict::Mismatch)
        .count();
    report.verdict = if report.mismatch_count == 0 {
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

    const MANTLE_REVISION: &str = "a141fcbaafe41f9a413a81275a33fe915bfca370";
    const SPACEWASM_REVISION: &str = "e24cf09355a90497148eb5029fdb8e3400bd63e3";
    const MANIFEST_DIGEST: &str =
        "39e4790a7b9d0b14fcafffe5810e268cd8af342d38d7e952a6ede923e33882b2";
    const BUNDLE_IDENTITY: &str =
        "c4826bb63fa9eef1fa619e0f0c4c2c35dd10ca92a8d4999fec10c55e92b692b7";
    const STALE_BUNDLE_IDENTITY: &str =
        "cee7190f2f78321b07f3d1f493baaa5b2cb74d517eb4f229c7e7a6094b877342";
    const RUNNER_DIGEST: &str = "be8aeb698afdecf6fb608910980292517ed952f122b6447705d4bdae485b0221";
    const MAX_INSTRUCTION_PAIRS: usize = 32;
    const MAX_GENERATED_MODULE_BYTES: usize = 256;
    const PROPERTY_CASE_INDEX_BOUND: usize = 1_000;

    fn profile() -> DifferentialProfile {
        DifferentialProfile {
            schema: String::from(PROFILE_SCHEMA),
            profile_id: String::from("spacewasm-mvp-differential-v1"),
            mantle_revision: String::from(MANTLE_REVISION),
            mantle_profile_id: String::from("nasa-spacewasm-e24cf093-reference-v1"),
            spacewasm_revision: String::from(SPACEWASM_REVISION),
            spacewasm_runner_blake3: String::from(RUNNER_DIGEST),
            wasmtime_version: String::from("wasmtime 41.0.3"),
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
            },
            non_claims: REQUIRED_NON_CLAIMS.into_iter().map(String::from).collect(),
        }
    }

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
        );
        assert_eq!(comparison.verdict, ComparisonVerdict::Mismatch);
        let same_predicate = compare_case(
            String::from("same-predicate"),
            CaseKind::Execute,
            String::from(MANIFEST_DIGEST),
            completed("spacewasm"),
            trapped("wasmtime", "unreachable"),
        );
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

    #[test]
    fn bundle_drift_and_evidence_overclaim_are_rejected() {
        let profile = profile();
        let digest = String::from(MANIFEST_DIGEST);
        let members = [
            (
                "binaries/host/mantle-spacewasm-diagnostic-runner",
                "host-runner",
            ),
            ("fixtures/wasm/mvp-positive.wasm", "fixture-artifact"),
            ("profile/profile.json", "profile-export"),
            ("source/spacewasm.tar.gz", "source-archive"),
        ]
        .into_iter()
        .map(|(path, role)| BundleMember {
            path: String::from(path),
            role: String::from(role),
            digest_blake3: if role == "host-runner" {
                String::from(RUNNER_DIGEST)
            } else {
                digest.clone()
            },
            size_bytes: 1,
        })
        .collect();
        let manifest = BundleManifest {
            schema: String::from(MANTLE_MANIFEST_SCHEMA),
            profile_id: String::from("nasa-spacewasm-e24cf093-reference-v1"),
            profile_identity_blake3: digest.clone(),
            cohort_identity_blake3: digest,
            members,
            parent_edges: Vec::new(),
            non_claims: Vec::new(),
            bundle_identity_blake3: String::from(BUNDLE_IDENTITY),
        };
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

    #[test]
    fn timeout_and_resource_exhaustion_remain_distinct() {
        let timeout = normalize_tool(
            "wasmtime",
            CaseKind::Execute,
            &ToolObservation {
                success: false,
                timed_out: true,
                stderr: String::new(),
            },
        );
        let exhausted = normalize_tool(
            "wasmtime",
            CaseKind::Execute,
            &ToolObservation {
                success: false,
                timed_out: false,
                stderr: String::from("all fuel consumed"),
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
}
