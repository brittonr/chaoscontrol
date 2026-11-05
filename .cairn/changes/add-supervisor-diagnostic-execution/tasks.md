## Phase 1: Implementation

- [x] [serial] Implement bounded diagnostic execution with identity verification, timeout kill, and bounded capture. r[chaoscontrol.guest_diagnostic.supervisor_execution]
- [x] [serial] Cover success, identity mismatch before spawn, and timeout with unit tests. r[chaoscontrol.guest_diagnostic.supervisor_execution]
- [x] [serial] Bind execution outcomes into canonical, bounded receipts. r[chaoscontrol.guest_diagnostic.receipt_binding]
- [x] [serial] Queue admitted requests, record reported receipts, and preserve both across snapshots. r[chaoscontrol.guest_diagnostic.host_dispatch]
- [x] [serial] Add the SDK poll/execute/report client with local-transport and error behavior tests. r[chaoscontrol.guest_diagnostic.guest_client]
- [x] [serial] Record captured diagnostic streams as content-addressed artifacts. r[chaoscontrol.guest_diagnostic.diagnostic_artifacts]
- [x] [serial] Label diagnostic receipts as counterfactual and reject unknown flag bytes. r[chaoscontrol.guest_diagnostic.receipt_counterfactual]
