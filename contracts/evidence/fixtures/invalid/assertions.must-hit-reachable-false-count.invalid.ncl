let Contract = import "../../assertion-summary.ncl" in
let base = (import "../valid/assertions.must-hit.valid.ncl").optional_reachable_passed in
let assertion = std.record.update "true_count" 0 (
  std.record.update "false_count" 1 (std.array.at 0 base.assertions)
) in
(std.record.update "assertions" [assertion] base) | Contract
