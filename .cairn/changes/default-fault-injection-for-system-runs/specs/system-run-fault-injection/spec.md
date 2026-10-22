# System Run Fault Injection Delta Specification

## ADDED Requirements

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
