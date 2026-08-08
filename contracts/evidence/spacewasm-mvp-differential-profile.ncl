# Human-authored bounds and exact cohort admission for the diagnostic-only
# SpaceWasm/Wasmtime MVP differential rail. Runtime observations stay Rust-owned.
let P = import "profile-primitives.ncl" in
let MaximumCases = 1024 in
let MaximumInputBytes = 16777216 in
let MaximumBundleMembers = 4096 in
let MaximumBundleMemberBytes = 1073741824 in
let MaximumBundleTotalBytes = 4294967296 in
let MaximumProcessMilliseconds = 300000 in
let MaximumOutputBytes = 16777216 in
let MaximumFuel = 1000000000 in
let MaximumGeneratedInstructionPairs = 4096 in
let MaximumShrinkAttempts = 4096 in
let MaximumResumeSegments = 4096 in
let GitRevision = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{40}$" value
) in
let Blake3Hex = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value
) in
let ExactFeatureIntersection = std.contract.from_predicate (fun value =>
  value == ["wasm1", "mutable-globals"]
) in
let ExactChunkSchedules = std.contract.from_predicate (fun value =>
  value == ["complete", "one-byte"]
) in
let ExactCorpusClasses = std.contract.from_predicate (fun value =>
  value == [
    "completion",
    "fuel-exhaustion",
    "generated-malformed",
    "generated-valid",
    "malformed",
    "streaming-malformed",
    "streaming-valid",
    "trap-unreachable",
  ]
) in
let ExactObservationFields = std.contract.from_predicate (fun value =>
  value == [
    "outcome",
    "resource-class",
    "return-values",
    "state-identity-blake3",
    "trap-class",
  ]
) in
let RequiredNonClaims = std.contract.from_predicate (fun value =>
  std.array.elem "not-spacewasm-correctness" value
  && std.array.elem "not-spacewasm-wasmtime-equivalence" value
  && std.array.elem "not-memory-safety" value
  && std.array.elem "not-webassembly-conformance" value
  && std.array.elem "not-flight-qualification" value
  && std.array.elem "not-sandbox-effectiveness" value
  && std.array.elem "not-production-readiness" value
  && std.array.elem "not-release-eligibility" value
) in
let Bounds = {
  maximum_cases | P.bounded_positive MaximumCases,
  maximum_input_bytes | P.bounded_positive MaximumInputBytes,
  maximum_bundle_members | P.bounded_positive MaximumBundleMembers,
  maximum_bundle_member_bytes | P.bounded_positive MaximumBundleMemberBytes,
  maximum_bundle_total_bytes | P.bounded_positive MaximumBundleTotalBytes,
  maximum_process_milliseconds | P.bounded_positive MaximumProcessMilliseconds,
  maximum_output_bytes | P.bounded_positive MaximumOutputBytes,
  wasmtime_fuel | P.bounded_positive MaximumFuel,
  maximum_generated_instruction_pairs | P.bounded_positive MaximumGeneratedInstructionPairs,
  maximum_shrink_attempts | P.bounded_positive MaximumShrinkAttempts,
} in
let ModuleAbi = {
  required_export | P.exact_string "run",
  parameter_count | P.exact_value 0,
  result_count | P.exact_value 0,
  maximum_imports | P.exact_value 0,
  maximum_memories | P.exact_value 0,
  maximum_tables | P.exact_value 0,
  maximum_host_functions | P.exact_value 0,
  maximum_linear_memory_bytes | P.exact_value 0,
  memory_grow | P.exact_string "not-admitted",
} in
let Runtime = {
  spacewasm_resume_segment_fuel | P.bounded_positive MaximumFuel,
  maximum_resume_segments | P.bounded_positive MaximumResumeSegments,
  wasmtime_proposals | P.exact_string "all-proposals-disabled",
  host_authority | P.exact_string "none",
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.spacewasm-mvp-differential-profile.v1",
  profile_id | P.identifier,
  mantle_revision | GitRevision,
  mantle_profile_id | P.identifier,
  spacewasm_revision | GitRevision,
  spacewasm_runner_blake3 | Blake3Hex,
  wasmtime_version | P.exact_string "wasmtime 41.0.3",
  bundle_manifest_blake3 | Blake3Hex,
  bundle_identity_blake3 | Blake3Hex,
  generator_id | P.identifier,
  seed | P.u64,
  generated_valid_cases | P.positive_u64,
  generated_malformed_cases | P.positive_u64,
  feature_intersection | ExactFeatureIntersection,
  module_abi | ModuleAbi,
  runtime | Runtime,
  chunk_schedules | ExactChunkSchedules,
  corpus_classes | ExactCorpusClasses,
  observation_fields | ExactObservationFields,
  bounds | Bounds,
  non_claims | RequiredNonClaims,
} in
let FixedCases = 6 in
let Coherent = fun value =>
  value.generated_valid_cases
  + value.generated_malformed_cases
  + FixedCases
  <= value.bounds.maximum_cases
  && value.bounds.maximum_bundle_member_bytes <= value.bounds.maximum_bundle_total_bytes
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
