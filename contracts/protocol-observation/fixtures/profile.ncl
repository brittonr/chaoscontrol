# Synthetic identities for structural fixtures, not third-party provenance.
let first_vm = 7 in
let second_vm = 9 in
let record_limit = 8 in
let projection_limit = 512 in
let total_bytes = 4096 in
let participants = 2 in
let boundaries = 4 in
let backlog = 16 in
let work = 32 in
let diagnostics = 4 in
{
  schema : String = "chaoscontrol.protocol-observation-profile.v1",
  execution_ref : String = "execution:7777777777777777777777777777777777777777777777777777777777777777",
  protocol_ref : String = "protocol:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  projection_schema_ref : String = "projection-schema:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  logical_boundary_schema_ref : String = "logical-boundary-schema:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
  cohort_ref : String = "cohort:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  producers = [
    {
      producer_ref : String = "producer:1111111111111111111111111111111111111111111111111111111111111111",
      participant_ref : String = "participant:3333333333333333333333333333333333333333333333333333333333333333",
      process_ref : String = "process:5555555555555555555555555555555555555555555555555555555555555555",
      vm_id : Number = first_vm,
      admitted_generation : Number = 0,
    },
    {
      producer_ref : String = "producer:2222222222222222222222222222222222222222222222222222222222222222",
      participant_ref : String = "participant:4444444444444444444444444444444444444444444444444444444444444444",
      process_ref : String = "process:6666666666666666666666666666666666666666666666666666666666666666",
      vm_id : Number = second_vm,
      admitted_generation : Number = 0,
    },
  ],
  required_participants : Array String = [
    "participant:3333333333333333333333333333333333333333333333333333333333333333",
    "participant:4444444444444444444444444444444444444444444444444444444444444444",
  ],
  completion_rule : String = "all-required-participants",
  oracle = {
    adapter_ref : String = "oracle-adapter:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
    authority : String = "consumer-independent",
    requires_complete_cohort : Bool = true,
  },
  novelty_selectors : Array String = ["projection-ref", "logical-boundary-ref", "transition-class"],
  marker_policy : String = "optional-declared",
  bounds = {
    max_records_per_producer : Number = record_limit,
    max_projection_bytes : Number = projection_limit,
    max_total_projection_bytes : Number = total_bytes,
    max_participants : Number = participants,
    max_logical_boundaries : Number = boundaries,
    max_cohort_backlog : Number = backlog,
    max_oracle_work_items : Number = work,
    max_diagnostic_refs : Number = diagnostics,
  },
  non_claims : Array String = [
    "does not establish production readiness",
    "does not establish protocol semantics",
    "does not establish release eligibility",
    "does not establish universal correctness",
    "does not infer a cross-producer total order",
    "does not synchronize wall clocks",
  ],
}
