let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let changed = std.record.update "identity" null assertion in
(std.record.update "catalog_status" "legacy-ambiguous" (
  std.record.update "collision_safe_evidence" false (
    std.record.update "assertions" [changed] base
  )
)) | Contract
