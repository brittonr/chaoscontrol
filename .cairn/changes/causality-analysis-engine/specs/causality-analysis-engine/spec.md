# Causality Analysis Engine Specification

## Purpose

Produce a minimized scheduling delta and a ranked probable-cause attribution for a reproduced assertion failure, with bounded evidence.

## ADDED Requirements

### Requirement: Interleaving minimization is a pure core

r[chaoscontrol.causality.minimization_core] Interleaving minimization MUST be pure deterministic logic over replay outcomes, and MUST return the smallest scheduling delta that still reproduces the failure under the declared budget.

#### Scenario: Minimal delta
- GIVEN a reproduced failure under a long schedule
- WHEN minimization runs
- THEN the core MUST return a shorter schedule that still reproduces the failure.

#### Scenario: Budget exhausted
- GIVEN a minimization run that exceeds the declared budget
- WHEN the run ends
- THEN the core MUST return a partial result and an explicit exhaustion record.

### Requirement: Attribution ranks candidates

r[chaoscontrol.causality.attribution] Attribution MUST rank seed, fault schedule, declared event, and variant classes by whether neutralizing each changes the replay outcome, and MUST state that the rank is a probability estimate, not a proof.

#### Scenario: Reliable candidate ranks first
- GIVEN a fixture race whose declared event explains the failure
- WHEN attribution runs
- THEN that event class MUST rank above neutral candidates for the fixture.

#### Scenario: No discriminating cause
- GIVEN candidates whose neutralization never changes the outcome
- WHEN attribution runs
- THEN the engine MUST report equivalent ranking without inventing a cause.

### Requirement: Shell and core remain separated

r[chaoscontrol.causality.boundary] The core MUST receive candidate replay outcomes and return decisions. The shell MUST read replay and verdict artifacts and drive candidate executions.

#### Scenario: Core reads no artifacts
- GIVEN a direct call to the core
- WHEN the core runs
- THEN it MUST not read files, clocks, or processes.

### Requirement: Attribution is bounded and recorded

r[chaoscontrol.causality.budget] Every analysis MUST enforce a declared candidate budget and MUST record the budget spent and the candidate set in its evidence.

#### Scenario: Budget recorded
- GIVEN a completed analysis
- WHEN its evidence is inspected
- THEN the candidate count and the spent budget MUST be present.

### Requirement: Analysis evidence binds to verdicts

r[chaoscontrol.causality.evidence_binding] Attribution and minimized-delta artifacts MUST bind to the replay verdict, the snapshot identities, and the candidate set, and MUST fail closed on identity drift.

#### Scenario: Artifact identity drifts
- GIVEN analysis evidence whose verdict identity differs from the executed replay
- WHEN receipt validation runs
- THEN the analysis MUST fail closed.

### Requirement: Causality validation is adversarial

r[chaoscontrol.causality.validation] Validation MUST pair a positive attribution and minimization fixture with negative fixtures for budget exhaustion, equivalent ranking, identity drift, and non-reproducing candidates.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to treat analysis as evidence
- WHEN core, shell, replay, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
