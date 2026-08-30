let BugReport = import "../../bug-report.ncl" in
let fixture = import "../../fixtures/valid/bug-report.identity.valid.json" in
(fixture & {
  fallback_scope = {
    process = {
      guest = "guest",
      process = "wal-worker",
    },
    record_sequence = 0,
    record_blake3 = fixture.assertion_identity.fingerprint,
    sink_blake3 = fixture.assertion_identity.catalog_token,
    assertion_fingerprint = fixture.assertion_identity.fingerprint,
  },
}) | BugReport
