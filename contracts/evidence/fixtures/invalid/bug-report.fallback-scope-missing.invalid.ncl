let BugReport = import "../../bug-report.ncl" in
let fixture = import "../../fixtures/valid/bug-report.identity.valid.json" in
(fixture & {
  assertion_identity.descriptor.category = "fallback-process",
}) | BugReport
