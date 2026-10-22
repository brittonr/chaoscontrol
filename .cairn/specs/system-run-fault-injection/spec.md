# System Run Fault Injection Specification

## Purpose

Defines the `system-run-fault-injection` capability.

## Requirements

### Requirement: Default fault injection for system runs
r[chaoscontrol.system_runs.fault_injection] A system run MUST inject seed-derived faults in addition to its named schedule unless the operator explicitly opts out, and the opt-out MUST be an explicit, named configuration value.

#### Scenario: A system run injects faults by default
r[chaoscontrol.system_runs.fault_injection.scenario.default]
- GIVEN a simulation configuration that does not name the fault injection setting
- WHEN the run builds its fault engine
- THEN the engine MUST inject random faults in addition to the named schedule.

#### Scenario: The operator opts out explicitly
r[chaoscontrol.system_runs.fault_injection.scenario.opt_out]
- GIVEN a simulation configuration that names the fault injection setting as off
- WHEN the run builds its fault engine
- THEN the engine MUST NOT inject random faults, and the run MUST record the opt-out as an explicit choice.

#### Scenario: Injection stays deterministic under a fixed seed
r[chaoscontrol.system_runs.fault_injection.scenario.determinism]
- GIVEN the same master seed and the same configuration
- WHEN the run is repeated
- THEN the injected faults MUST be the same, so the default does not trade determinism for coverage.

### Requirement: Exploration runs take, record, and replay the setting
r[chaoscontrol.system_runs.fault_injection.exploration] An exploration run MUST take the fault injection setting from its operator, with the default on, and MUST build every branch, worker, and minimization run with that value. The run MUST record the value as an explicit boolean in its checkpoint, its campaign progress, and every bug report, and resumption, minimization, reproduction, and replay MUST use the recorded value.

#### Scenario: An exploration run takes the setting from its operator
r[chaoscontrol.system_runs.fault_injection.scenario.exploration]
- GIVEN an exploration command or run profile that leaves the fault injection setting unnamed or names it as a boolean
- WHEN the explorer builds its system runs
- THEN every run MUST inject random faults unless the operator named the setting as off.
- AND a value that is not an explicit boolean MUST be rejected before any run starts.

#### Scenario: Later runs use the recorded setting
r[chaoscontrol.system_runs.fault_injection.scenario.recorded]
- GIVEN a checkpoint, campaign progress file, bug report, or replay recording
- WHEN resumption, minimization, reproduction, or replay rebuilds the run
- THEN it MUST use the recorded setting rather than the default.
- AND a record written before the setting existed MUST load as off, because its run injected no random faults.
