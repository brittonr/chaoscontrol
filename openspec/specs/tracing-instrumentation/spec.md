# tracing-instrumentation Specification

## Purpose
TBD - created by archiving change exploration-profiling. Update Purpose after archive.
## Requirements
### Requirement: Profiling feature flag
`chaoscontrol-vmm` and `chaoscontrol-explore` SHALL declare an optional `profiling` cargo feature that adds `tracing` as a dependency. The feature SHALL NOT be enabled by default. When disabled, no `tracing` code SHALL be compiled.

#### Scenario: Default build excludes tracing
- **WHEN** `cargo build -p chaoscontrol-vmm` is run without `--features profiling`
- **THEN** `tracing` SHALL NOT appear in the dependency tree and no tracing spans SHALL be emitted

#### Scenario: Feature flag enables tracing dependency
- **WHEN** `cargo build -p chaoscontrol-vmm --features profiling` is run
- **THEN** `tracing` SHALL appear in the dependency tree and instrumented functions SHALL emit spans

### Requirement: Instrumented functions
When the `profiling` feature is enabled, the following functions SHALL be annotated with `#[tracing::instrument(skip_all)]`:
- `DeterministicVm::run_bounded`
- `DeterministicVm::snapshot`
- `DeterministicVm::snapshot_incremental`
- `DeterministicVm::restore`
- `DeterministicVm::restore_incremental`
- `DeterministicVm::handle_sdk_hypercall`
- `SimulationController::run`
- `SimulationController::snapshot_all`
- `SimulationController::snapshot_all_incremental`
- `SimulationController::restore_all`
- `SimulationController::restore_all_incremental`
- `Explorer::run_branch`

#### Scenario: Spans emitted for snapshot
- **WHEN** `DeterministicVm::snapshot()` is called with `profiling` enabled and a `tracing-subscriber` configured
- **THEN** a span named `snapshot` SHALL be recorded with entry and exit timestamps

#### Scenario: No overhead without feature
- **WHEN** `DeterministicVm::snapshot()` is called without `profiling` feature
- **THEN** no span creation, thread-local access, or conditional branching related to tracing SHALL occur

### Requirement: cfg_attr pattern for zero-cost
Instrumentation SHALL use `#[cfg_attr(feature = "profiling", tracing::instrument(skip_all))]` so that the attribute is entirely absent when the feature is disabled.

#### Scenario: Conditional attribute expansion
- **WHEN** `cargo expand -p chaoscontrol-vmm` is run without `--features profiling`
- **THEN** the expanded `run_bounded` function SHALL contain no tracing-related code

### Requirement: CI check for feature flag
The CI pipeline (nix flake check or equivalent) SHALL include a `cargo check --features profiling` step for both `chaoscontrol-vmm` and `chaoscontrol-explore` to prevent bitrot of the profiling instrumentation.

#### Scenario: Broken profiling feature caught in CI
- **WHEN** a developer removes a function that has a profiling span without updating the feature code
- **THEN** `nix flake check` SHALL fail with a compilation error from the `profiling` feature check
