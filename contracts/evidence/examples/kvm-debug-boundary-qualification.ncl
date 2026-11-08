# Runtime paths are required arguments. The measured identities below select
# inputs, never a successful execution or supported IRQ/NMI delivery claim.
let Qualification = import "../kvm-debug-boundary-qualification.ncl" in
let Run = import "../run-config.ncl" in
let Paths = {
  kernel | String,
  initrd | String,
  observation_profile | String,
  admitted_observation_profiles | String,
} in
let Reference = fun artifact_path artifact_identity => {
  kind = if std.string.is_match "^/" artifact_path then "absolute-path" else "relative-artifact",
  path = artifact_path,
  identity = artifact_identity,
} in
let BoundaryBudget = fun boundary_count => {
  KvmBoundaries = { profile_id = "KvmDebugExitBoundaryV1", count = boundary_count },
} in
let Seed = 42 in
let BoundaryCount = 64 in
# Preserve the authored Raft profile's outer controller exit bound; it is not
# the boundary scheduler count even when both bound the same run.
let OuterExitBudget = 100 in
let RandomMinimum = 32 in
let RandomExclusiveMaximum = 128 in
# Two is the minimum paired identical-input control; retain both outcomes.
let RepetitionCount = 2 in
let BootstrapTicks = 100000 in
let HistoryTicks = 20 in
let CheckpointIntervalTicks = 10 in
let JournalRecords = 65536 in
let EntryWatchdogMicroseconds = 250000 in
let OuterDeadlineMilliseconds = 3600000 in
let TeardownDeadlineMilliseconds = 10000 in
fun supplied_paths =>
let paths = supplied_paths | Paths in
let GuestArtifacts = {
  kernel = Reference paths.kernel "blake3:f2abae8a52c83c975ed15de504535445e2ac159cc0198f8028867b0c18cdf67c",
  initrd = Reference paths.initrd "blake3:bdab04c42d49ae3f75468e1c2e6408c4829bcd5c17d8792ac2e090eeeeb972fa",
  disk = null,
} in
let MakeRun = fun strategy_name minimum maximum => {
  schema = "chaoscontrol.run-profile.v3",
  profile_id = "kvm-debug-boundary-paired-control",
  mode = "vm-exploration",
  progress_mode = "KvmDebugBoundary",
  artifacts = GuestArtifacts,
  guest_cmdline = "noapictimer clocksource=jiffies notsc",
  seed = Seed,
  topology = {
    num_vms = 1,
    num_vcpus = 2,
    memory_mib = 128,
    scheduling = {
      strategy = strategy_name,
      minimum_budget = BoundaryBudget minimum,
      maximum_budget = BoundaryBudget maximum,
      journal_limit = JournalRecords,
      entry_watchdog_microseconds = EntryWatchdogMicroseconds,
      diversity = false,
    },
  },
  exploration = {
    mode = "fault-schedule",
    branch_factor = 1,
    ticks_per_branch = HistoryTicks,
    max_rounds = 1,
    max_frontier = 1,
    outer_exit_budget = OuterExitBudget,
    progress_budget = BoundaryBudget BoundaryCount,
    bootstrap_budget = BootstrapTicks,
    stale_round_limit = 0,
    frontier_policy = "campaign",
    random_faults = false,
  },
  coverage = { mode = "blind", bitmap_gpa = 0 },
  logging = {
    raw_log_policy = "debug-only-excluded-from-git",
    determinism_log_policy = "disabled",
    determinism_log_output = null,
    register_interval = 0,
    memory_hash = false,
  },
  scope = "pre-run VM exploration intent; not KVM availability, guest correctness, deterministic replay, fault effect, campaign completion, or accepted evidence",
} | Run in
{
  schema = "chaoscontrol.kvm-debug-boundary-qualification.v1",
  qualification_id = "ryzen9950x3d-linux61850-rust-observer-paired-v2",
  status = "qualification-inputs-only",
  host = {
    architecture = "x86_64",
    cpu_vendor = "AuthenticAMD",
    cpu_model = "AMD Ryzen 9 9950X3D 16-Core Processor",
    cpu_family = 26,
    cpu_model_number = 68,
    cpu_stepping = 0,
    virtualization = "AMD-V",
    kernel_release = "6.18.50",
    kvm_api_version = 12,
  },
  workload = {
    executable = "init",
    executable_identity = "blake3:0b1de75e526ab568c1948c80b00d2a44f6e8f08d0e2c3672dcdc1cb7e71b15e7",
    observation_profile = Reference paths.observation_profile "blake3:085fcf9b24315208bf91dc8b37f6bb155a88ea9151c0085fdcf7cc1cdee7d50a",
    admitted_observation_profiles = Reference paths.admitted_observation_profiles "blake3:f54391c538db7af2e5ca79a12b66886a0fd861e88a458ae8289f8ac7eba97e7c",
  },
  runs = {
    round_robin = MakeRun "round-robin" BoundaryCount BoundaryCount,
    randomized = MakeRun "randomized" RandomMinimum RandomExclusiveMaximum,
  },
  bounds = {
    identical_repetitions = RepetitionCount,
    history_ticks = HistoryTicks,
    checkpoint_interval_ticks = CheckpointIntervalTicks,
    observation_horizons = [CheckpointIntervalTicks, HistoryTicks],
    outer_deadline_ms = OuterDeadlineMilliseconds,
    teardown_deadline_ms = TeardownDeadlineMilliseconds,
    # Preserve the admitted persisted-debugger exact-local-v1 byte ceilings.
    max_manifest_bytes = 4194304,
    max_recording_bytes = 16777216,
    max_artifact_bytes = 536870912,
    max_snapshot_bytes = 268435456,
    max_total_bytes = 4294967296,
    max_checkpoints = 128,
    max_branches = 128,
    max_artifacts = 256,
    max_decoded_snapshot_bytes = 2147483648,
    max_trace_records_per_vm = JournalRecords,
    max_output_bytes = 16777216,
    max_replay_ticks = JournalRecords,
  },
  scope = "finite same-cohort qualification inputs; not execution, replay, delivery-class qualification, instruction equivalence, cross-host portability, release eligibility, or whole-system correctness",
} | Qualification
