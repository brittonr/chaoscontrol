let Contract = import "../../assertion-summary.ncl" in
let base = (import "../valid/assertions.must-hit.valid.ncl").required_unreachable_failed in
let assertion = std.record.update "true_count" 1 (
  std.record.update "false_count" 0 (std.array.at 0 base.assertions)
) in
(std.record.update "assertions" [assertion] base) | Contract
