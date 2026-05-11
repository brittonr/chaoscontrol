## Context

Scheduler evidence now includes a local sequential executor. The next smallest product-facing seam is a durable fleet scheduler receipt that binds queue state, leases, workers, and run receipts without requiring a daemon deployment in this slice.

## Decisions

### 1. Receipt-backed bounded fleet proof

**Choice:** add a `replay-readiness-fleet-scheduler-receipt` JSON model instead of a long-running service.

**Rationale:** operators get durable queue/worker/lease/run evidence that can be validated in CI and Nix. A live service can later emit the same receipt shape.

**Rejected:** launching a daemon or cross-machine runner in this slice. That would require broader lifecycle, networking, recovery, and ops decisions.

### 2. Fail closed on evidence gaps

**Choice:** validation rejects raw-log scraping, missing workers, duplicate IDs, missing queue/run links, missing receipt summaries for passed runs, and missing linked decision receipts.

**Rationale:** the hosted/fleet claim must be grounded in structured evidence, not logs or narrative status.

## Risks

- A static receipt can be mistaken for a live service. Mitigation: generated status and scope text call it bounded receipt-backed evidence and keep live always-on service as promotion evidence.
