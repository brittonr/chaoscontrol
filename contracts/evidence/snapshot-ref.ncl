# Rust-derived replay parent snapshot artifact reference.
let NonEmptyString = std.contract.from_predicate (fun value => std.is_string value && std.string.length value > 0) in
{
  store | NonEmptyString,
  digest | NonEmptyString,
  codec | NonEmptyString,
  schema_version | Number,
  path | NonEmptyString,
}
