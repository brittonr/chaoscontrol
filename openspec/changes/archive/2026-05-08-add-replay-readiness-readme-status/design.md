## Context

The existing `replay-readiness-summary` script validates a receipt and emits a stable one-line status. The new dashboard improves artifact review, but README readers still need a compact status without downloading CI artifacts.

## Decisions

### 1. Marker-bounded README update

**Choice:** Replace only content between `<!-- replay-readiness-status:start -->` and `<!-- replay-readiness-status:end -->`.

**Rationale:** The status is generated, reviewable, and low-risk; marker bounds avoid broad README rewrites.

**Alternative:** Hand-write the line. Rejected because it can drift from receipt semantics.

### 2. Reuse fail-closed summary validation

**Choice:** Import `summarize-replay-readiness-receipt.py` and render the same summary line in Markdown.

**Rationale:** README status must not diverge from CI/dashboard receipt validation.

### 3. Explicit bounded claim language

**Choice:** The generated block includes the summary line plus a sentence that it is a bounded committed-evidence readiness signal, not universal determinism or hosted Antithesis parity.

**Rationale:** The front-page status should improve visibility without overclaiming.

## Risks / Trade-offs

- **README churn**: Only the marker-bounded block is generated.
- **Stale status**: Documentation tells operators to refresh from a current receipt; CI artifacts remain the source for run-specific evidence.
