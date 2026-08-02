## Why

ChaosControl contains delta debugging for fault schedules, but the reducer is private and calls VMM reruns directly. Candidate generation, result classification, retry policy, replay execution, and assertion selection share one control path.

A reusable reducer can serve ChaosControl and other OnixResearch test, proof, parser, and workflow tools. The reusable core must request predicate evaluations without performing them.

## What Changes

- Establish a product-neutral `failure-reducer` repository under AGPL-3.0-or-later.
- Implement a pure incremental reduction state machine over ordered item identities.
- Distinguish reproduces, does-not-reproduce, and indeterminate predicate outcomes.
- Enforce caller-owned candidate, evaluation, and transcript budgets.
- Record deterministic BLAKE3-bound reduction transcripts and bounded completion status.
- Keep process, VMM, timeout, retry, persistence, and evidence decisions in consumer shells.
- Add a ChaosControl adapter for fault schedules and exact assertion targets after fault-outcome and assertion-identity prerequisites complete.

## Impact

- **Source candidates**: `chaoscontrol-explore/src/minimizer.rs` and fault schedule subset helpers.
- **New repository**: `failure-reducer` with a pure core and optional standard-library orchestration adapters.
- **Consumers**: ChaosControl first. Cairn, Kamacite, Octet, and other corpus-driven tools remain independent adoption targets.
- **Compatibility**: reduced ChaosControl schedules must preserve ordering, stable fault identity, and the accepted replay predicate.
- **Claims**: completion establishes bounded reduction under observed predicate outcomes. It does not prove predicate correctness or global minimality.
