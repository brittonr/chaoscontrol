let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let identity = assertion.identity in
let descriptor = std.record.update "message" "invalid\nmessage" identity.descriptor in
let changed_identity = std.record.update "descriptor" descriptor identity in
let changed = std.record.update "message" "invalid\nmessage" (
  std.record.update "identity" changed_identity assertion
) in
(std.record.update "assertions" [changed] base) | Contract
