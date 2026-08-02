## Phase 0: Prerequisites and inventory

- [ ] [serial] Complete the unified AGPL license boundary before shared publication and adoption. [depends:adopt-unified-agpl-license]
- [ ] [serial] Complete strict assertion identity so the minimizer can target one exact accepted property. [depends:reject-assertion-identity-conflicts]
- [ ] [serial] Complete fault application outcomes so minimization does not treat selection as observed effect. [depends:verify-fault-application-outcomes]
- [ ] [serial] Inventory current candidate generation, VMM execution, assertion matching, retry, and output responsibilities. r[shared.failure_reducer.chaoscontrol_adapter]

## Phase 1: Shared reducer core

- [ ] [serial] Establish the `failure-reducer` AGPL repository and immutable publication workflow. r[shared.failure_reducer.repository]
- [ ] [serial] Define versioned source, policy, state, candidate request, outcome, completion, and typed failure values. r[shared.failure_reducer.state_machine]
- [ ] [serial] Implement pure partition, complement, granularity, tie-break, and completion transitions with checked counters. r[shared.failure_reducer.determinism] r[shared.failure_reducer.budgets]
- [ ] [parallel] Implement explicit indeterminate handling and bounded retry requests without executing predicates in core. r[shared.failure_reducer.predicate_boundary]
- [ ] [parallel] Implement deterministic BLAKE3-bound transcripts and bounded completion status. r[shared.failure_reducer.transcript]

## Phase 2: Generic shell and ChaosControl adapter

- [ ] [parallel] Add a thin standard shell for caller-provided predicate execution, cancellation, deadline, and persistence adapters. r[shared.failure_reducer.predicate_boundary]
- [ ] [serial] Add a ChaosControl adapter that maps ordered fault identities to candidates and preserves schedule order. r[shared.failure_reducer.chaoscontrol_adapter]
- [ ] [serial] Require the adapter predicate to resolve one exact assertion identity and accepted replay outcome. r[shared.failure_reducer.chaoscontrol_adapter]
- [ ] [serial] Keep replay authority, VMM execution, fault observation, artifact policy, and evidence promotion outside the shared core. r[shared.failure_reducer.claim_boundary]

## Phase 3: Parity and checks

- [ ] [parallel] Compare current and shared candidate sequences and final reductions for deterministic positive fixtures. r[shared.failure_reducer.migration]
- [ ] [parallel] Add empty, singleton, no-reproducer, always-reproducer, non-monotonic, indeterminate, exhausted-budget, stale-response, duplicate-response, and overflow tests. r[shared.failure_reducer.validation]
- [ ] [serial] Migrate ChaosControl only after schedule ordering and accepted replay outcomes match. r[shared.failure_reducer.migration]
- [ ] [serial] Run shared repository checks, focused minimizer and replay tests, workspace checks, dependency policy, and Cairn gates before sync or archive. r[shared.failure_reducer.validation]
