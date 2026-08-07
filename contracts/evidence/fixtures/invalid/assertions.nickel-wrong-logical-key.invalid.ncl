let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
let assertion = std.array.at 0 base.assertions in
let identity = assertion.identity in
let descriptor = identity.descriptor in
let logical_key = std.record.update "id" 7 descriptor.logical_key in
let changed_descriptor = std.record.update "logical_key" logical_key descriptor in
let changed = std.record.update "identity" (std.record.update "descriptor" changed_descriptor identity) assertion in
(std.record.update "assertions" [changed] base) | Contract
