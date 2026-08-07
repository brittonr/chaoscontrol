## Phase 1: Attempt and planning core

- [x] [serial] Define canonical BLAKE3 fault-attempt identity and explicit selected, applicable, rejected, applied, application-failed, and observed transitions. r[chaoscontrol.fault_outcomes.stage_model]
- [x] [serial] Define a pure applicability planner over normalized topology, capability, range, and policy facts with typed plans and rejection reasons. r[chaoscontrol.fault_outcomes.applicability] r[chaoscontrol.fault_outcomes.boundary]
- [x] [serial] Add plain assertions for valid transitions and negative stale, duplicate, out-of-order, overflow, invalid-target, invalid-parameter, and unsupported-capability transitions. r[chaoscontrol.fault_outcomes.validation.core]

## Phase 2: Enforcement adapters

- [x] [serial] Inventory every public `Fault` variant and bind it to one planner, imperative adapter, and optional observation hook or mark it explicitly unsupported. r[chaoscontrol.fault_outcomes.effect_reachability]
- [x] [parallel] Connect disk, network, scheduler, virtual-time, CPU, process, interrupt, and resource fault state to the real path that enforces it. r[chaoscontrol.fault_outcomes.effect_reachability]
- [x] [serial] Change application adapters to return typed applied or application-failed records and prevent missing targets/devices and invalid values from succeeding as no-ops. r[chaoscontrol.fault_outcomes.application]
- [x] [serial] Preserve ordered prior outcomes on later failure and define rollback, non-runnable, or indeterminate behavior for partial multi-operation adapters. r[chaoscontrol.fault_outcomes.application_failure]

## Phase 3: Observation and accounting

- [x] [parallel] Emit attempt-bound observations from actual block, network, schedule, clock, memory, CPU, process, and interrupt consumption points. r[chaoscontrol.fault_outcomes.observation]
- [x] [serial] Replace pre-application `faults_injected`/`faults_fired` updates with stage-specific counters and ordered round outcomes updated only by valid transitions. r[chaoscontrol.fault_outcomes.accounting]
- [x] [serial] Persist pending effect mechanisms, attempt state, counters, and observation ordering through existing engine/simulation snapshot owners. r[chaoscontrol.fault_outcomes.snapshot_state]
- [x] [serial] Update exploration, minimization, replay, dashboard, and evidence consumers to preserve stage distinctions; keep runtime traces Rust-owned and validate compact summaries at the Nickel boundary where applicable. r[chaoscontrol.fault_outcomes.compatibility]

## Phase 4: Conformance evidence

- [x] [parallel] Add one positive application test for every supported fault variant and explicit unsupported tests for every unimplemented variant. r[chaoscontrol.fault_outcomes.validation.variant_matrix]
- [x] [parallel] Add observation tests proving armed effects count as observed only when the real execution or data path consumes them. r[chaoscontrol.fault_outcomes.validation.observation]
- [x] [parallel] Add negative target, vCPU, register-bit, rate, duration, arithmetic, range, missing-device, adapter-failure, and partial-failure tests and prove they do not increment applied or observed counters. r[chaoscontrol.fault_outcomes.validation.negative]
- [x] [serial] Add replay and snapshot tests comparing ordered attempts, stage transitions, counters, pending mechanisms, and observations. r[chaoscontrol.fault_outcomes.validation.replay]
- [x] [serial] Document stage semantics, unsupported behavior, campaign rejection policy, compatibility aliases, and application/observation non-claims. r[chaoscontrol.fault_outcomes.compatibility]
- [x] [serial] Run focused fault/controller/device tests, workspace tests, replay comparisons, Cairn validation, and proposal/design/tasks gates before sync or archive. r[chaoscontrol.fault_outcomes.validation]
