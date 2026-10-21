# Apply this function to measured, reviewed bindings after selecting the cohort.
# No subject, command, registry, selection, policy, dependency, or tool identity
# is supplied by default. This file never creates execution-result rows.
let Registry = import "registry.ncl" in
let Digest = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value
) in
let Tool = {
  executable | String,
  executable_hash | Digest,
  tool_cohort_hash | Digest,
  environment | { _ : String },
} in
let Bindings = {
  subject_hash | Digest,
  source_projection_hash | Digest,
  policy_hash | Digest,
  registry_hash | Digest,
  selection_hash | Digest,
  command_hashes | { _ : Digest },
  dependency_hashes | Array Digest
    | std.contract.from_predicate (fun values => std.array.length values > 0),
  cargo | Tool,
  nix | Tool,
} in
fun raw =>
let observed = raw | Bindings in
let Commands = std.array.map (fun check => {
  check_id = check.check_id,
  command = [observed."%{check.tool}".executable] @ check.args,
  command_hash = observed.command_hashes."%{check.check_id}",
  prerequisites = [],
}) Registry.checks in
{
  contract = {
    schema = "cairn.agent-completion.v2",
    repository_id = Registry.repository_id,
    registry_id = Registry.registry_id,
    registry_hash = observed.registry_hash,
    selection_hash = observed.selection_hash,
    policy_hash = observed.policy_hash,
    final_subject_hash = observed.subject_hash,
    required_checks = Commands,
  },
  profiles = std.array.map (fun check => {
    schema = "cairn.check-profile.v1",
    check_id = check.check_id,
    kind = check.kind,
    executable = observed."%{check.tool}".executable,
    executable_hash = observed."%{check.tool}".executable_hash,
    tool_cohort_hash = observed."%{check.tool}".tool_cohort_hash,
    target = "x86_64-unknown-linux-gnu",
    features = [],
    environment = observed."%{check.tool}".environment,
    working_root = ".",
    bounds = {
      id = "%{check.check_id}-bounds-v1",
      deadline_ms = check.deadline_ms,
      teardown_ms = 60000,
      stdout_bytes = check.stdout_bytes,
      stderr_bytes = check.stderr_bytes,
    },
  }) Registry.checks,
  applicability = {
    subject_hash = observed.subject_hash,
    policy_hash = observed.policy_hash,
    dependency_hashes = observed.dependency_hashes,
    source_projection_hash = observed.source_projection_hash,
  },
}
