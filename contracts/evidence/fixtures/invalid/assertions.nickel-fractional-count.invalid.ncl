let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.record.update "hit_count" 1.5 (std.array.at 0 base.assertions) in
(std.record.update "assertions" [assertion] base) | Contract
