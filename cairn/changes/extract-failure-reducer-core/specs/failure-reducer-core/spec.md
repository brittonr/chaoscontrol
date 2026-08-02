# Failure Reducer Core Specification

## Purpose

Defines a reusable deterministic reduction mechanism that separates candidate selection from predicate execution.

## Requirements

### Requirement: Failure reduction has a shared repository

r[shared.failure_reducer.repository] The project MUST publish a product-neutral `failure-reducer` repository under `AGPL-3.0-or-later`. Consumers MUST pin an immutable reviewed revision without a sibling path fallback.

#### Scenario: ChaosControl adopts the reducer

- GIVEN the shared repository passes its package and behavior checks
- WHEN ChaosControl adds the dependency
- THEN it MUST pin one immutable reviewed revision
- AND normal builds MUST NOT use a mutable or workspace-relative fallback.

### Requirement: The reducer is an incremental pure state machine

r[shared.failure_reducer.state_machine] The reducer core MUST consume an ordered source set, explicit policy, current immutable state, and supplied candidate outcome. It MUST return the next candidate request, completion, or typed failure without executing the predicate.

#### Scenario: A candidate needs evaluation

- GIVEN a valid reduction state has remaining work
- WHEN the pure transition runs
- THEN it MUST return one candidate identity and ordered item subset
- AND it MUST NOT call a process, VM, clock, network, filesystem, or callback.

#### Scenario: A stale outcome enters the core

- GIVEN the core requested one candidate identity
- WHEN the caller supplies an outcome for another identity
- THEN the core MUST return a stale-response failure
- AND the prior state MUST remain unchanged.

### Requirement: Candidate order is deterministic

r[shared.failure_reducer.determinism] Partitioning, complements, granularity changes, tie breaks, and completion MUST follow a versioned deterministic algorithm. Equal source, policy, and supplied outcomes MUST produce equal requests and final results.

#### Scenario: Equal sessions are replayed

- GIVEN two sessions have equal ordered source items, policy, and outcome sequence
- WHEN both sessions run to completion
- THEN every requested candidate identity and subset MUST match
- AND their completion results and transcript identities MUST match.

### Requirement: Predicate outcomes are explicit

r[shared.failure_reducer.predicate_boundary] Candidate outcomes MUST distinguish `Reproduces`, `DoesNotReproduce`, and `Indeterminate`. Indeterminate handling MUST follow explicit fail, bounded-retry, or conservative-retention policy.

#### Scenario: Predicate execution times out

- GIVEN the shell cannot classify a candidate before its deadline
- WHEN it reports an indeterminate timeout
- THEN the core MUST apply the declared indeterminate policy
- AND it MUST NOT treat the timeout as proof that the failure does not reproduce.

### Requirement: Reduction work is bounded

r[shared.failure_reducer.budgets] Policy MUST provide named limits for source items, candidate evaluations, transcript entries, and indeterminate retries. The core MUST use checked arithmetic and stop before crossing a limit.

#### Scenario: Evaluation budget is exhausted

- GIVEN the next candidate would exceed the evaluation budget
- WHEN the core plans the transition
- THEN it MUST return a bounded incomplete status or typed budget failure
- AND it MUST NOT request another predicate execution.

### Requirement: Completion claims local minimality only

r[shared.failure_reducer.minimality] A successful reduction MAY claim only that the retained ordered set is locally one-minimal under the declared algorithm and supplied predicate transcript. It MUST NOT claim globally smallest cardinality or predicate correctness.

#### Scenario: A smaller untested combination exists

- GIVEN the completed transcript did not evaluate a smaller reproducing combination
- WHEN the result is reported
- THEN it MUST retain the local minimality claim
- AND it MUST NOT describe the result as globally minimum.

### Requirement: Reduction transcripts have stable identity

r[shared.failure_reducer.transcript] A transcript MUST bind algorithm version, ordered source identity, policy, candidate identities, supplied outcomes, and final status with domain-separated BLAKE3. Transcript retention MUST remain within policy.

#### Scenario: One predicate outcome changes

- GIVEN two sessions differ in one supplied candidate outcome
- WHEN transcript identity is computed
- THEN their BLAKE3 identities MUST differ
- AND both transcripts MUST preserve the differing outcome at the same candidate identity.

### Requirement: ChaosControl retains predicate authority

r[shared.failure_reducer.chaoscontrol_adapter] The ChaosControl adapter MUST preserve fault order and stable fault identity. It MUST classify reproduction through one exact accepted assertion identity and the required replay and fault-observation policy.

#### Scenario: A candidate selects a fault but never observes it

- GIVEN a reduced schedule selects a fault that the accepted predicate requires to affect execution
- WHEN ChaosControl evaluates reproduction
- THEN selection alone MUST NOT satisfy the predicate
- AND the adapter MUST use the accepted fault outcome and replay facts.

### Requirement: Shared claims remain bounded

r[shared.failure_reducer.claim_boundary] The shared repository MUST NOT claim process isolation, VM determinism, assertion correctness, fault application, artifact trust, evidence validity, or release eligibility.

#### Scenario: A consumer supplies an unsound predicate

- GIVEN the reducer records consistent supplied outcomes from an unsound consumer predicate
- WHEN reduction completes
- THEN the transcript MAY prove which outcomes were supplied
- AND it MUST NOT claim that those outcomes were semantically correct.

### Requirement: Migration preserves behavior

r[shared.failure_reducer.migration] ChaosControl MUST compare candidate order, retained fault order, final result, and accepted replay classification before removing the current reducer.

#### Scenario: Shared candidate order differs

- GIVEN a maintained deterministic minimization fixture
- WHEN current and shared reducers request different candidate sequences
- THEN migration MUST stop
- AND the difference MUST require an explicit algorithm-version decision.

### Requirement: Checks include failure modes

r[shared.failure_reducer.validation] The shared and consumer suites MUST include positive reduction cases and negative empty, singleton, stale, duplicate, indeterminate, non-reproducing, non-monotonic, overflow, and exhausted-budget cases.

#### Scenario: Full reducer checks run

- GIVEN unit fixtures and ChaosControl integration fixtures
- WHEN all focused checks run
- THEN valid sessions MUST produce deterministic bounded results
- AND invalid sessions MUST fail without panic, hidden retry, predicate execution in core, or state corruption.
