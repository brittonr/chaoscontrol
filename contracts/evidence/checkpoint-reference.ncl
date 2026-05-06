# Checkpoint reference contract; payload remains Rust-owned.
let NonEmptyString = std.contract.from_predicate (fun value => std.is_string value && std.string.length value > 0) in
let Sha256Digest = std.contract.from_predicate (fun value => std.is_string value && std.string.is_match "^sha256:[0-9a-f]{64}$" value) in
{
  path | NonEmptyString,
  digest | Sha256Digest,
  kernel_path | NonEmptyString,
  initrd_path | NonEmptyString,
  seed | Number,
}
