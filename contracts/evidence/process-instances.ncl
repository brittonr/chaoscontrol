# Rust-owned observed process-instance set; not whole-guest or replay authority.
let P = import "profile-primitives.ncl" in
let MaximumInstances = 32 in
let MaximumTokenBytes = 96 in
let ValidToken = fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= MaximumTokenBytes
  && std.string.is_match "^[A-Za-z0-9_.:-]+$" value
in
std.contract.from_predicate (fun value =>
  std.is_array value
  && std.array.length value <= MaximumInstances
  && std.array.all ValidToken value
  && P.unique_by (fun token => token) value
)
