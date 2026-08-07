# Nickel-authored in-process simulator profile.
let P = import "profile-primitives.ncl" in
let Workload = {
  name | P.identifier,
  adapter_version | P.identifier,
  scenario_id | P.identifier,
} in
let Scheduler = {
  name | P.exact_string "round-robin-v1",
  max_steps | P.positive_u64,
} in
let VirtualClock = {
  start_tick | P.u64,
  tick_quantum | P.positive_u64,
} in
let Rng = {
  algorithm | P.exact_string "xorshift64-v1",
  seed_derivation | P.exact_string "config.seed",
} in
let SimulatedProfile = {
  profile_id | P.identifier,
  simulated | P.exact_value true,
} in
let FaultScheduleReference = {
  schedule_id | P.identifier,
  digest | P.sha256,
} in
let ArtifactBinding = {
  name | P.identifier,
  digest | P.sha256,
} in
let Shape = {
  schema_version | P.exact_value 1,
  run_id | P.identifier,
  seed | P.u64,
  workload | Workload,
  scheduler | Scheduler,
  virtual_clock | VirtualClock,
  rng | Rng,
  network | SimulatedProfile,
  disk | SimulatedProfile,
  fault_schedule | FaultScheduleReference,
  artifacts | Array ArtifactBinding,
  scope | P.exact_string "adapter-based in-process simulator evidence; not VM replay proof, not arbitrary binary support, not full FoundationDB parity",
} in
let Coherent = fun value =>
  std.array.length value.artifacts > 0
  && std.array.length value.artifacts <= 64
  && P.unique_by (fun artifact => artifact.name) value.artifacts
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
