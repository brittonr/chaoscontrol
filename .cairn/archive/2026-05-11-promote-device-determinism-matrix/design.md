## Context

The canonical `vm-determinism-drift` spec explicitly says the current hide-TSC evidence is profile-specific and not a universal hypervisor/device/timing theorem. The next step should widen evidence through a named matrix while preserving fail-closed wording.

## Goals / Non-Goals

**Goals:**
- Define a matrix contract for guest/device/profile combinations.
- Bind every matrix row to deterministic input fingerprints and observed fingerprints.
- Add negative evidence that detects drift and prevents overpromotion.
- Generate operator-facing status that distinguishes bounded matrix coverage from universal determinism.

**Non-Goals:**
- Proving all x86 devices, host CPUs, kernel versions, or timing sources deterministic.
- Replacing accepted workload replay evidence.
- Launching expensive broad VM campaigns by default.

## Decisions

### 1. Matrix as evidence contract, not theorem

**Choice:** Add a Rust-owned matrix receipt that lists selected guest/device/profile rows and their drift results.
**Rationale:** This fits current evidence architecture and keeps promotion evidence concrete.
**Alternative:** Claim arbitrary determinism from the hide-TSC gate; rejected because current docs explicitly forbid that overclaim.

**Implementation:** Add pure aggregation/validation code first, then wire one packaged bounded matrix rail.

### 2. Negative drift fixtures are required

**Choice:** Validators must exercise at least one mutated/mismatched matrix row.
**Rationale:** Without negative evidence, a matrix checker can accidentally accept stale or incomplete rows.
**Alternative:** Only run positive VM cases; rejected because it does not prove the guard fails closed.

## Risks / Trade-offs

**VM cost** → Keep the first packaged rail small and make larger profile/device sets opt-in.
**Overclaiming** → Status text and promotion gates must keep “bounded matrix coverage” separate from “arbitrary determinism”.
