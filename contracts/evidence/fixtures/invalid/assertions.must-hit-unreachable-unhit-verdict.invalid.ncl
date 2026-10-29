let Contract = import "../../assertion-summary.ncl" in
let base = (import "../valid/assertions.must-hit.valid.ncl").required_unreachable_unhit in
let assertion = std.record.update "verdict" "failed" (std.array.at 0 base.assertions) in
(std.record.update "assertions" [assertion] base) | Contract
