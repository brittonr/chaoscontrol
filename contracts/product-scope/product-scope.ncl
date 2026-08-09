# Typed product-scope registry contract.
let P = import "../evidence/profile-primitives.ncl" in
let ScopeState = P.closed_enum ["supported", "experimental", "deferred", "blocked", "non-goal"] in
let EvidenceState = P.closed_enum ["passed", "missing", "blocked", "not-required"] in
let EvidencePrerequisite = {
  id | P.identifier,
  state | EvidenceState,
  source | P.non_empty_string,
} in
let Capability = {
  id | P.identifier,
  name | P.non_empty_string,
  owner | P.identifier,
  state | ScopeState,
  status_label | P.identifier,
  evidence | EvidencePrerequisite,
  boundary | P.non_empty_string,
  documentation_targets | Array P.non_empty_string,
  non_claims | Array P.non_empty_string,
} in
let ChangeIntent = {
  change | P.identifier,
  owner | P.identifier,
  target_state | ScopeState,
  evidence_prerequisite | P.non_empty_string,
  non_claims | Array P.non_empty_string,
} in
let Authority = {
  workspace_manifest | P.non_empty_string,
  test_inventory_command | P.non_empty_string,
  replay_manifest | P.non_empty_string,
} in
let Shape = {
  schema_version | P.exact_value 1,
  authority | Authority,
  capabilities | Array Capability,
  change_intents | Array ChangeIntent,
  prohibited_current_claims | Array P.non_empty_string,
} in
let Coherent = fun value =>
  std.array.length value.capabilities > 0
  && std.array.length value.change_intents > 0
  && P.unique_by (fun capability => capability.id) value.capabilities
  && P.unique_by (fun capability => capability.name) value.capabilities
  && P.unique_by (fun intent => intent.change) value.change_intents
  && std.array.all (fun capability =>
    std.array.length capability.documentation_targets > 0
    && std.array.length capability.non_claims > 0
    && (capability.state != "supported" || capability.evidence.state == "passed")
    && (capability.state != "non-goal" || capability.evidence.state == "not-required")
  ) value.capabilities
  && std.array.all (fun intent => std.array.length intent.non_claims > 0) value.change_intents
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
