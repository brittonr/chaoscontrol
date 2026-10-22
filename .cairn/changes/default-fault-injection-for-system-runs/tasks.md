# Tasks: default fault injection for system runs

## Phase 1: Implementation

- [x] [serial] T1 Add the explicit fault injection setting to the simulation configuration with the default on, and pass it to the fault engine. r[chaoscontrol.system_runs.fault_injection]
- [x] [parallel] T2 Give the integration harness and the performance benchmark the explicit opt-out. r[chaoscontrol.system_runs.fault_injection.scenario.opt_out]
- [x] [parallel] T3 Add the unit tests for the default, the opt-out, and the seed-derived determinism of the injection. r[chaoscontrol.system_runs.fault_injection.scenario.default] r[chaoscontrol.system_runs.fault_injection.scenario.determinism]
- [x] [serial] T4 Document the default and the opt-out on the fault surface. r[chaoscontrol.system_runs.fault_injection]
- [x] [serial] T5 Give exploration the explicit setting: `ExplorerConfig.random_faults`, `--random-faults true|false` on `run` and `campaign`, and `exploration.random_faults` in the run profile and its Nickel contract, each defaulting to on. r[chaoscontrol.system_runs.fault_injection.exploration] r[chaoscontrol.system_runs.fault_injection.scenario.exploration]
- [x] [serial] T6 Record the setting in checkpoints, campaign progress, bug reports, and replay recordings, rebuild resumed, minimized, reproduced, and replayed runs from the record, and load records that predate the setting as off. r[chaoscontrol.system_runs.fault_injection.exploration] r[chaoscontrol.system_runs.fault_injection.scenario.recorded]
- [x] [parallel] T7 Give the networking harness the explicit opt-out. r[chaoscontrol.system_runs.fault_injection.scenario.opt_out]
- [x] [parallel] T8 Add positive and negative tests and Nickel fixtures for the flag, the run profile, the bug-report contract, every record, and the legacy load. r[chaoscontrol.system_runs.fault_injection.scenario.exploration] r[chaoscontrol.system_runs.fault_injection.scenario.recorded]

## Phase 2: Verification and completion evidence

- [x] [serial] V1 Build the VMM crate and run the affected tests and gates. r[chaoscontrol.system_runs.fault_injection]
- [ ] [serial] V2 Run the Cairn gates, sync the accepted spec, archive the change, and record the evidence. The installed `cairn` validates this repository's refreshed policy, and `cairn validate --root . --strict` passes with this package active; syncing the spec and archiving remain. r[chaoscontrol.system_runs.fault_injection.scenario.opt_out]
- [x] [serial] V3 Build the workspace, run its tests and Clippy, the product-scope, evidence-contract, and profile-projection checks, and `nix flake check` including the KVM smoke checks. Evidence: `verification.md`. r[chaoscontrol.system_runs.fault_injection.exploration]
