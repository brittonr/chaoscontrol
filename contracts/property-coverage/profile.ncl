# r[impl chaoscontrol.property_coverage.profile]
# r[impl chaoscontrol.property_coverage.boundary]
# Typed review boundary for bounded state-machine property profiles.
let P = import "../evidence/profile-primitives.ncl" in
let MaximumSeeds = 16 in
let MaximumSequences = 4096 in
let MaximumSteps = 1024 in
let MaximumShrinkAttempts = 8192 in
let MaximumCounterexamples = 128 in
let MaximumRuntimeSeconds = 7200 in
let Shape = {
  schema_version | P.exact_value 1,
  id | P.identifier,
  target_models | Array P.identifier,
  seed_policy | P.exact_string "named-explicit",
  seeds | Array Number,
  sequence_count | P.bounded_positive MaximumSequences,
  max_steps | P.bounded_positive MaximumSteps,
  max_shrink_attempts | P.bounded_positive MaximumShrinkAttempts,
  max_retained_counterexamples | P.bounded_positive MaximumCounterexamples,
  max_runtime_seconds | P.bounded_positive MaximumRuntimeSeconds,
  claim | P.non_empty_string,
  non_claims | Array P.non_empty_string,
} in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked =>
      let seed_count = std.array.length checked.seeds in
      let seeds_valid = seed_count > 0
        && seed_count <= MaximumSeeds
        && P.unique_by (fun seed => seed) checked.seeds in
      let targets_valid = std.array.length checked.target_models > 0
        && P.unique_by (fun target => target) checked.target_models in
      let non_claims = std.string.join " " checked.non_claims in
      let boundary_complete = std.array.length checked.non_claims >= 5
        && std.string.is_match "formal proof" non_claims
        && std.string.is_match "whole-system correctness" non_claims
        && std.string.is_match "exhaustive state coverage" non_claims
        && std.string.is_match "KVM behavior" non_claims
        && std.string.is_match "absence of defects" non_claims in
      let claim_bounded = !std.string.is_match "formal proof" checked.claim
        && !std.string.is_match "complete correctness" checked.claim
        && !std.string.is_match "absence of defects" checked.claim in
      if seeds_valid && targets_valid && boundary_complete && claim_bounded
      then 'Ok checked
      else 'Error {},
    'Error error => 'Error error,
  }
)
