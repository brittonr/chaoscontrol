## Context

`replay-readiness --receipt` already writes a bounded JSON receipt and `replay-readiness-summary` emits a stable one-line summary. GitHub Actions uploads those two files, but an operator must still download JSON or read logs to understand the result.

## Decisions

### 1. Static HTML artifact

**Choice:** Render a self-contained static HTML file from the receipt.
**Rationale:** Static HTML is safe to upload as a CI artifact, easy to open locally, and does not introduce a hosted service or database.
**Alternative:** Extend the live exploration dashboard server. Rejected for this slice because the receipt is an offline CI artifact and should not require running a server.

### 2. Fail-closed receipt validation reuse

**Choice:** Reuse the existing summary script validation before rendering.
**Rationale:** The dashboard must not accept malformed or non-replay-readiness receipts differently from the CI summary line.

### 3. Bounded claims in UI

**Choice:** The dashboard displays the scope string and labels dogfood/evidence curation explicitly.
**Rationale:** This prevents operator-facing UI from overstating Antithesis parity or universal determinism.

## Risks / Trade-offs

- **HTML drift**: Covered by a deterministic self-test that checks success, failure, dogfood, and escaping behavior.
- **CI artifact bloat**: The dashboard is a small static file generated from the same receipt already emitted by the check.
