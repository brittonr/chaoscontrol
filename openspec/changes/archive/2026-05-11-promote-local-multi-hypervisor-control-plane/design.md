## Context

ChaosControl already has a bounded local multi-hypervisor campaign receipt model and runner surface. The model validates queue entries, leases, run IDs, worker identities, resource budgets, artifact roots, artifact indexes, follow-up jobs, queue-state persistence, and anti-claims. The packaged KVM smoke rail now emits a real campaign plan/receipt for `raft` and `rust-workload`, and full `nix flake check -L` passes.

The remaining work is not a new scheduler daemon. It is to make the generated readiness surfaces match the evidence: the one-machine local control plane is supported in a bounded form, while hosted services, shared remote queues, cross-machine scheduling, and full Antithesis parity remain non-goals.

## Goals / Non-Goals

**Goals:**
- Promote the local multi-hypervisor control-plane row to a supported bounded-local status.
- Require status wording to cite durable receipt-backed control-plane evidence.
- Keep the generated report and README roadmap aligned with the evidence baseline.

**Non-Goals:**
- Build a hosted service, SaaS UI, shared remote queue, or cross-machine scheduler.
- Claim universal fleet throughput or full Antithesis-style product replacement.
- Change SDK language scope or add non-Rust SDKs.

## Decisions

### 1. Promote by status gate, not new runtime code

**Choice:** Update generated readiness status and promotion-gate expectations to treat the existing durable local control-plane receipt/KVM smoke rail as the supported local workflow.

**Rationale:** The runtime and receipt evidence already exist. Adding a new daemon would widen scope without improving the current product claim.

**Alternative:** Build a hosted or networked scheduler next. Rejected because current scope is one-machine multiple local hypervisors.

### 2. Preserve separate evidence classes

**Choice:** The supported local control plane remains distinct from accepted workload replay proof and hosted/fleet readiness.

**Rationale:** Accepted workload rows prove named workload snapshot-backed replay. The local control-plane row proves orchestration/receipt quality for bounded local multi-hypervisor execution.

## Risks / Trade-offs

**Overclaiming local control-plane support** → Promotion gate must still reject hosted service, shared remote queue, cross-machine scheduling, universal fleet-scale throughput, and full Antithesis parity wording.

**Stale generated docs** → Regenerate `docs/replay-readiness-status.md` from Rust-owned constants and verify with `--check`.
