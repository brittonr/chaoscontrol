let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let strict = std.array.at 0 base.assertions in
let legacy = std.record.update "message" "legacy\nforgery" (
  std.record.remove "identity" strict
) in
(std.record.update "catalog_status" "legacy-ambiguous" (
  std.record.update "collision_safe_evidence" false (
    std.record.update "assertions" [legacy] base
  )
)) | Contract
