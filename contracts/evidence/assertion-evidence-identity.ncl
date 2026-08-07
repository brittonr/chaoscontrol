# Rust-derived exact assertion evidence carrier.
let U32Maximum = 4294967295 in
let MaxCanonicalBytes = 2304 in
let UInt32 = std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value >= 0
  && value <= U32Maximum
) in
let PositiveUInt32 = std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value > 0
  && value <= U32Maximum
) in
let Byte = std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value >= 0
  && value <= 255
) in
let Hex64 = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value
) in
let NonEmptyString = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value > 0
) in
let LogicalKeyShape = {
  type | std.contract.from_predicate (fun value =>
    value == "automatic" || value == "stable"
  ),
  source_site | NonEmptyString | optional,
  key | NonEmptyString | optional,
} in
let LogicalKey = std.contract.custom (fun label value =>
  std.contract.check LogicalKeyShape label value
  |> match {
    'Ok checked =>
      let has_source = std.record.has_field "source_site" checked in
      let has_key = std.record.has_field "key" checked in
      let valid =
        if checked.type == "automatic" then has_source && !has_key
        else !has_source && has_key
      in
      if valid then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
) in
let Kind = std.contract.from_predicate (fun value =>
  value == "always"
  || value == "sometimes"
  || value == "reachable"
  || value == "unreachable"
) in
{
  descriptor = {
    identity_version | std.contract.from_predicate (fun value => value == 1),
    namespace | NonEmptyString,
    logical_key | LogicalKey,
    compatibility_id | UInt32 | optional,
    kind | Kind,
    message | NonEmptyString,
    source_file | NonEmptyString,
    source_line | PositiveUInt32,
    source_column | PositiveUInt32,
    guest | NonEmptyString,
    category | NonEmptyString,
  },
  fingerprint | Hex64,
  canonical_descriptor | Array Byte | std.contract.from_predicate (fun value =>
    std.is_array value
    && std.array.length value > 0
    && std.array.length value <= MaxCanonicalBytes
  ),
  catalog_token | Hex64,
}
