## Context

ChaosControl treats Antithesis documentation as a comparison source. The source covers simulation, fuzzing, assertions, faults, exploration, replay, and reports.

Repo policy makes the boundary explicit. Comparison material is not a requirement or parity claim.

WalTier works at a different layer. It simulates coordination over an object-store seam. Each seeded run interleaves operations, injected faults, latency, and crash/reopen cycles.

Its oracle checks committed-history monotonicity, exact-prefix instance state, and object retention after every step.

## Decisions

### Decision: Record WalTier DST as a second comparison source

**Choice:** Add WalTier DST beside Antithesis with the same bounded, non-parity posture.

**Rationale:** WalTier supplies a store-seam simulation example. ChaosControl supplies a VMM simulation system. The two layers have different authority.

### Decision: Name oracle invariants as bounded inputs

**Choice:** Document history monotonicity, exact-prefix state, and object retention. Do not add them as new ChaosControl gates.

**Rationale:** ChaosControl already owns deterministic replay, assertions, and verifier comparisons. The reference informs these rails without duplicating them.

### Decision: Add no implementation obligation

**Choice:** This change affects documentation only. It adds no code, dependency, fixture, or gate.

**Rationale:** A bounded pointer preserves the insight without expanding product scope.

## Risks / Trade-offs

- A reader can apply a store-seam claim to KVM evidence. State the mechanism layer and claim boundary.
- The reference can become stale. Keep a source pointer and named invariants instead of a vendored copy.
