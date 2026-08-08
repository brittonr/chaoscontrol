## Context

The existing fleet index and decision receipt narrow operator triage, but they do not represent recurring or multi-run replay campaign planning. A full scheduler/service is intentionally out of scope.

## Decisions

### 1. Local receipt, not a scheduler service

**Choice:** model a bounded local scheduler receipt that records a manual multi-run plan with run IDs, workload names, commands, receipt paths, and decision policy.

**Rationale:** this gives operators a durable, validated planning artifact while avoiding daemon, queue, hosted UI, or cross-machine semantics.

**Rejected:** implementing a scheduler daemon or shared queue now. That would broaden runtime and persistence scope beyond the current readiness slice.

### 2. Rust-owned validation with fail-closed anti-claims

**Choice:** validate duplicate run IDs, positive bounded concurrency, supported schedule modes, non-empty run plans, no raw-log scraping, and anti-claims for not-hosted/not-fleet/not-shared-queue.

**Rationale:** the key risk is overclaiming a sample plan as scheduler/product parity.

## Risks

- Operators may read a scheduled plan as automatic orchestration. Mitigation: status wording and receipt anti-claims explicitly say bounded local manual plan only.
