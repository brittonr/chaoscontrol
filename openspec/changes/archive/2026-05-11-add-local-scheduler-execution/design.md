## Context

The previous local scheduler receipt intentionally avoided daemon or queue semantics. This slice turns the local receipt into a real sequential executor that records command exit status and validates generated replay-readiness receipts.

## Decisions

### 1. Sequential only

**Choice:** execute runs with `concurrency=1` and reject higher concurrency in execution receipts.

**Rationale:** this proves local orchestration without implying scheduler service, queueing, worker leases, or fleet concurrency.

**Rejected:** parallel or background worker execution. That belongs to a future hosted scheduler/fleet change.

### 2. Receipt summaries, not raw log scraping

**Choice:** each successful run must link a receipt path and store the stable replay-readiness summary line.

**Rationale:** replay readiness evidence is already receipt-backed; raw logs remain debug artifacts and must not become the operator evidence API.

## Risks

- Shell command execution is intentionally local/operator-controlled. Mitigation: the receipt records command strings and exit codes, but support status remains bounded local execution only.
