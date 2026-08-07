let Contract = import "../assertion-summary.ncl" in
let base = import "../fixtures/valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let identity = assertion.identity in
let descriptor = std.record.remove "compatibility_id" identity.descriptor in
let without_alias = std.record.update "id" 0 (
  std.record.update "identity" (std.record.update "descriptor" descriptor identity) assertion
) in
(std.record.update "assertions" [without_alias] base) | Contract
