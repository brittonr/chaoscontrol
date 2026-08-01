# Nickel-authored VM exploration profile. Runtime observations are not fields.
let P = import "profile-primitives.ncl" in
let Scheduling = {
  strategy | P.closed_enum ["round-robin", "randomized"],
  minimum_quantum | P.positive_u64,
  maximum_quantum | P.positive_u64,
  diversity | Bool,
} in
let Artifacts = {
  kernel | P.artifact_reference,
  initrd | P.nullable_artifact_reference,
  disk | P.nullable_artifact_reference,
} in
let Topology = {
  num_vms | P.bounded_positive 64,
  num_vcpus | P.bounded_positive 64,
  memory_mib | P.bounded_positive 1048576,
  scheduling | Scheduling,
} in
let Exploration = {
  mode | P.closed_enum ["fault-schedule", "input-tree", "hybrid"],
  branch_factor | P.bounded_positive 4096,
  ticks_per_branch | P.positive_u64,
  max_rounds | P.positive_u64,
  max_frontier | P.bounded_positive 1048576,
  quantum | P.positive_u64,
  bootstrap_budget | P.positive_u64,
  stale_round_limit | P.u64,
} in
let Coverage = {
  mode | P.closed_enum ["blind", "bitmap"],
  bitmap_gpa | P.u64,
} in
let Logging = {
  raw_log_policy | P.closed_enum ["disabled", "debug-only-excluded-from-git"],
  determinism_log_policy | P.closed_enum ["disabled", "bounded"],
  determinism_log_output | P.nullable_artifact_reference,
  register_interval | P.u64,
  memory_hash | Bool,
} in
let RunProfileShape = {
  schema | P.exact_string "chaoscontrol.run-profile.v2",
  profile_id | P.identifier,
  mode | P.exact_string "vm-exploration",
  artifacts | Artifacts,
  guest_cmdline | P.nullable_non_empty_string,
  seed | P.u64,
  topology | Topology,
  exploration | Exploration,
  coverage | Coverage,
  logging | Logging,
  scope | P.exact_string "pre-run VM exploration intent; not KVM availability, guest correctness, deterministic replay, fault effect, campaign completion, or accepted evidence",
} in
let Coherent = fun value =>
  value.topology.scheduling.minimum_quantum <= value.topology.scheduling.maximum_quantum
  && (value.topology.num_vcpus > 1 || value.topology.scheduling.strategy == "round-robin")
  && (value.topology.num_vcpus > 1 || !value.topology.scheduling.diversity)
  && (value.topology.scheduling.strategy == "randomized"
    || (value.topology.scheduling.minimum_quantum == value.topology.scheduling.maximum_quantum
      && value.topology.scheduling.minimum_quantum == value.exploration.quantum))
  && ((value.coverage.mode == "blind" && value.coverage.bitmap_gpa == 0)
    || (value.coverage.mode == "bitmap" && value.coverage.bitmap_gpa > 0))
  && ((value.logging.determinism_log_policy == "disabled"
      && value.logging.determinism_log_output == null
      && value.logging.register_interval == 0
      && !value.logging.memory_hash)
    || (value.logging.determinism_log_policy == "bounded"
      && value.logging.determinism_log_output != null))
in
std.contract.custom (fun label value =>
  std.contract.check RunProfileShape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
