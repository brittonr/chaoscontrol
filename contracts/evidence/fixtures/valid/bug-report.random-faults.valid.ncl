let BugReport = import "../../bug-report.ncl" in
let fixture = import "bug-report.identity.valid.json" in
(fixture & { random_faults = true }) | BugReport
