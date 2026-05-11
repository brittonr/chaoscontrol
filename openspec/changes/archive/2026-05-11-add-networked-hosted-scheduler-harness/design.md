## Context

The current loopback hosted/shared-state harness proves the receipt boundary, shared queue shape, writer identities, and decision-store records. It still executes inside one process and does not prove independently started workers coordinating through a shared adapter.

## Goals / Non-Goals

**Goals:**
- Exercise the hosted/shared-state scheduler through separate worker identities with explicit worker sessions and heartbeats.
- Persist queue-state and decision-store snapshots with revision/digest evidence.
- Package the harness into replay-readiness checks with fail-closed validators and generated status wording.

**Non-Goals:**
- SaaS hosting, auth, billing, multi-tenant isolation, production UI, or internet-facing service operation.
- Universal fleet-scale throughput or Antithesis product parity claims.
- Replacing existing local KVM/multi-hypervisor proof rails.

## Decisions

### 1. Bounded networked harness before hosted product

**Choice:** Implement a local networked or multi-process harness that starts separate worker sessions against a shared queue/decision-store adapter.
**Rationale:** This proves the coordination seam missing from loopback receipts without prematurely building a production service.
**Alternative:** Build a daemon/SaaS service first; rejected because it widens scope before the receipt and validator semantics are fail-closed.

### 2. Receipt-first validation

**Choice:** Treat worker sessions, leases, queue revisions, decision revisions, and run receipt summaries as first-class receipt fields.
**Rationale:** Operator trust depends on replayable structured evidence, not log scraping.

## Risks / Trade-offs

**False promotion risk** → Keep readiness status bounded and require anti-claims that reject SaaS/product/Antithesis parity.

**Flaky process orchestration** → Keep the first harness deterministic and local, with explicit timeouts and small command plans.
