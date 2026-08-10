# Architecture Module Boundaries Specification

## Purpose

Defines the `architecture-module-boundaries` capability.

## Requirements

### Requirement: Modules have explicit owners

r[chaoscontrol.architecture_modules.ownership] Each migrated module MUST name its owned state, invariants, accepted inputs, produced outputs, allowed effects, and test boundary.

#### Scenario: Type moves to a new module
- GIVEN a type is selected for migration
- WHEN its owner map is reviewed
- THEN exactly one module MUST own its mutation and invariant enforcement.

#### Scenario: Ownership is ambiguous
- GIVEN two modules can mutate the same state without one checked plan boundary
- WHEN architecture validation runs
- THEN validation MUST fail.

### Requirement: VM effects follow checked plans

r[chaoscontrol.architecture_modules.vmm] VM construction, transition, snapshot, poison, and teardown decisions MUST be separate from KVM, thread, timer, memory, and device effects.

#### Scenario: VM transition is valid
- GIVEN the pure core receives valid current state and an observation
- WHEN it plans the next transition
- THEN the shell MAY apply only the returned checked plan.

### Requirement: Controller effects follow checked plans

r[chaoscontrol.architecture_modules.controller] Scheduling, fault, observation, and multi-VM commit decisions MUST be pure before controller shells mutate VMs or publish evidence.

#### Scenario: Multi-VM round fails before commit
- GIVEN one selected VM cannot complete its planned transition
- WHEN controller classification runs
- THEN no later shell effect MAY publish the round as complete.

### Requirement: Evidence ownership is separated

r[chaoscontrol.architecture_modules.evidence] Evidence file loading, structured classification, orchestration, rendering, and publication MUST have separate module owners. Classification MUST operate on in-memory facts.

#### Scenario: Evidence report is rendered
- GIVEN the core returns one validated render model
- WHEN the shell writes Markdown or JSON
- THEN rendering MUST NOT recompute promotion eligibility.

### Requirement: Core dependency direction is enforced

r[chaoscontrol.architecture_modules.boundary] Pure cores MUST NOT read files, inspect environment, execute processes, access clocks, call KVM, print output, or depend on ambient mutable state.

#### Scenario: Core imports a shell effect
- GIVEN a core module adds a forbidden effect dependency
- WHEN architecture validation runs
- THEN validation MUST fail with the module and effect class.

### Requirement: Migration preserves compatibility

r[chaoscontrol.architecture_modules.migration] The migration MUST preserve public Rust behavior, JSON fields, enum meanings, error classes, receipt semantics, and deterministic transition results.

#### Scenario: Compatibility fixture is evaluated
- GIVEN a pre-migration fixture and identical inputs
- WHEN the migrated code evaluates it
- THEN public outputs MUST remain equal.

### Requirement: Architecture validation covers failure paths

r[chaoscontrol.architecture_modules.validation] Validation MUST pair successful transitions with invalid state, partial commit, poison, cancellation, timer, handle-lifetime, teardown, schema, and forbidden-dependency cases.

#### Scenario: Closeout validation runs
- GIVEN all call sites use the owned modules
- WHEN focused and workspace validation runs
- THEN compatibility tests MUST pass and every negative class MUST fail as specified.
