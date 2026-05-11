## Context

FoundationDB-style simulation runs the system or model in a controlled deterministic process with simulated time, network, disk, and randomness. ChaosControl’s VMM rail should remain the replay-proof surface; an in-process simulator should be a faster complementary exploration rail for workloads that can adopt explicit simulator adapters.

## Goals / Non-Goals

**Goals:**
- Define a deterministic simulator kernel with explicit sources of time, randomness, scheduling, network, disk, and faults.
- Provide a workload adapter boundary so supported workloads can run in-process without pretending to be unmodified guests.
- Emit reproducibility receipts that bind seed/config/fault schedule/history digests.
- Prevent simulator evidence from being promoted as VM replay evidence.

**Non-Goals:**
- Running arbitrary unmodified binaries in-process.
- Replacing the VMM snapshot/replay rail.
- Full FoundationDB parity or a broad workload ecosystem in the first slice.

## Decisions

### 1. Adapter-first simulator boundary

**Choice:** Only workloads that implement explicit simulator traits can run in the in-process rail.
**Rationale:** This makes nondeterminism sources visible and testable.
**Alternative:** Attempt transparent binary interposition; rejected because it overlaps with the VMM rail and is too broad.

### 2. Receipts before broad features

**Choice:** Add deterministic receipt/config/history contracts before many simulated devices.
**Rationale:** Evidence surfaces must prevent overclaiming and support reproducibility from the first slice.
**Alternative:** Build many simulated faults first; rejected because without receipts they cannot become trustworthy evidence.

## Risks / Trade-offs

**Divergence from VM semantics** → Reports must explicitly identify simulator-only evidence and require separate VM replay for promotion.
**Adapter burden** → Keep the first workload narrow and document the adapter contract clearly.
