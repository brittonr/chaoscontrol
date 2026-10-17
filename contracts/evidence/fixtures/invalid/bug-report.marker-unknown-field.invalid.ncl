let Contract = import "../../bug-report.ncl" in
let Good = import "../valid/bug-report.marker-context.valid.ncl" in
(std.record.update "marker_context" (Good.marker_context & {
  invented_authority = true,
}) Good) | Contract
