# Human-authored bounded SMR chain workload profile.
# Runtime proposal, observation, fault, and replay traces remain Rust-owned.
let P = import "profile-primitives.ncl" in
let ObservationMode = std.contract.from_predicate (fun value =>
  value == "lossless" || value == "sampled"
) in
let MaximumCommands = 65536 in
let MaximumCommandBytes = 1048576 in
let MaximumClients = 4096 in
let MaximumConcurrency = 4096 in
let MaximumVirtualProgress = 1000000000 in
let MaximumTraceEvents = 1000000 in
let MaximumFaultActions = 65536 in
let MaximumReplayEvents = 1000000 in
let MaximumEvidenceBytes = 16777216 in
let MaximumReductionAttempts = 65536 in
let Bounds = {
  maximum_commands | P.bounded_positive MaximumCommands,
  maximum_command_bytes | P.bounded_positive MaximumCommandBytes,
  maximum_clients | P.bounded_positive MaximumClients,
  maximum_concurrency | P.bounded_positive MaximumConcurrency,
  maximum_virtual_progress | P.bounded_positive MaximumVirtualProgress,
  maximum_trace_events | P.bounded_positive MaximumTraceEvents,
  maximum_fault_actions | P.bounded_positive MaximumFaultActions,
  maximum_replay_events | P.bounded_positive MaximumReplayEvents,
  maximum_evidence_bytes | P.bounded_positive MaximumEvidenceBytes,
  maximum_reduction_attempts | P.bounded_positive MaximumReductionAttempts,
} in
let Liveness = {
  profile_id | P.identifier,
  required_progress | P.positive_u64,
  virtual_progress_horizon | P.positive_u64,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.smr-workload-profile.v1",
  profile_id | P.identifier,
  initial_state_ref | P.blake3,
  observation_mode | ObservationMode,
  bounds | Bounds,
  liveness | Liveness,
} in
let Coherent = fun value =>
  value.bounds.maximum_concurrency <= value.bounds.maximum_clients
  && value.liveness.required_progress <= value.bounds.maximum_commands
  && value.liveness.virtual_progress_horizon <= value.bounds.maximum_virtual_progress
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
