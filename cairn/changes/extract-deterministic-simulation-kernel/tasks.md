## Phase 1: Boundary inventory

- [x] [serial] Classify every `chaoscontrol-vmm` controller, scheduler, network fabric, fault, clock, and snapshot module as core (pure decision) or shell (KVM/machine effect), and record the inventory in the design. r[chaoscontrol.simulation_kernel.pure_core]
- [x] [serial] Define the `ExecutionCommand` and `ExitObservation` DTOs and the shell executor contract they impose. r[chaoscontrol.simulation_kernel.execution_boundary]
- [x] [serial] Add positive fixtures (canonical command/observation sequences for boot, network delivery, fault injection, snapshot) and negative fixtures (unknown observation kind, out-of-order command, malformed DTO). r[chaoscontrol.simulation_kernel.validation.fixtures]

## Phase 2: Core extraction

- [ ] [serial] Create `crates/chaoscontrol-sim-core` with scheduler, network fabric, virtual clock, and fault schedule selection moved from `chaoscontrol-vmm` unchanged in behavior. r[chaoscontrol.simulation_kernel.pure_core]
- [ ] [serial] Move the tick loop and event/trace model so the core emits canonical traces from explicit seed and config inputs only. r[chaoscontrol.simulation_kernel.determinism]
- [x] [serial] Add a purity check proving the core has no KVM, filesystem, clock, environment, or process dependency. r[chaoscontrol.simulation_kernel.validation.purity]
- [ ] [parallel] Relocate pure snapshot state types and validation to the core without semantic change, coordinated with `complete-vm-snapshot-state`. r[chaoscontrol.simulation_kernel.snapshot_model]

## Phase 3: Shell rewiring

- [ ] [serial] Rewire `SimulationController` to request decisions from the core and dispatch effects through the executor contract. r[chaoscontrol.simulation_kernel.execution_boundary]
- [ ] [serial] Implement the KVM executor adapter in `chaoscontrol-vmm`, mapping commands to ioctls and exit reasons to observations. r[chaoscontrol.simulation_kernel.execution_boundary]
- [x] [parallel] Migrate `chaoscontrol-explore` call sites through compatibility adapters without changing CLI output or artifact JSON. r[chaoscontrol.simulation_kernel.compatibility]

## Phase 4: Equivalence and regression evidence

- [ ] [serial] Add seeded trace-equivalence tests proving pre-split and post-split engines emit identical canonical event traces for fixed seed, config, and guest artifacts. r[chaoscontrol.simulation_kernel.determinism]
- [ ] [parallel] Add negative equivalence fixtures proving a mutated seed, config, or artifact produces a detected divergence. r[chaoscontrol.simulation_kernel.validation.fixtures]
- [ ] [serial] Run workspace tests, clippy, KVM smoke gates, evidence contract checks, and Cairn validation/gates before sync or archive. r[chaoscontrol.simulation_kernel.validation]
