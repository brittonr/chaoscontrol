# Tasks: default fault injection for system runs

## Phase 1: Implementation

- [x] [serial] T1 Add the explicit fault injection setting to the simulation configuration with the default on, and pass it to the fault engine. r[chaoscontrol.system_runs.fault_injection]
- [x] [parallel] T2 Give the integration harness and the performance benchmark the explicit opt-out. r[chaoscontrol.system_runs.fault_injection.scenario.opt_out]
- [x] [parallel] T3 Add the unit tests for the default, the opt-out, and the seed-derived determinism of the injection. r[chaoscontrol.system_runs.fault_injection.scenario.default] r[chaoscontrol.system_runs.fault_injection.scenario.determinism]
- [x] [serial] T4 Document the default and the opt-out on the fault surface. r[chaoscontrol.system_runs.fault_injection]

## Phase 2: Verification and completion evidence

- [x] [serial] V1 Build the VMM crate and run the affected tests and gates. r[chaoscontrol.system_runs.fault_injection]
- [ ] [serial] V2 Run the Cairn gates, sync the accepted spec, archive the change, and record the evidence. Blocked in this environment: the available `cairn` 0.1.0 builds predate the `outcome-machine` workflow profile that this repository's generated policy requires, and this checkout has no vendored Cairn source to rebuild from. r[chaoscontrol.system_runs.fault_injection.scenario.opt_out]
