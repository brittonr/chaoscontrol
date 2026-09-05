# Opaque protocol-observation profile. Rust owns execution and runtime records.
let P = import "../evidence/profile-primitives.ncl" in
let Limits = {
  participants = 256,
  records = 4096,
  projection_bytes = 2048,
  total_projection_bytes = 8388608,
  boundaries = 256,
  oracle_work = 4096,
  diagnostics = 32,
  non_claims = 32,
  claim_bytes = 128,
  vm_id = 4294967295,
  generation = 18446744073709551615,
} in
let Reference = fun prefix => std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^%{prefix}:[0-9a-f]{64}$" value
) in
let NonClaims = [
  "does not establish production readiness",
  "does not establish protocol semantics",
  "does not establish release eligibility",
  "does not establish universal correctness",
  "does not infer a cross-producer total order",
  "does not synchronize wall clocks",
] in
let Producer = {
  producer_ref | Reference "producer",
  participant_ref | Reference "participant",
  process_ref | Reference "process",
  vm_id | P.integer_between 0 Limits.vm_id,
  admitted_generation | P.integer_between 0 Limits.generation,
} in
let Bounds = {
  max_records_per_producer | P.integer_between 1 Limits.records,
  max_projection_bytes | P.integer_between 1 Limits.projection_bytes,
  max_total_projection_bytes | P.integer_between 1 Limits.total_projection_bytes,
  max_participants | P.integer_between 1 Limits.participants,
  max_logical_boundaries | P.integer_between 1 Limits.boundaries,
  max_cohort_backlog | P.integer_between 1 Limits.records,
  max_oracle_work_items | P.integer_between 1 Limits.oracle_work,
  max_diagnostic_refs | P.integer_between 1 Limits.diagnostics,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.protocol-observation-profile.v1",
  execution_ref | Reference "execution",
  protocol_ref | Reference "protocol",
  projection_schema_ref | Reference "projection-schema",
  logical_boundary_schema_ref | Reference "logical-boundary-schema",
  cohort_ref | Reference "cohort",
  producers | Array Producer,
  required_participants | Array (Reference "participant"),
  completion_rule | P.exact_string "all-required-participants",
  oracle | {
    adapter_ref | Reference "oracle-adapter",
    authority | P.exact_string "consumer-independent",
    requires_complete_cohort | P.exact_value true,
  },
  novelty_selectors | Array (P.closed_enum ["projection-ref", "logical-boundary-ref", "transition-class"]),
  marker_policy | P.closed_enum ["denied", "optional-declared"],
  bounds | Bounds,
  non_claims | Array String,
} in
let Coherent = fun profile =>
  let participants = std.array.length profile.required_participants in
  participants > 0 && participants <= profile.bounds.max_participants
  && participants <= profile.bounds.max_cohort_backlog
  && participants <= profile.bounds.max_oracle_work_items
  && std.array.length profile.producers == participants
  && profile.bounds.max_total_projection_bytes >= profile.bounds.max_projection_bytes
  && P.unique_by (fun producer => producer.producer_ref) profile.producers
  && P.unique_by (fun producer => producer.participant_ref) profile.producers
  && P.unique_by (fun producer => "%{std.to_string producer.vm_id}:%{producer.process_ref}") profile.producers
  && P.unique_by (fun participant => participant) profile.required_participants
  && std.array.all (fun participant =>
    std.array.any (fun producer => producer.participant_ref == participant) profile.producers
  ) profile.required_participants
  && std.array.length profile.novelty_selectors > 0
  && P.unique_by (fun selector => selector) profile.novelty_selectors
  && std.array.length profile.non_claims <= Limits.non_claims
  && std.array.all (fun claim => std.array.elem claim profile.non_claims) NonClaims
  && P.unique_by (fun claim => claim) profile.non_claims
  && std.array.all (fun claim => std.string.length claim <= Limits.claim_bytes) profile.non_claims
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
