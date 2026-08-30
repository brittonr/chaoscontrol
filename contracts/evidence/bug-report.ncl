# Rust-derived bug report evidence shape. Legacy ID-only reports are diagnostic-only.
let NonEmptyString = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value > 0
) in
let FaultSchedule = { faults | Array Dyn } in
let Fingerprint = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value
) in
let FallbackProcess = {
  guest | NonEmptyString,
  process | NonEmptyString,
} in
let FallbackScope = {
  process | FallbackProcess,
  record_sequence | Number,
  record_blake3 | Fingerprint,
  sink_blake3 | Fingerprint,
  assertion_fingerprint | Fingerprint,
} in
let AssertionEvidenceIdentity = import "assertion-evidence-identity.ncl" in
let BugReportShape = {
  bug_id | Number,
  assertion_id | Number,
  assertion_identity | AssertionEvidenceIdentity | optional,
  fallback_scope | FallbackScope | optional,
  assertion_location | NonEmptyString,
  schedule | FaultSchedule,
  tick | Number,
  replay_parent_depth | Number,
  replay_parent_snapshot_ref | Dyn | default = null,
  dedup_key | Number,
  schedule_variant | Dyn,
  scenario_config | Dyn,
  scenario_summary | Dyn,
} in
let AliasBinding = fun value =>
  if !std.record.has_field "assertion_identity" value then true
  else if std.record.has_field "compatibility_id" value.assertion_identity.descriptor then
    value.assertion_identity.descriptor.compatibility_id == value.assertion_id
  else
    value.assertion_id == 0
in
let FallbackBinding = fun value =>
  let has_identity = std.record.has_field "assertion_identity" value in
  let has_scope = std.record.has_field "fallback_scope" value in
  if !has_identity then !has_scope
  else if value.assertion_identity.descriptor.category == "fallback-process" then
    has_scope
    && value.fallback_scope.assertion_fingerprint == value.assertion_identity.fingerprint
  else
    !has_scope
in
std.contract.custom (fun label value =>
  std.contract.check BugReportShape label value
  |> match {
    'Ok checked =>
      if AliasBinding checked && FallbackBinding checked
      then 'Ok checked
      else 'Error {},
    'Error error => 'Error error,
  }
)
