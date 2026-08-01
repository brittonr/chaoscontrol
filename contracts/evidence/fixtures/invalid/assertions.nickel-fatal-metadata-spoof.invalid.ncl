let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let spoofed = std.record.update "message" "spoofed top-level message" assertion in
(std.record.update "catalog_status" "fatal-conflict" (
  std.record.update "collision_safe_evidence" false (
    std.record.update "assertions" [spoofed] base
  )
)) | Contract
