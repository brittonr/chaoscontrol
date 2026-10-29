let Contract = import "../../assertion-summary.ncl" in
let base = (import "../valid/assertions.must-hit.valid.ncl").optional_always_failed in
let assertion = std.record.update "verdict" "passed" (std.array.at 0 base.assertions) in
(std.record.update "assertions" [assertion] base) | Contract
