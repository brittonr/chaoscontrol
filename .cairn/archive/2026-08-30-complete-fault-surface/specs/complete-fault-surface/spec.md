# Complete Fault Surface Specification

## Purpose

Execute clock freeze, clock jitter, CPU stall, and memory pressure as deterministic faults with full stage evidence, instead of returning unsupported rejections.

## ADDED Requirements

### Requirement: Clock freeze suspends virtual time

r[chaoscontrol.fault_surface.clock_freeze] A clock-freeze fault MUST suspend the admitted virtual clock for the declared window and MUST resume it deterministically at the window end.

#### Scenario: Freeze replays
- GIVEN two identical runs with the same freeze window
- WHEN the guest records the clock before and after the freeze
- THEN both runs MUST record the same elapsed values.

#### Scenario: Release after window
- GIVEN a clock-freeze fault with a finite window
- WHEN the window ends
- THEN the virtual clock MUST resume from the frozen boundary without a jump.

### Requirement: Clock jitter stays bounded

r[chaoscontrol.fault_surface.clock_jitter] A clock-jitter fault MUST apply a declared bound to the virtual clock and MUST respect that bound in every execution of the same configuration.

#### Scenario: Jitter within bound
- GIVEN a declared jitter bound
- WHEN the fault applies across identical runs
- THEN every observed delta MUST fall within the bound.

### Requirement: CPU stall suspends a vCPU

r[chaoscontrol.fault_surface.cpu_stall] A CPU-stall fault MUST mark the target vCPU not runnable for the declared window and MUST resume it exactly at the window end.

#### Scenario: Stalled vCPU does not progress
- GIVEN a CPU-stall fault targeting one vCPU
- WHEN the window is active
- THEN that vCPU MUST make no guest progress.

#### Scenario: Stall release
- GIVEN a CPU-stall fault with a finite window
- WHEN the window ends
- THEN the vCPU MUST resume and MUST continue deterministically.

### Requirement: Memory pressure is guest-visible

r[chaoscontrol.fault_surface.memory_pressure] A memory-pressure fault MUST expose a deterministically managed memory ceiling to the guest and MUST release the ceiling to the admitted baseline on completion.

#### Scenario: Ceiling applied and released
- GIVEN a memory-pressure fault
- WHEN the window is active
- THEN the guest MUST observe the declared ceiling.

#### Scenario: Ceiling restored
- GIVEN a completed memory-pressure fault
- WHEN the window ends
- THEN the ceiling MUST return to the admitted baseline.

### Requirement: Fault effects use the stage ledger

r[chaoscontrol.fault_surface.stage_evidence] Every new fault MUST move through the existing Selected, Applicable, Applied, and Observed stages, and MUST record a typed rejection or application failure on a misapplied window.

#### Scenario: Effect observed
- GIVEN an applied and observable freeze, stall, or pressure window
- WHEN the fault ledger is inspected
- THEN the record MUST show an Observed stage for the effect.

#### Scenario: Misapplied window
- GIVEN a clock, stall, or pressure fault with an invalid window
- WHEN the fault is planned
- THEN the record MUST show a typed rejection and MUST NOT show an Observed stage.

### Requirement: Unsupported capability stays visible

r[chaoscontrol.fault_surface.unsupported_visible] A capability that a profile does not execute MUST return the existing typed unsupported rejection and MUST NOT pretend to be applied.

#### Scenario: Rejection recorded
- GIVEN a profile that does not support one new effect
- WHEN the fault is selected
- THEN the typed unsupported rejection MUST be recorded.

### Requirement: Fault-surface validation is adversarial

r[chaoscontrol.fault_surface.validation] Validation MUST pair positive freeze, jitter, stall, and pressure fixtures with negative fixtures for invalid windows, missing release, misapplied stages, and unsupported profiles.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to treat these faults as executable
- WHEN planner, VM, replay, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
