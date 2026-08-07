let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let identity = assertion.identity in
let logical_key = {
  type = "legacy_u32",
  id = 99,
} in
let descriptor = std.record.update "logical_key" logical_key identity.descriptor in
let changed = std.record.update "identity" (std.record.update "descriptor" descriptor identity) assertion in
(std.record.update "catalog_status" "fatal-conflict" (
  std.record.update "collision_safe_evidence" false (
    std.record.update "assertions" [changed] base
  )
)) | Contract
