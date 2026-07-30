# Fault Application Evidence Specification

## Purpose

Report fault selection, validation, successful application, failure, and guest-visible observation as distinct deterministic stages.

## Requirements

### Requirement: Fault attempts have explicit stage semantics

r[chaoscontrol.fault_outcomes.stage_model] Each selected fault MUST have a canonical BLAKE3 attempt identity and MUST transition only through the valid selected, applicable, applied, application-failed, rejected, and observed stage relationships.

#### Scenario: Due fault is selected

- GIVEN a deterministic fault schedule makes a normalized fault due
- WHEN the engine selects it
- THEN ChaosControl MUST record a selected attempt
- AND it MUST NOT describe that attempt as applied, fired, injected, or observed before the corresponding stage succeeds.

#### Scenario: Stage event is duplicated or out of order

r[chaoscontrol.fault_outcomes.validation.core]
- GIVEN a stage event is stale, duplicated, references another attempt, or violates the transition graph
- WHEN the outcome core evaluates it
- THEN the event MUST be rejected deterministically
- AND counters and prior authoritative state MUST remain unchanged.

### Requirement: Applicability is validated before mutation

r[chaoscontrol.fault_outcomes.applicability] ChaosControl MUST validate fault targets, parameters, arithmetic, ranges, topology, device/backend presence, and declared implementation capabilities in a pure planner before an imperative adapter mutates simulation state.

#### Scenario: Target or parameter is invalid

r[chaoscontrol.fault_outcomes.validation.negative]
- GIVEN a selected fault names a missing VM or device, invalid vCPU or register bit, invalid rate or duration, overflowing range, or unsupported capability
- WHEN applicability is planned
- THEN the attempt MUST receive a typed rejected outcome
- AND no VM, device, schedule, counter, or report state MAY be mutated as if application succeeded.

### Requirement: Applied faults have reachable enforcement paths

r[chaoscontrol.fault_outcomes.effect_reachability] A public fault variant MUST be reported as applied only when its imperative adapter successfully completes a synchronous effect or installs state that the named execution or data path actually consumes; otherwise the variant MUST be rejected as unsupported or application-failed.

#### Scenario: Fault only writes inert state

r[chaoscontrol.fault_outcomes.validation.variant_matrix]
- GIVEN a fault adapter would only assign a field that no block, network, scheduler, time, memory, CPU, process, or interrupt path reads
- WHEN conformance is evaluated
- THEN that adapter MUST NOT produce an applied outcome
- AND the variant MUST be connected to a real enforcement path or explicitly rejected as unsupported.

### Requirement: Application returns a typed outcome

r[chaoscontrol.fault_outcomes.application] Every imperative fault adapter MUST return a typed applied or application-failed record bound to the attempt identity and naming the immediate or armed effect mechanism.

#### Scenario: Required backend operation succeeds

- GIVEN a valid applicable plan and all required backend operations succeed
- WHEN the adapter completes
- THEN exactly one applied record MUST be emitted for the attempt
- AND applied accounting MUST advance exactly once.

#### Scenario: Required backend operation fails

r[chaoscontrol.fault_outcomes.application_failure]
- GIVEN a valid plan but a required KVM, device, or simulation operation fails
- WHEN the adapter handles the error
- THEN it MUST emit application-failed rather than applied
- AND any partial mutation MUST be rolled back, leave the affected VM non-runnable, or be labeled indeterminate by explicit policy
- AND ordered outcomes from earlier attempts MUST remain available.

### Requirement: Observation originates at effect consumption

r[chaoscontrol.fault_outcomes.observation] An observed record MUST be emitted only by the execution or data path where an applied fault changes a concrete operation and MUST bind both the fault attempt and deterministic operation identities.

#### Scenario: Armed fault is never exercised

r[chaoscontrol.fault_outcomes.validation.observation]
- GIVEN an applied fault installs a valid future-trigger mechanism but the workload never reaches that operation
- WHEN the run completes
- THEN the attempt MUST remain applied and unobserved
- AND reports MUST NOT infer guest-visible impact from application alone.

#### Scenario: Armed fault changes an operation

- GIVEN an applied mechanism is active when its target block, packet, schedule, clock, memory, CPU, process, or interrupt operation occurs
- WHEN the mechanism changes that operation
- THEN one or more typed observations MUST identify the concrete effect and operation.

### Requirement: Accounting is stage-specific

r[chaoscontrol.fault_outcomes.accounting] Fault engine snapshots, round results, reports, and replay comparisons MUST maintain separate selected, rejected, applied, application-failed, and observed counters derived only from valid stage transitions.

#### Scenario: Mixed outcomes occur in one round

- GIVEN a round contains an applied attempt, a rejected attempt, and a later adapter failure
- WHEN round accounting is produced
- THEN each attempt MUST appear in deterministic order under its actual outcome
- AND no ambiguous fired or injected total MAY count all three as successful application.

### Requirement: Pending fault state survives snapshots

r[chaoscontrol.fault_outcomes.snapshot_state] Existing engine and simulation snapshot owners MUST preserve attempt identities, stage counters, pending applied mechanisms, and deterministic observation ordering needed for continuation.

#### Scenario: Snapshot contains an armed unobserved fault

- GIVEN a valid mechanism has been applied but not yet consumed
- WHEN a complete simulation snapshot is restored and the target operation occurs
- THEN the restored run MUST produce the same observation and stage accounting as uninterrupted execution.

### Requirement: Fault planning has a functional core

r[chaoscontrol.fault_outcomes.boundary] Attempt identity, applicability, effect-plan construction, transition validation, rejection classification, and counter deltas MUST be pure deterministic logic, while KVM/device mutation, runtime observation capture, persistence, logging, and rendering remain in imperative shells.

#### Scenario: Identical facts produce identical plan

- GIVEN identical normalized fault, topology, capability, range, and policy facts
- WHEN applicability is evaluated
- THEN the core MUST return the same plan or rejection without filesystem, environment, clock, process, KVM, device, output, or ambient mutable-state access.

### Requirement: Outcome schema compatibility preserves distinctions

r[chaoscontrol.fault_outcomes.compatibility] Exploration, minimization, replay, dashboard, and evidence-boundary consumers MUST preserve fault stage distinctions; any temporary legacy field MUST document exactly one source stage and MUST NOT imply later stages.

#### Scenario: Legacy consumer requests fired faults

- GIVEN a consumer uses a legacy ambiguous field
- WHEN compatibility data is produced
- THEN the field MUST be explicitly mapped to one documented stage or rejected
- AND acceptance logic MUST use the stage-specific records rather than infer application or observation.

### Requirement: Every variant has conformance evidence

r[chaoscontrol.fault_outcomes.validation] The change MUST maintain a complete public-variant matrix with positive application evidence for supported variants, explicit rejection evidence for unsupported variants, negative input/capability cases, observation tests, and deterministic replay/snapshot comparisons.

#### Scenario: Full variant matrix runs

r[chaoscontrol.fault_outcomes.validation.replay]
- GIVEN the conformance suite enumerates every public fault variant
- WHEN focused and replay tests run
- THEN every variant MUST have an asserted supported or unsupported outcome
- AND repeated execution and snapshot continuation MUST preserve ordered attempt, stage, counter, mechanism, and observation records.
