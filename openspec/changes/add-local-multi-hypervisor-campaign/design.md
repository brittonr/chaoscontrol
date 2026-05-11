## Context

The existing replay-readiness operator surface has progressed from static readiness receipts to local decision receipts, local scheduler execution, bounded fleet queue/worker receipts, a bounded worker loop, and restart-persistent queue state. The remaining question is whether ChaosControl can exercise fleet-style execution by launching multiple of its own hypervisors rather than standing up a hosted scheduler service.

## Goals / Non-Goals

**Goals:**

- Prove one bounded local campaign can spawn multiple ChaosControl hypervisor workers from one queue-backed plan.
- Reuse the existing durable queue, lease, worker, per-run receipt, and restart-recovery evidence model.
- Emit operator-reviewable receipts that show which hypervisor handled each run and how queue state advanced.
- Keep the implementation small enough to verify locally before any real multi-machine scheduler work.

**Non-Goals:**

- No hosted service, SaaS control plane, shared remote database, or cross-machine distributed scheduler.
- No claim of universal fleet-scale throughput or Antithesis product parity.
- No broad VM matrix campaign beyond the smallest smoke needed to prove the runner contract.

## Decisions

### 1. Local multi-hypervisor runner before hosted scheduler

**Choice:** Add a local campaign runner that starts bounded ChaosControl hypervisor workers against one shared local queue/state file.

**Rationale:** This proves the important product-shaped seam—multiple hypervisors consuming one plan—without conflating it with distributed coordination, remote scheduling, or a hosted UI.

**Alternative:** Build a daemon or multi-host scheduler first. Rejected because it broadens scope before the existing local queue/receipt model proves actual hypervisor-backed concurrency.

### 2. Receipt-backed coordination is the source of truth

**Choice:** Treat the queue state and fleet receipt as the evidence source of truth; logs are debug aids only.

**Rationale:** Prior readiness work consistently avoids raw-log scraping and prevents overclaiming by requiring structured receipts.

**Implementation:** Each worker run records `campaign_id`, `hypervisor_worker_id`, queue entry, lease ID, command/run metadata, receipt path, exit status, and replay-readiness summary when successful. The queue state is persisted after each lease transition/run completion.

### 3. Fail closed on ambiguous execution

**Choice:** Validators must reject duplicate leases, missing hypervisor worker links, missing queue-state persistence, successful runs without receipt summaries, and receipts whose anti-claims imply hosted/shared multi-machine scheduling.

**Rationale:** Multi-hypervisor execution can otherwise look like fleet support while hiding duplicate/lost work or unsupported product claims.

## Risks / Trade-offs

- **KVM availability:** local hypervisor smoke may be unavailable in some environments. Mitigation: keep pure receipt/plan tests mandatory and make any KVM smoke a bounded packaged rail with clear skip/blocker semantics.
- **Concurrency flake:** parallel hypervisors can expose timing/resource pressure. Mitigation: bound worker count, timeouts, and queue size; require deterministic receipt validation independent of raw logs.
- **Overclaiming:** local multi-hypervisor evidence is not multi-machine hosted scheduling. Mitigation: generated status and anti-claims must preserve that distinction.
