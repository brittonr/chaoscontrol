# Rust-derived bug report evidence shape.
let NonEmptyString = std.contract.from_predicate (fun value => std.is_string value && std.string.length value > 0) in
let FaultSchedule = { faults | Array Dyn } in
{
  bug_id | Number,
  assertion_id | Number,
  assertion_location | NonEmptyString,
  schedule | FaultSchedule,
  tick | Number,
  dedup_key | Number,
  schedule_variant | Dyn,
  scenario_config | Dyn,
  scenario_summary | Dyn,
}
