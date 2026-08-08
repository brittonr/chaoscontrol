## Context

The archived multi-hypervisor campaign and KVM smoke changes established receipt-backed local execution. The next product slice should make this feel like a local control plane rather than a collection of sample receipts.

## Goals / Non-Goals

**Goals:**
- Bounded single-machine multi-hypervisor orchestration.
- Resource placement and per-worker failure attribution.
- Local reproduce/minimize handoff from bug-producing runs.
- Static/local dashboard over receipts and queue state.

**Non-Goals:**
- SaaS, hosted service, remote shared queue, or cross-machine scheduling.
- Universal fleet-scale throughput claims.
- Raw log scraping as evidence.

## Decisions

### 1. Durable local state remains the coordination source

**Choice:** Keep one local queue/state file plus per-worker directories as the source of truth.
**Rationale:** It gives restart behavior and evidence without distributed systems complexity.
**Alternative:** Reuse the networked hosted harness. Rejected because multi-machine coordination is not the target.
**Implementation:** Extend local campaign plan/receipt with worker resource budgets, state transitions, artifact roots, and follow-up jobs.

### 2. Dashboard reads receipts, not logs

**Choice:** Render static/local operator views from validated receipts and state snapshots.
**Rationale:** This preserves evidence integrity and avoids brittle log scraping.
**Implementation:** Add renderer inputs for queue state, workers, run receipts, bug exports, reproduce/minimize receipts, and artifact hashes.

## Risks / Trade-offs

**Overclaim drift** → Add negative tests that reject hosted, cross-machine, and universal throughput wording.
**Resource portability** → Allow CPU pinning/budgets to be explicit optional fields with validation when present.
