# Typed review boundary for fresh schema-v2 KVM workload proofs.
let P = import "../evidence/profile-primitives.ncl" in
let MaximumWorkloads = 16 in
let GitRevision = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{40}$" value
) in
let StableAssertion = {
  namespace | P.non_empty_string,
  logical_key | P.non_empty_string,
  compatibility_id | P.u32,
  guest | P.identifier,
  category | P.identifier,
  message | P.non_empty_string,
} in
let ArtifactSelector = {
  role | P.closed_enum ["host-binary", "guest-initrd", "guest-kernel", "guest-disk"],
  nix_output | P.non_empty_string,
  digest_policy | P.exact_string "sha256-runtime-observation",
} in
let NamedBounds = {
  max_attempts | P.bounded_positive 16,
  vms | P.bounded_positive 9,
  rounds | P.bounded_positive 16,
  branches | P.bounded_positive 16,
  ticks | P.bounded_positive 10000,
  bootstrap_ticks | P.bounded_positive 100000,
  memory_mib | P.bounded_positive 4096,
  run_timeout_seconds | P.bounded_positive 3600,
  export_timeout_seconds | P.bounded_positive 3600,
  replay_timeout_seconds | P.bounded_positive 3600,
  snapshot_probe_fail_after | P.bounded_positive 10000,
} in
let Workload = {
  workload | P.identifier,
  order | P.bounded_positive MaximumWorkloads,
  assertion | StableAssertion,
  artifacts | Array ArtifactSelector,
  cmdline_template | P.non_empty_string,
  bounds | NamedBounds,
} in
let Shape = {
  schema_version | P.exact_value 1,
  cohort_id | P.identifier,
  source_revision | GitRevision,
  execution | {
    backend | P.exact_string "linux-kvm",
    device | P.exact_string "/dev/kvm",
    required_access | P.exact_string "read-write",
  },
  snapshot_policy | {
    codec | P.exact_string "simulation-snapshot-cbor-zstd-v2",
    schema_version | P.exact_value 2,
    digest | P.exact_string "sha256",
    minimum_replay_parent_depth | P.bounded_positive 16,
  },
  replay_policy | {
    verdict_schema_version | P.exact_value 2,
    required_class | P.exact_string "snapshot_backed_reproduced",
    exact_assertion_identity | P.exact_value true,
    exact_snapshot_reference | P.exact_value true,
  },
  receipt_policy | {
    required | P.exact_value true,
    artifact_digest | P.exact_string "sha256",
    raw_logs | P.exact_string "local-debug-only",
  },
  non_claims | Array P.non_empty_string,
  workloads | Array Workload,
} in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked =>
      let orders_unique = P.unique_by (fun workload => workload.order) checked.workloads in
      let names_unique = P.unique_by (fun workload => workload.workload) checked.workloads in
      let aliases_unique = P.unique_by (fun workload => workload.assertion.compatibility_id) checked.workloads in
      let complete_non_claims = std.array.length checked.non_claims >= 4 in
      let artifacts_complete = std.array.all (fun workload =>
        std.array.length workload.artifacts >= 3
        && std.array.any (fun artifact => artifact.role == "host-binary") workload.artifacts
        && std.array.any (fun artifact => artifact.role == "guest-kernel") workload.artifacts
        && std.array.any (fun artifact => artifact.role == "guest-initrd") workload.artifacts
      ) checked.workloads in
      if std.array.length checked.workloads == 4
        && orders_unique
        && names_unique
        && aliases_unique
        && complete_non_claims
        && artifacts_complete
      then 'Ok checked
      else 'Error {},
    'Error error => 'Error error,
  }
)
