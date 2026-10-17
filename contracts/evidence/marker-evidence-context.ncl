# Rust-owned MarkerEvidenceContext review shape. This never grants replay authority.
let P = import "profile-primitives.ncl" in
let MaxOwnerBytes = 128 in
let MaxReferenceBytes = 256 in
let BoundedText = fun maximum => std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value > 0
  && std.string.length value <= maximum
  && !(std.string.is_match "\\p{Cc}" value)
) in
let MarkerIdentity = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^b3:[0-9a-f]{64}$" value
) in
let NullableIdentity = std.contract.custom (fun label value =>
  if value == null then 'Ok value else std.contract.check MarkerIdentity label value
) in
let NullableReference = std.contract.custom (fun label value =>
  if value == null then 'Ok value
  else std.contract.check (BoundedText MaxReferenceBytes) label value
) in
{
  marker_identity | MarkerIdentity,
  owner | BoundedText MaxOwnerBytes,
  state_ref | NullableIdentity | default = null,
  logical_position_ref | NullableReference | default = null,
  run_id | P.u32,
  observed_tick | P.u64,
}
