## ADDED Requirements

### Requirement: Schedule fingerprinting
The `VcpuScheduler` SHALL accumulate a rolling hash fingerprint of all vCPU transitions (active vCPU index + quantum length) during execution. The fingerprint SHALL be deterministic for a given scheduler seed and execution sequence. The fingerprint SHALL be zero when `num_vcpus == 1`.

#### Scenario: Fingerprint differs for different interleavings
- **WHEN** two branches run from the same snapshot with different scheduler seeds
- **THEN** their scheduler fingerprints MUST differ (with high probability)

#### Scenario: Fingerprint is deterministic
- **WHEN** the same branch is run twice with the same scheduler seed from the same snapshot
- **THEN** the scheduler fingerprints MUST be identical

#### Scenario: Single-vCPU fingerprint is zero
- **WHEN** a branch runs with `num_vcpus == 1`
- **THEN** the scheduler fingerprint MUST be 0

#### Scenario: Fingerprint survives snapshot/restore
- **WHEN** a scheduler snapshot is taken and restored
- **THEN** the restored scheduler's fingerprint MUST equal the original's fingerprint

### Requirement: Schedule fingerprint injected into coverage bitmap
The explorer SHALL inject the schedule fingerprint into the coverage bitmap after each branch completes. The fingerprint SHALL occupy the upper quarter of MAP_SIZE (`[3/4 * MAP_SIZE, MAP_SIZE)`). Assertion-state edges SHALL occupy `[MAP_SIZE/2, 3/4 * MAP_SIZE)`. Code edges SHALL occupy `[0, MAP_SIZE/2)`.

#### Scenario: Different interleavings produce different coverage
- **WHEN** two branches run identical fault schedules but different scheduler seeds (with `num_vcpus > 1`)
- **THEN** their coverage bitmaps MUST differ in the schedule fingerprint region

#### Scenario: Single-vCPU branches produce no schedule coverage
- **WHEN** a branch runs with `num_vcpus == 1`
- **THEN** the schedule fingerprint region of the coverage bitmap MUST be empty

### Requirement: Per-branch schedule variation
When schedule diversity is enabled, the explorer SHALL assign each branch a distinct `ScheduleVariant` containing a scheduler seed, optional strategy override, and optional quantum override. The `ScheduleVariant` SHALL be applied after snapshot restore and before running the branch.

#### Scenario: Branches within a round get different scheduler seeds
- **WHEN** a round runs with `branch_factor = N` and schedule diversity enabled
- **THEN** each of the N branches MUST have a distinct scheduler seed

#### Scenario: Schedule variant is applied after restore
- **WHEN** a branch is run with a `ScheduleVariant`
- **THEN** the controller SHALL re-seed all per-VM schedulers from the variant's seed before calling `run()`

#### Scenario: Schedule diversity disabled
- **WHEN** schedule diversity is disabled (default for `num_vcpus == 1`)
- **THEN** no `ScheduleVariant` SHALL be generated and the scheduler runs with its snapshot-restored state

### Requirement: Schedule mutations in mutator
The `ScheduleMutator` SHALL support three schedule mutation operators: ReSeed (new scheduler seed), QuantumShift (multiply or divide quantum by 2-8×), and StrategyFlip (switch between RoundRobin and Randomized). When schedule diversity is enabled, approximately 30% of mutations SHALL target scheduling parameters.

#### Scenario: ReSeed mutation
- **WHEN** a ReSeed mutation is applied
- **THEN** the `ScheduleVariant` SHALL have a new random scheduler seed and no strategy/quantum overrides

#### Scenario: QuantumShift mutation
- **WHEN** a QuantumShift mutation is applied
- **THEN** the `ScheduleVariant` SHALL have a quantum override that is 2-8× larger or smaller than the base quantum

#### Scenario: StrategyFlip mutation
- **WHEN** a StrategyFlip mutation is applied to a RoundRobin config
- **THEN** the `ScheduleVariant` SHALL override the strategy to Randomized (and vice versa)

### Requirement: Schedule variant recorded in bug reports
Every `BugReport` and `BranchResult` SHALL include the `ScheduleVariant` used for that branch. The replay and minimize tools SHALL use the recorded variant to reproduce the exact interleaving.

#### Scenario: Bug reproduction uses recorded schedule variant
- **WHEN** a bug is reproduced via `chaoscontrol-explore reproduce`
- **THEN** the controller SHALL apply the `ScheduleVariant` from the bug report before running

#### Scenario: Minimizer preserves schedule variant
- **WHEN** the minimizer reduces a fault schedule
- **THEN** the `ScheduleVariant` from the original bug MUST be held fixed across all minimization attempts

### Requirement: Controller schedule variant application
`SimulationController` SHALL provide an `apply_schedule_variant` method that re-seeds all per-VM schedulers from a `ScheduleVariant`. Each VM's scheduler SHALL receive `variant.scheduler_seed + vm_id` as its seed. Strategy and quantum overrides SHALL apply to all VMs.

#### Scenario: Per-VM seed domain separation
- **WHEN** `apply_schedule_variant` is called with seed S on a 3-VM controller
- **THEN** VM 0's scheduler seed SHALL be S+0, VM 1's SHALL be S+1, VM 2's SHALL be S+2

#### Scenario: Quantum override propagation
- **WHEN** a `ScheduleVariant` with `quantum_override = Some(50)` is applied
- **THEN** all VMs' schedulers SHALL use quantum 50 for that branch

### Requirement: Checkpoint serialization of schedule diversity config
Schedule diversity configuration and per-branch `ScheduleVariant` data SHALL be serialized in exploration checkpoints. Resume from checkpoint SHALL restore schedule diversity settings.

#### Scenario: Checkpoint round-trip
- **WHEN** an exploration with schedule diversity enabled is checkpointed and resumed
- **THEN** the resumed exploration SHALL use the same schedule diversity configuration

### Requirement: CLI flags for schedule diversity
The `chaoscontrol-explore run` and `campaign` subcommands SHALL accept a `--schedule-diversity` flag that enables per-branch schedule variation for SMP explorations. When `num_vcpus == 1`, the flag SHALL be accepted but have no effect.

#### Scenario: Flag enables diversity for SMP
- **WHEN** `--schedule-diversity` is passed with `--vcpus 2`
- **THEN** the explorer SHALL generate per-branch `ScheduleVariant`s

#### Scenario: Flag is no-op for single vCPU
- **WHEN** `--schedule-diversity` is passed with `--vcpus 1`
- **THEN** the explorer SHALL run normally with no schedule variation
