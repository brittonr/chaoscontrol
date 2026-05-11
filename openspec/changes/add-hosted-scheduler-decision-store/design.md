## Context

Replay-readiness evidence has progressed through static receipts, local decision receipts, local scheduler execution, durable fleet queue receipts, restart-persistent local worker loops, local multi-hypervisor campaign execution, and a real KVM smoke rail. These prove bounded local orchestration, but the readiness status still correctly says there is no hosted service, shared remote queue, cross-machine scheduler, shared decision store, or operator workflow evidence.

## Goals / Non-Goals

**Goals:**

- Prove a bounded hosted/fleet scheduler contract over shared durable queue state that can coordinate multiple machine identities.
- Prove a shared decision-store contract that records operator decisions against replay-readiness receipts and preserves conflict/staleness evidence.
- Link every run and decision to structured receipts and summaries without raw-log scraping.
- Keep the first implementation small enough for deterministic local verification while modeling the cross-machine semantics explicitly.

**Non-Goals:**

- No SaaS control plane, authentication system, billing, or always-on production deployment.
- No claim of universal fleet-scale throughput, arbitrary workload coverage, formal determinism theorem, or Antithesis product parity.
- No promotion from local KVM evidence to hosted support without committed shared-state receipts and fail-closed negative fixtures.

## Decisions

### 1. Shared-state contract before production service

**Choice:** Add a deterministic shared queue/decision-store contract first, with an adapter boundary that can be exercised locally and later backed by a real service.

**Rationale:** The missing product seam is shared ownership and decisions across machine identities. A contract-first adapter proves the semantics without conflating them with deployment, auth, or UI work.

**Alternative:** Build a daemon/UI first. Rejected because it would broaden scope before the shared-state invariants are pinned and testable.

### 2. Machine identity and lease epoch are required evidence

**Choice:** Every hosted/fleet run receipt must bind machine identity, hypervisor worker identity, queue entry, lease ID, lease epoch, command, exit status, receipt path, and stable replay-readiness summary for passed runs.

**Rationale:** Cross-machine scheduling can otherwise hide duplicate ownership, stale leases, or untraceable runs.

### 3. Decisions use conflict-aware records, not local notes

**Choice:** Operator decisions must be stored as conflict-aware records keyed by receipt/run/bug identifiers with writer identity, revision, action, and source receipt links.

**Rationale:** The current local decision receipt is useful, but a hosted/fleet claim needs shared decision semantics and stale-write rejection evidence.

### 4. Overclaim prevention remains generated and fail-closed

**Choice:** Readiness status may promote hosted/fleet triage or scheduler rows only after validators, generated docs, and promotion gates all recognize the shared-state evidence.

**Rationale:** Prior readiness work depends on generated anti-overclaiming guardrails; this change must preserve them.

## Risks / Trade-offs

- **Distributed complexity:** shared-state semantics can become a full scheduler project. Mitigation: bound the first slice to deterministic adapter behavior, two machine identities, and receipt validation.
- **False hosted claim:** a loopback harness is not production hosting. Mitigation: require wording that says bounded shared-state proof unless a real deployed service is later evidenced.
- **Conflict handling gaps:** decision-store writes may appear successful while losing data. Mitigation: add stale revision and duplicate writer negative fixtures before promotion.
