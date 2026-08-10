# State-Machine Property Coverage Specification

## Purpose

Exercise pure stateful cores through bounded generated command sequences and independent invariants.

## ADDED Requirements

### Requirement: Property runs use typed profiles

r[chaoscontrol.property_coverage.profile] Each property lane MUST bind target model, seed policy, sequence bound, case bound, shrink bound, retained counterexample bound, and lane identity.

#### Scenario: Profile is complete
- GIVEN every finite bound and target identity is present
- WHEN profile validation runs
- THEN it MUST produce one deterministic test configuration.

#### Scenario: Profile is unbounded
- GIVEN one required limit is absent
- WHEN profile validation runs
- THEN validation MUST fail.

### Requirement: Generated tests use reference models

r[chaoscontrol.property_coverage.framework] Each selected core MUST have a smaller reference model for states, commands, expected outcomes, and invariants. Generators MUST create bounded valid and invalid command sequences.

#### Scenario: Valid sequence executes
- GIVEN the model admits the next command
- WHEN the model and implementation advance
- THEN their projected states and outcome classes MUST agree.

#### Scenario: Invalid command executes
- GIVEN the model rejects the next command
- WHEN the implementation evaluates it
- THEN it MUST return the expected typed error without prohibited mutation.

### Requirement: Scheduler and snapshot sequences are covered

r[chaoscontrol.property_coverage.scheduler_snapshot] Generated coverage MUST exercise scheduler selection, wake, halt, stale events, snapshot capture, restore, overlay, and continuation sequences.

#### Scenario: Snapshot sequence restores
- GIVEN a generated valid prefix is captured and restored
- WHEN both original and restored states receive the same suffix
- THEN their projected outcomes MUST remain equal.

### Requirement: Fault and assertion sequences are covered

r[chaoscontrol.property_coverage.fault_assertion] Generated coverage MUST exercise fault selection through observation and heal, plus assertion catalog, event, merge, snapshot, and rejection transitions.

#### Scenario: Rejected assertion event appears
- GIVEN a generated event has stale or conflicting identity
- WHEN the assertion core rejects it
- THEN counters and accepted catalog state MUST remain unchanged.

### Requirement: Virtio and evidence sequences are covered

r[chaoscontrol.property_coverage.virtio_evidence] Generated coverage MUST exercise virtio transport states and evidence admission states with malformed, partial, stale, and valid inputs.

#### Scenario: Virtio transition is malformed
- GIVEN a generated command violates queue or status rules
- WHEN transport validation runs
- THEN it MUST return a bounded typed violation without successful commit.

### Requirement: Properties enforce independent invariants

r[chaoscontrol.property_coverage.invariants] Property tests MUST check state validity, no mutation after rejection, exact commit count, capacity bounds, continuation, identity binding, and deterministic output.

#### Scenario: Model and implementation share a wrong transition
- GIVEN both projections agree but an independent invariant fails
- WHEN the property evaluates the state
- THEN the case MUST still fail.

### Requirement: Shrinking preserves failure class

r[chaoscontrol.property_coverage.shrink] A minimized counterexample MUST retain the same named invariant or outcome failure. Stable minimized cases MUST become normal regression fixtures.

#### Scenario: Smaller sequence changes failure meaning
- GIVEN a shrink candidate fails under another invariant
- WHEN shrink admission runs
- THEN the candidate MUST be rejected.

### Requirement: Property lanes are bounded and separate

r[chaoscontrol.property_coverage.ci] A fast deterministic lane MUST run in normal CI. A deeper bounded lane MAY run on a schedule. Neither lane MUST replace KVM behavior validation.

#### Scenario: Fast lane fails
- GIVEN a property finds a counterexample
- WHEN CI reports the failure
- THEN it MUST retain the profile, seed, target, and minimized sequence.

### Requirement: Property claims remain bounded

r[chaoscontrol.property_coverage.boundary] Passing generated cases MUST NOT claim formal proof, complete state coverage, KVM behavior, or absence of defects.

#### Scenario: Passing lane becomes proof claim
- GIVEN all selected cases pass
- WHEN a report claims complete correctness
- THEN claim validation MUST reject the report.

### Requirement: Property coverage validation is adversarial

r[chaoscontrol.property_coverage.validation] Validation MUST include positive valid sequences and negative invalid commands, invariant failures, shrink-class drift, bound failures, and stale regression fixtures.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to enable both lanes
- WHEN focused, regression, workspace, lifecycle, and CI validation runs
- THEN every positive and negative class MUST produce its expected result.
