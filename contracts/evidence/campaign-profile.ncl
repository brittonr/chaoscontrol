# Nickel-authored multi-seed campaign profile. Progress and results stay Rust-owned.
let P = import "profile-primitives.ncl" in
let RunProfile = import "run-config.ncl" in
let Probability = std.contract.from_predicate (fun value =>
  std.is_number value && value >= 0 && value <= 1
) in
let Workers = {
  count | P.bounded_positive 256,
  failure_policy | P.exact_string "fail-campaign",
} in
let Mutation = {
  maximum_tick | P.positive_u64,
  add_probability | Probability,
  remove_probability | Probability,
  shift_probability | Probability,
  replace_probability | Probability,
  schedule_mutation_ratio | Probability,
  havoc_after_stale | P.u64,
  havoc_minimum_mutations | P.positive_u32,
  havoc_maximum_mutations | P.positive_u32,
} in
let Metrics = {
  enabled | Bool,
  output | P.nullable_artifact_reference,
} in
let OutputLayout = {
  root | P.artifact_reference,
  seed_directory_template | P.exact_string "seed_{seed}",
  checkpoint_name | P.exact_string "campaign_progress.json",
} in
let ResourceBounds = {
  maximum_seeds | P.bounded_positive 1024,
  maximum_workers | P.bounded_positive 256,
  maximum_total_memory_mib | P.positive_u64,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.campaign-profile.v1",
  campaign_id | P.identifier,
  seeds | Array P.u64,
  run | RunProfile,
  workers | Workers,
  mutation | Mutation,
  scenario | P.nullable_artifact_reference,
  metrics | Metrics,
  output | OutputLayout,
  bounds | ResourceBounds,
  scope | P.exact_string "pre-run campaign intent; not thread start, VM start, completed seed, fault effect, replay, report, receipt, or accepted evidence",
} in
let ProbabilityTotal = fun mutation =>
  mutation.add_probability
  + mutation.remove_probability
  + mutation.shift_probability
  + mutation.replace_probability
in
let Coherent = fun value =>
  std.array.length value.seeds > 0
  && std.array.length value.seeds <= value.bounds.maximum_seeds
  && P.unique_by (fun seed => seed) value.seeds
  && value.run.seed == std.array.at 0 value.seeds
  && value.workers.count <= value.bounds.maximum_workers
  && value.workers.count <= std.array.length value.seeds
  && value.mutation.havoc_minimum_mutations <= value.mutation.havoc_maximum_mutations
  && ProbabilityTotal value.mutation == 1
  && value.run.topology.num_vms * value.run.topology.memory_mib
    <= value.bounds.maximum_total_memory_mib
  && ((value.metrics.enabled && value.metrics.output != null)
    || (!value.metrics.enabled && value.metrics.output == null))
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
