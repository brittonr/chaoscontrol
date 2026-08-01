let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let first = std.array.at 0 base.assertions in
let identity = first.identity in
let descriptor = std.record.update "namespace" "org.example.second" identity.descriptor in
let second_identity = std.record.update "catalog_tokens" ["0000000000000000000000000000000000000000000000000000000000000000"] (
  std.record.update "fingerprint" "0000000000000000000000000000000000000000000000000000000000000000" (
    std.record.update "descriptor" descriptor identity
  )
) in
let second = std.record.update "identity" second_identity first in
(std.record.update "assertions" [first, second] base) | Contract
