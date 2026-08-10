# Typed operator command contract for bounded process execution.
# Rust owns executable observation and bounded-exec invocation.
# r[impl chaoscontrol.typed_operator_commands.plan]
let P = import "profile-primitives.ncl" in
let MaximumExecutableBytes = 1073741824 in
let MaximumTimeoutMs = 86400000 in
let MaximumStreamBytes = 1073741824 in
let MaximumPollIntervalMs = 60000 in
let MaximumTeardownTimeoutMs = 600000 in
let MaximumExitCode = 255 in
let RawBlake3 = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value
) in
let AbsolutePath = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^/[^[:cntrl:]]+$" value
) in
let RelativePath = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.is_match "^[^/][^[:cntrl:]]*$" value
  && !std.string.is_match "(^|/)\\.\\.(/|$)" value
) in
let EnvironmentMode = P.closed_enum ["clear"] in
let StdinMode = P.closed_enum ["null", "bytes"] in
let TerminationScope = P.closed_enum ["process-group"] in
let EnvironmentEntry = {
  name | P.non_empty_string,
  value | String,
} in
let HexBytes = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^([0-9a-f]{2})*$" value
) in
let Stdin = {
  mode | StdinMode,
  hex | HexBytes | optional,
  blake3 | RawBlake3 | optional,
} in
let Limits = {
  timeout_ms | P.bounded_positive MaximumTimeoutMs,
  stdin_max_bytes | P.bounded_positive MaximumStreamBytes,
  stdout_max_bytes | P.bounded_positive MaximumStreamBytes,
  stderr_max_bytes | P.bounded_positive MaximumStreamBytes,
  poll_interval_ms | P.bounded_positive MaximumPollIntervalMs,
  teardown_timeout_ms | P.bounded_positive MaximumTeardownTimeoutMs,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.typed-command-plan.v1",
  mechanism_revision | P.exact_string "29dac88ecded94457572db3fdfaaaab95fa91525",
  executable | {
    path | AbsolutePath,
    blake3 | RawBlake3,
    maximum_bytes | P.bounded_positive MaximumExecutableBytes,
  },
  args | Array String,
  working_directory | RelativePath,
  environment | {
    mode | EnvironmentMode,
    entries | Array EnvironmentEntry,
  },
  stdin | Stdin,
  limits | Limits,
  accepted_exit_codes | Array (P.integer_between 0 MaximumExitCode),
  reject_stdout_truncation | Bool,
  reject_stderr_truncation | Bool,
  termination_scope | TerminationScope,
  evidence_eligible | Bool,
} in
let Coherent = fun value =>
  std.array.length value.accepted_exit_codes > 0
  && P.unique_by (fun code => code) value.accepted_exit_codes
  && P.unique_by (fun entry => entry.name) value.environment.entries
  && ((value.stdin.mode == "null"
      && !(std.record.has_field "hex" value.stdin)
      && !(std.record.has_field "blake3" value.stdin))
    || (value.stdin.mode == "bytes"
      && std.record.has_field "hex" value.stdin
      && std.record.has_field "blake3" value.stdin
      && std.string.length value.stdin.hex / 2 <= value.limits.stdin_max_bytes))
  && value.limits.poll_interval_ms <= value.limits.timeout_ms
  && value.limits.teardown_timeout_ms <= value.limits.timeout_ms
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
