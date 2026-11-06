# Bounded review shape. Rust recomputes command/wire/output/accounting bindings.
let P = import "../evidence/profile-primitives.ncl" in
let Digest = std.contract.from_predicate (fun value => std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value) in
let SourceDigest = std.contract.from_predicate (fun value => std.is_string value && std.string.is_match "^b3:[0-9a-f]{64}$" value) in
let Wire = std.contract.from_predicate (fun value => value == null ||
  std.is_array value && std.array.length value <= 146
  && std.array.all (fun byte => std.is_number byte && byte >= 0 && byte <= 255 && std.number.floor byte == byte) value
) in
let Shape = {
  schema | P.exact_string "chaoscontrol.observation-receipt.v1",
  binding | {
    session_identity | SourceDigest,
    checkpoint_identity | SourceDigest,
    checkpoint_tick | P.u64,
    selected_tick | P.u64,
    vm_index | P.u32,
  },
  profile_identity | Digest,
  request_digest | Digest,
  deadline_tick | P.positive_u64,
  start_tick | P.u64,
  end_tick | P.u64,
  executed_rounds | P.positive_u64,
  stdout_limit | P.integer_between 1 65536,
  stderr_limit | P.integer_between 1 65536,
  stdout_bytes | P.u32,
  stderr_bytes | P.u32,
  stdout_digest | Digest,
  stderr_digest | Digest,
  diagnostic_receipt | Wire,
  output_digest | Digest,
  incomplete | Bool,
  counterfactual | std.contract.from_predicate (fun value => value == true),
  mutates_guest | Bool,
  certifies_original_history | std.contract.from_predicate (fun value => value == false),
  nonclaim | P.exact_string "local guest-branch observation only; read-only commands may perturb scheduling; not original-history certification, complete capture, causal attribution, source correctness, arbitrary or remote execution, replay promotion, or release eligibility",
} in
std.contract.custom (fun label value =>
  std.contract.check Shape label value |> match {
    'Ok checked =>
      if checked.start_tick == checked.binding.selected_tick
      && checked.binding.selected_tick >= checked.binding.checkpoint_tick
      && checked.end_tick > checked.start_tick
      && checked.end_tick <= checked.deadline_tick
      && checked.deadline_tick - checked.start_tick <= 1000000
      && checked.executed_rounds == checked.end_tick - checked.start_tick
      && checked.stdout_bytes <= checked.stdout_limit
      && checked.stderr_bytes <= checked.stderr_limit
      && (checked.diagnostic_receipt != null || checked.incomplete && checked.end_tick == checked.deadline_tick)
      then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
