# Nickel-authored VM exploration profile. Runtime observations are not fields.
let P = import "profile-primitives.ncl" in
let Scheduler = import "scheduler-progress.ncl" in
let ProgressBudget = Scheduler.ProgressBudget in
let ProgressMode = std.contract.custom (fun label value =>
  if std.is_string value then
    std.contract.check (P.closed_enum ["ExactSingleStep", "KvmDebugBoundary"]) label value
  else
    std.contract.check { PmuAccelerated | { exact_step_margin | P.positive_u64 } } label value
) in
let IsInstructions = fun budget => std.record.has_field "Instructions" budget in
let Count = Scheduler.Count in
let SameUnit = Scheduler.SameUnit in
let Scheduling = {
  strategy | P.closed_enum ["round-robin", "randomized"],
  minimum_budget | ProgressBudget,
  maximum_budget | ProgressBudget,
  journal_limit | P.positive_u64,
  entry_watchdog_microseconds | P.bounded_positive 9223372036854775,
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
  outer_exit_budget | P.positive_u64,
  progress_budget | ProgressBudget,
  bootstrap_budget | P.positive_u64,
  stale_round_limit | P.u64,
  # Absent means "campaign", the default Campaign-backed frontier policy.
  # "legacy-diagnostic" is explicit diagnostic rollback only: its outputs are
  # marked release_eligible = false and every release/dogfood gate refuses them.
  frontier_policy | P.closed_enum ["campaign", "legacy-diagnostic"] | optional,
  # Absent means the system-run default: every run injects seed-derived random
  # faults in addition to its schedule. false is the explicit opt-out.
  random_faults | Bool | optional,
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
  schema | P.exact_string "chaoscontrol.run-profile.v3",
  profile_id | P.identifier,
  mode | P.exact_string "vm-exploration",
  progress_mode | ProgressMode,
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
  let scheduling = value.topology.scheduling in
  let budget = value.exploration.progress_budget in
  SameUnit budget scheduling.minimum_budget
  && SameUnit budget scheduling.maximum_budget
  && Count scheduling.minimum_budget <= Count scheduling.maximum_budget
  && (if std.is_string value.progress_mode then
      (value.progress_mode == "KvmDebugBoundary") == !(IsInstructions budget)
      && (value.progress_mode != "KvmDebugBoundary" || value.topology.num_vcpus >= 2)
    else IsInstructions budget
      && value.progress_mode.PmuAccelerated.exact_step_margin < Count scheduling.minimum_budget)
  && (value.topology.num_vcpus > 1 || value.topology.scheduling.strategy == "round-robin")
  && (value.topology.num_vcpus > 1 || !value.topology.scheduling.diversity)
  && (if scheduling.strategy == "randomized" then
      Count scheduling.minimum_budget < Count scheduling.maximum_budget
    else scheduling.minimum_budget == scheduling.maximum_budget
      && scheduling.minimum_budget == budget)
  && (value.progress_mode != "KvmDebugBoundary"
    || (value.guest_cmdline != null
      && std.string.is_match "(^|[[:space:]])noapictimer([[:space:]]|$)" value.guest_cmdline
      && std.string.is_match "(^|[[:space:]])clocksource=jiffies([[:space:]]|$)" value.guest_cmdline
      && std.array.all (fun word =>
        (!(std.string.is_match "^clocksource=" word) || word == "clocksource=jiffies")
        && !(std.string.is_match "^(clock=|tsc=)|^tsc$|^hpet=force$" word)
      ) (std.string.split " " value.guest_cmdline)
      && std.string.is_match "(^|[[:space:]])notsc([[:space:]]|$)" value.guest_cmdline))
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
