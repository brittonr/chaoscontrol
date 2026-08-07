let Contract = import "../assertion-summary.ncl" in
let base = import "../fixtures/valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let identity = assertion.identity in
let descriptor = identity.descriptor in
let automatic = std.record.update "logical_key" {
  type = "automatic",
  source_site = "src/main.rs:1:1",
} descriptor in
let stable = std.record.update "logical_key" {
  type = "stable",
  key = "stable-key",
} descriptor in
let with_descriptor = fun selected =>
  std.record.update "identity" (std.record.update "descriptor" selected identity) assertion
in
[
  (std.record.update "assertions" [with_descriptor automatic] base) | Contract,
  (std.record.update "assertions" [with_descriptor stable] base) | Contract,
]
