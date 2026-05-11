## Why

Item 7 in the competitor gap list is a Jepsen-style checker ecosystem. ChaosControl has SDK assertions and accepted replay evidence, but not a reusable library of operation histories, consistency models, nemesis/fault generators, and checker reports that can evaluate distributed-system semantics independently of a single assertion catalog.

## What Changes

- Add a consistency-checker ecosystem domain.
- Define typed operation histories, checker traits, model reports, and negative evidence.
- Keep checker results separate from snapshot replay proof and assertion-readiness promotion.

## Capabilities

### New Capabilities
- `consistency-checker-ecosystem`: reusable checker/history/generator contracts for Jepsen-style semantic validation.

## Impact

- **Files**: new checker crate or module, SDK/explorer history export, evidence report models, docs/status surfaces, OpenSpec canonical spec.
- **APIs**: typed history events, checker trait, model-specific reports, workload adapter hooks.
- **Testing**: pure checker fixtures, known-good/known-bad histories, workload adapter tests, and report anti-overclaim checks.
