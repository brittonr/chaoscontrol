let Contract = import "../../bug-report.ncl" in
let Good = import "../valid/bug-report.marker-context.valid.ncl" in
(std.record.update "tick" 0 Good) | Contract
