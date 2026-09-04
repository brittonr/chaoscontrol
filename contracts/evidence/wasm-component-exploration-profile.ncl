# Human-authored bounds and exact cohort admission for the bounded host-side
# adversarial Wasm component exploration rail. Generated corpora, mutations,
# execution observations, shrink steps, and reports stay Rust-owned.
#
# Schema v1 pins bundle_status to "not-yet-materialized": no Mantle
# materialization bundle is admitted yet, so every v1 case is diagnostic-only.
# Proof-eligible admission requires a later schema version that binds exact
# bundle member BLAKE3 identities from a verified Mantle materialization.
let P = import "profile-primitives.ncl" in
let MaximumArtifactBytes = 16777216 in
let MaximumWitBytes = 1048576 in
let MaximumStructuralCount = 4096 in
let MaximumMemoryTableCount = 64 in
let MaximumHostcallBytes = 1048576 in
let MaximumGenerationCases = 4096 in
let MaximumMutationCases = 4096 in
let MaximumBranches = 4096 in
let MaximumSamples = 65536 in
let MaximumShrinkAttempts = 4096 in
let MaximumRuntimeSteps = 1000000000 in
let MaximumConcurrentCases = 64 in
let MaximumRetainedArtifacts = 4096 in
let MaximumFuel = 1000000000 in
let GitRevision = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{40}$" value
) in
let ExactGenerators = std.contract.from_predicate (fun value =>
  value == ["wasm-smith", "wit-smith", "wasm-tools-component"]
) in
let ExactMutators = std.contract.from_predicate (fun value =>
  value == ["wasm-mutate", "profile-aware"]
) in
let ExactShrinkers = std.contract.from_predicate (fun value =>
  value == ["wasm-shrink", "bounded-reducer"]
) in
let ExactStrategies = std.contract.from_predicate (fun value =>
  value == ["cranelift", "pulley"]
) in
let ExactCorpusClasses = std.contract.from_predicate (fun value =>
  value == [
    "baseline-component",
    "baseline-wit-package",
    "generated-malformed",
    "generated-valid",
    "mutated-invalid",
    "mutated-valid",
  ]
) in
let ExactOutcomeClasses = std.contract.from_predicate (fun value =>
  value == [
    "bound-skip",
    "compile-rejection",
    "deterministic-trap",
    "harness-error",
    "link-rejection",
    "profile-rejection",
    "replay-mismatch",
    "shrink-complete",
    "static-rejection",
    "strategy-divergence",
    "strategy-match",
    "transform-divergence",
    "transform-match",
  ]
) in
let ExactObservationFields = std.contract.from_predicate (fun value =>
  value == [
    "hostcall-transcript",
    "outcome",
    "preserves-output",
    "resource-class",
    "result-values",
    "state-identity-blake3",
    "trap-class",
  ]
) in
let RequiredNonClaims = std.contract.from_predicate (fun value =>
  std.array.elem "not-assertion-readiness" value
  && std.array.elem "not-package-trust" value
  && std.array.elem "not-release-eligibility" value
  && std.array.elem "not-sandbox-proof" value
  && std.array.elem "not-semantic-equivalence" value
  && std.array.elem "not-vm-replay-proof" value
) in
let NonClaim = P.closed_enum [
  "not-assertion-readiness",
  "not-package-trust",
  "not-release-eligibility",
  "not-sandbox-proof",
  "not-semantic-equivalence",
  "not-vm-replay-proof",
] in
let Materialization = {
  mantle_revision | GitRevision,
  bundle_profile_id | P.identifier,
  bundle_status | P.exact_string "not-yet-materialized",
} in
let ToolCohort = {
  suite_source | P.exact_string "nix-lock",
  wasm_tools_suite | P.exact_string "wasm-tools 1.245.1",
  generators | ExactGenerators,
  mutators | ExactMutators,
  shrinkers | ExactShrinkers,
  waffle | P.nullable_non_empty_string,
} in
let RuntimeCohort = {
  wasmtime_version | P.exact_string "wasmtime 43.0.0",
  strategies | ExactStrategies,
  proposals | P.exact_string "all-proposals-disabled",
  component_features | P.exact_string "post-mvp-features-disabled",
  host_authority | P.exact_string "none",
  epoch_deadline | P.exact_string "none",
  wasmtime_fuel | P.bounded_positive MaximumFuel,
} in
let Bounds = {
  maximum_artifact_bytes | P.bounded_positive MaximumArtifactBytes,
  maximum_wit_bytes | P.bounded_positive MaximumWitBytes,
  maximum_imports | P.bounded_positive MaximumStructuralCount,
  maximum_exports | P.bounded_positive MaximumStructuralCount,
  maximum_functions | P.bounded_positive MaximumStructuralCount,
  maximum_types | P.bounded_positive MaximumStructuralCount,
  maximum_memories | P.bounded_positive MaximumMemoryTableCount,
  maximum_tables | P.bounded_positive MaximumMemoryTableCount,
  maximum_hostcall_bytes | P.bounded_positive MaximumHostcallBytes,
  maximum_generation_cases | P.bounded_positive MaximumGenerationCases,
  maximum_mutation_cases | P.bounded_positive MaximumMutationCases,
  maximum_branches | P.bounded_positive MaximumBranches,
  maximum_samples | P.bounded_positive MaximumSamples,
  maximum_shrink_attempts | P.bounded_positive MaximumShrinkAttempts,
  maximum_runtime_steps | P.bounded_positive MaximumRuntimeSteps,
  maximum_concurrent_cases | P.bounded_positive MaximumConcurrentCases,
  maximum_retained_artifacts | P.bounded_positive MaximumRetainedArtifacts,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.wasm-component-exploration-profile.v1",
  profile_id | P.identifier,
  materialization | Materialization,
  tool_cohort | ToolCohort,
  runtime_cohort | RuntimeCohort,
  corpus_classes | ExactCorpusClasses,
  outcome_classes | ExactOutcomeClasses,
  observation_fields | ExactObservationFields,
  bounds | Bounds,
  retention | P.non_empty_string,
  non_claims | Array NonClaim,
} in
let Coherent = fun value =>
  value.bounds.maximum_wit_bytes <= value.bounds.maximum_artifact_bytes
  && value.bounds.maximum_retained_artifacts <= value.bounds.maximum_samples
  && value.bounds.maximum_shrink_attempts <= value.bounds.maximum_mutation_cases
  && std.array.length value.non_claims == 6
  && P.unique_by (fun claim => claim) value.non_claims
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
