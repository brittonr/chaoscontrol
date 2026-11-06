let Contract = import "../../assertion-summary.ncl" in
let base = import "../valid/assertions.identity.valid.json" in
(std.record.update "liveness_gaps" null base) | Contract
