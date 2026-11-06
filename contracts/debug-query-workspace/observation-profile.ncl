# Review boundary for a declared command; Rust revalidates at both endpoints.
let P = import "../evidence/profile-primitives.ncl" in
let Relative = fun directory => std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0 && std.string.length value <= 256
  && (directory && value == "." ||
    !(std.string.is_match "(^/|\\\\|[[:cntrl:]]|(^|/)\\.\\.?(/|$)|//|/$)" value))
) in
let Argument = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value > 0 && std.string.length value <= 1024
  && !(std.string.is_match "(^/|\\\\|[[:cntrl:]]|(^|/)\\.\\.(/|$))" value)
) in
let Digest = std.contract.from_predicate (fun value =>
  std.is_array value && std.array.length value == 32
  && std.array.all (fun byte => std.is_number byte && byte >= 0 && byte <= 255 && std.number.floor byte == byte) value
  && std.array.any (fun byte => byte != 0) value
) in
let Environment = std.contract.from_predicate (fun value =>
  std.is_record value && std.array.length (std.record.fields value) <= 16
  && std.array.all (fun entry =>
    std.string.is_match "^[A-Z0-9_]{1,128}$" entry.field
    && !(std.string.is_match "^(LD_|DYLD_|PATH$|ENV$|BASH_ENV$|SHELLOPTS$)" entry.field)
    && std.is_string entry.value && std.string.length entry.value <= 1024
    && !(std.string.is_match "[[:cntrl:]]" entry.value)
  ) (std.record.to_array value)
  && std.array.fold_left (fun count entry => count + std.string.length entry.field + std.string.length entry.value) 0 (std.record.to_array value) <= 2048
) in
let Shape = {
  executable | Relative false,
  executable_blake3 | Digest,
  argv | Array Argument,
  environment | Environment,
  working_directory | Relative true,
  deadline_tick | P.positive_u64,
  stdout_limit | P.integer_between 1 65536,
  stderr_limit | P.integer_between 1 65536,
  mutates_guest | Bool,
} in
std.contract.custom (fun label value =>
  std.contract.check Shape label value |> match {
    'Ok checked =>
      if std.array.length checked.argv <= 16
      && std.array.fold_left (fun count arg => count + std.string.length arg) 0 checked.argv <= 4096
      then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
