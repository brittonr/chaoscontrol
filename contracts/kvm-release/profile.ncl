# Typed review boundary for the required KVM release-evidence matrix.
let P = import "../evidence/profile-primitives.ncl" in
let MaximumRows = 32 in
let MaximumCapabilities = 16 in
let MaximumCommandArguments = 64 in
let MaximumRetainedArtifacts = 4096 in
let MaximumTimeoutSeconds = 7200 in
let MaximumReceiptAgeSeconds = 604800 in
let MaximumArtifactBytes = 1073741824 in
let RowKinds = [
  "cohort-binary-build",
  "cohort-guest-build",
  "deterministic-smp",
  "serialized-snapshot-replay",
  "virtio-malformed-input",
  "admitted-drift",
  "fresh-workload-replay",
  "pmu",
] in
let RequiredBaseKinds = [
  "deterministic-smp",
  "serialized-snapshot-replay",
  "virtio-malformed-input",
  "admitted-drift",
  "fresh-workload-replay",
] in
let Command = {
  program | P.non_empty_string,
  args | Array P.non_empty_string,
} in
let Limits = {
  timeout_seconds | P.bounded_positive MaximumTimeoutSeconds,
  max_artifacts | P.bounded_positive MaximumRetainedArtifacts,
  max_artifact_bytes | P.bounded_positive MaximumArtifactBytes,
} in
let MatrixRow = {
  id | P.identifier,
  kind | P.closed_enum RowKinds,
  required | P.exact_value true,
  required_capabilities | Array P.identifier,
  command | Command,
  limits | Limits,
  retained_artifacts | Array P.non_empty_string,
} in
let Shape = {
  schema_version | P.exact_value 1,
  profile_id | P.identifier,
  required_worker_arch | P.exact_string "x86_64",
  required_worker_capabilities | Array P.identifier,
  max_receipt_age_seconds | P.bounded_positive MaximumReceiptAgeSeconds,
  claims_pmu_support | Bool,
  bounded_claim | P.non_empty_string,
  non_claims | Array P.non_empty_string,
  rows | Array MatrixRow,
} in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked =>
      let row_count_valid = std.array.length checked.rows > 0
        && std.array.length checked.rows <= MaximumRows in
      let row_ids_unique = P.unique_by (fun row => row.id) checked.rows in
      let row_kinds_unique = P.unique_by (fun row => row.kind) checked.rows in
      let worker_capabilities_valid = std.array.length checked.required_worker_capabilities > 0
        && std.array.length checked.required_worker_capabilities <= MaximumCapabilities
        && P.unique_by (fun capability => capability) checked.required_worker_capabilities in
      let rows_bounded = std.array.all (fun row =>
        std.array.length row.command.args > 0
        && std.array.length row.command.args <= MaximumCommandArguments
        && std.array.length row.required_capabilities > 0
        && std.array.length row.required_capabilities <= MaximumCapabilities
        && P.unique_by (fun capability => capability) row.required_capabilities
        && std.array.length row.retained_artifacts > 0
      ) checked.rows in
      let base_rows_present = std.array.all (fun required_kind =>
        std.array.any (fun row => row.kind == required_kind && row.required) checked.rows
      ) RequiredBaseKinds in
      let pmu_boundary_valid = !checked.claims_pmu_support
        || std.array.any (fun row => row.kind == "pmu" && row.required) checked.rows in
      let non_claims = std.string.join " " checked.non_claims in
      let non_claims_complete = std.array.length checked.non_claims >= 5
        && std.string.is_match "worker integrity" non_claims
        && std.string.is_match "all-host equivalence" non_claims
        && std.string.is_match "universal determinism" non_claims
        && std.string.is_match "workload correctness" non_claims
        && std.string.is_match "production availability" non_claims in
      if row_count_valid
        && row_ids_unique
        && row_kinds_unique
        && worker_capabilities_valid
        && rows_bounded
        && base_rows_present
        && pmu_boundary_valid
        && non_claims_complete
      then 'Ok checked
      else 'Error {},
    'Error error => 'Error error,
  }
)
