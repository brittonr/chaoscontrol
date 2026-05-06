# Rust-derived assertion summary contract.
let Verdict = std.contract.from_predicate (fun value => value == "passed" || value == "failed" || value == "unexercised") in
let NonEmptyString = std.contract.from_predicate (fun value => std.is_string value && std.string.length value > 0) in
let Assertion = {
  id | Number,
  message | NonEmptyString,
  kind | NonEmptyString,
  guest | NonEmptyString,
  category | NonEmptyString,
  verdict | Verdict,
  hit_count | Number,
  true_count | Number,
  false_count | Number,
  last_failure_details | String | optional,
} in
Array Assertion
