# Artifact hash contract.
let Sha256Digest = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^sha256:[0-9a-f]{64}$" value
) in
let NonEmptyString = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value > 0
) in
{
  path | NonEmptyString,
  sha256 | Sha256Digest,
}
