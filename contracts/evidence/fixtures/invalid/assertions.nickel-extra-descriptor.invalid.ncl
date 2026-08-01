let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let identity = assertion.identity in
let descriptor = std.record.update "unexpected" true identity.descriptor in
let changed = std.record.update "identity" (std.record.update "descriptor" descriptor identity) assertion in
(std.record.update "assertions" [changed] base) | Contract
