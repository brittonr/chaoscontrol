let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
(std.record.update "distinct_failing_assertions" null base) | Contract
