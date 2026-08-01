let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let changed = std.record.update "message" "forged top-level message" assertion in
(std.record.update "assertions" [changed] base) | Contract
