let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let changed = std.record.update "id" 99 assertion in
(std.record.update "assertions" [changed] base) | Contract
