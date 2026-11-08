# Finite qualification intent only; no field asserts that a run passed.
let P = import "profile-primitives.ncl" in
let Run = import "run-config.ncl" in
let Bounds = {
  identical_repetitions | P.exact_value 2,
  history_ticks | P.positive_u64,
  checkpoint_interval_ticks | P.positive_u64,
  observation_horizons | Array P.positive_u64,
  outer_deadline_ms | P.positive_u64,
  teardown_deadline_ms | P.positive_u64,
  max_manifest_bytes | P.positive_u64,
  max_recording_bytes | P.positive_u64,
  max_artifact_bytes | P.positive_u64,
  max_snapshot_bytes | P.positive_u64,
  max_total_bytes | P.positive_u64,
  max_checkpoints | P.positive_u64,
  max_branches | P.positive_u64,
  max_artifacts | P.positive_u64,
  max_decoded_snapshot_bytes | P.positive_u64,
  max_trace_records_per_vm | P.positive_u64,
  max_output_bytes | P.positive_u64,
  max_replay_ticks | P.positive_u64,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.kvm-debug-boundary-qualification.v1",
  qualification_id | P.identifier,
  status | P.exact_string "qualification-inputs-only",
  host | {
    architecture | P.exact_string "x86_64",
    cpu_vendor | P.exact_string "AuthenticAMD",
    cpu_model | P.exact_string "AMD Ryzen 9 9950X3D 16-Core Processor",
    cpu_family | P.exact_value 26,
    cpu_model_number | P.exact_value 68,
    cpu_stepping | P.exact_value 0,
    virtualization | P.exact_string "AMD-V",
    kernel_release | P.exact_string "6.18.50",
    kvm_api_version | P.exact_value 12,
  },
  workload | {
    executable | P.exact_string "init",
    executable_identity | P.exact_string "blake3:0b1de75e526ab568c1948c80b00d2a44f6e8f08d0e2c3672dcdc1cb7e71b15e7",
    observation_profile | P.artifact_reference,
    admitted_observation_profiles | P.artifact_reference,
  },
  runs | { round_robin | Run, randomized | Run },
  bounds | Bounds,
  scope | P.exact_string "finite same-cohort qualification inputs; not execution, replay, delivery-class qualification, instruction equivalence, cross-host portability, release eligibility, or whole-system correctness",
} in
let CoarseRun = fun run =>
  run.progress_mode == "KvmDebugBoundary"
  && run.seed == 42
  && run.topology.num_vms == 1
  && run.topology.num_vcpus == 2
  && run.topology.memory_mib == 128
  && run.topology.scheduling.journal_limit == 65536
  && run.topology.scheduling.entry_watchdog_microseconds == 250000
  && run.exploration.bootstrap_budget == 100000
in
let Coherent = fun value =>
  let rr = value.runs.round_robin in
  let random = value.runs.randomized in
  let bounds = value.bounds in
  CoarseRun rr && CoarseRun random
  && rr.artifacts == random.artifacts
  && rr.guest_cmdline == random.guest_cmdline
  && rr.topology.scheduling.strategy == "round-robin"
  && rr.exploration.progress_budget.KvmBoundaries.count == 64
  && random.topology.scheduling.strategy == "randomized"
  && random.exploration.progress_budget.KvmBoundaries.count == 64
  && random.topology.scheduling.minimum_budget.KvmBoundaries.count == 32
  && random.topology.scheduling.maximum_budget.KvmBoundaries.count == 128
  && bounds.identical_repetitions >= 2
  && bounds.history_ticks == 20
  && bounds.checkpoint_interval_ticks == 10
  && bounds.observation_horizons == [10, 20]
  && rr.artifacts.kernel.identity == "blake3:f2abae8a52c83c975ed15de504535445e2ac159cc0198f8028867b0c18cdf67c"
  && rr.artifacts.initrd != null
  && rr.artifacts.initrd.identity == "blake3:bdab04c42d49ae3f75468e1c2e6408c4829bcd5c17d8792ac2e090eeeeb972fa"
  && value.workload.observation_profile.identity == "blake3:085fcf9b24315208bf91dc8b37f6bb155a88ea9151c0085fdcf7cc1cdee7d50a"
  && value.workload.admitted_observation_profiles.identity == "blake3:f54391c538db7af2e5ca79a12b66886a0fd861e88a458ae8289f8ac7eba97e7c"
  && bounds.outer_deadline_ms == 3600000
  && bounds.teardown_deadline_ms < bounds.outer_deadline_ms
  && bounds.checkpoint_interval_ticks <= bounds.history_ticks
  && bounds.max_replay_ticks >= bounds.history_ticks
  && bounds.max_trace_records_per_vm >= rr.topology.scheduling.journal_limit
  && bounds.max_manifest_bytes <= bounds.max_total_bytes
  && bounds.max_recording_bytes <= bounds.max_total_bytes
  && bounds.max_artifact_bytes <= bounds.max_total_bytes
  && bounds.max_snapshot_bytes <= bounds.max_artifact_bytes
  && bounds.max_output_bytes <= bounds.max_total_bytes
  && bounds.max_checkpoints >= 2
  && bounds.max_artifacts >= bounds.max_checkpoints
  && bounds.teardown_deadline_ms == 10000
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
