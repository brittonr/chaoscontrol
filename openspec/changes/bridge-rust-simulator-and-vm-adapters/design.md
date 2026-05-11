## Context

The in-process simulator rail has deterministic scheduler/clock/RNG/fault hooks and receipts. The VM rail has snapshot-backed proof for accepted workloads. Rust users need a single mental model that can exercise both modes without blurring evidence classes.

## Goals / Non-Goals

**Goals:**
- Shared Rust workload adapter identity/config across simulator and VM paths.
- Comparable receipt metadata for simulator and VM runs.
- Promotion gates that keep simulator evidence and snapshot replay proof separate.

**Non-Goals:**
- Full FoundationDB simulator parity.
- Arbitrary unmodified binary support in the simulator.
- Non-Rust SDKs.

## Decisions

### 1. Adapter identity is the bridge

**Choice:** Bind workload name, adapter version, scenario, seed/fault schedule identity, and artifact digests in both simulator and VM receipts.
**Rationale:** Operators can compare what was exercised without pretending the execution environments prove the same thing.
**Alternative:** Create separate unrelated APIs. Rejected because it duplicates Rust workload integration work.

### 2. Evidence classes remain disjoint

**Choice:** Add explicit receipt fields and gate checks for `simulator-local` versus `vm-snapshot-replay` evidence classes.
**Rationale:** Simulator speed is useful, but it cannot replace hypervisor replay proof.

## Risks / Trade-offs

**Leaky abstraction** → Keep environment-specific hooks explicit and tested with negative fixtures.
