# Rust-derived multiprocess evidence shape. Rust revalidates identity bindings.
let P = import "../evidence/profile-primitives.ncl" in
let B3 = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^b3:[0-9a-f]{64}$" value
) in
let Event = {
  process_identity | B3,
  role | P.identifier,
  generation | P.positive_u32,
  tick | P.u64,
  kind | P.closed_enum ["spawned", "exited", "restarted", "killed", "paused", "resumed"],
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.multiprocess-receipt.v1",
  manifest_identity | B3,
  process_identities | Array B3,
  shared_directory_identities | Array B3,
  events | Array Event,
  claim_scope | P.exact_string "declared-processes-only",
} in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked =>
      if P.unique_by (fun identity => identity) checked.process_identities
      && P.unique_by (fun identity => identity) checked.shared_directory_identities
      && std.array.all (fun event =>
        std.array.elem event.process_identity checked.process_identities
      ) checked.events
      then 'Ok checked
      else 'Error {},
    'Error error => 'Error error,
  }
)
