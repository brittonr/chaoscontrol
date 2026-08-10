## Context

The pure cores already expose many deterministic transitions. Property tests can exercise long combinations without standing up KVM or filesystem state.

## Decisions

### 1. Start with explicit reference models

Each target gets a small model for valid states, accepted commands, expected errors, and invariants. The model must remain simpler than the implementation.

### 2. Generate commands, not arbitrary structs

Generators create bounded command sequences from the current model state. Separate invalid-command generators test rejection and no-mutation rules.

### 3. Seeds and bounds are named

Profiles name sequence length, case count, shrink budget, retained counterexamples, and lane. CI records the profile and failing seed.

### 4. Shrinking preserves failure meaning

A minimized sequence is accepted only when it violates the same named invariant or outcome class. Stable minimized cases become normal regression fixtures.

### 5. Invariants cover safety and determinism

Checks include state validity, no mutation after rejection, exact commit count, capacity bounds, snapshot continuation, catalog binding, and identical result from identical inputs.

### 6. Lanes remain distinct

The fast lane runs on every change with fixed bounds. The deep lane runs on a schedule or explicit request. KVM tests consume selected minimized cases only through separate harnesses.

### 7. Test logic has a functional core

Models, generators from seeded input, transition comparison, invariant evaluation, and shrink admission are pure. Test shells only persist counterexamples and invoke existing APIs.

## Risks

A model can repeat the same bug as the implementation. Keep models small, use independent representations, and add direct invariant checks beside model comparison.
