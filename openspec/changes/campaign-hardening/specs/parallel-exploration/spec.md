## ADDED Requirements

### Requirement: Worker branch panic recovery
Each branch execution in `WorkerPool::run_branches()` SHALL be wrapped in `std::panic::catch_unwind`. A panicked branch SHALL produce a `BranchResult` with zero coverage, no bugs, no snapshot, and an error flag. The worker's controller SHALL remain usable for subsequent branches.

#### Scenario: Branch panic produces empty result
- **WHEN** branch 3 panics during `run_single_branch`
- **THEN** the result for branch 3 has `coverage` with 0 edges, empty `bugs` vec, `snapshot: None`, and the round continues processing all branch results

#### Scenario: Worker continues after branch panic
- **WHEN** branch 3 panics on worker 1 and then branch 7 is assigned to worker 1
- **THEN** worker 1 restores from the shared snapshot and executes branch 7 normally

#### Scenario: Panic logged with context
- **WHEN** a branch panics
- **THEN** the panic is logged at `error` level with the branch index, worker index, and panic message

### Requirement: Worker bootstrap panic does not crash pool creation
If a worker's bootstrap (kernel boot + setup_complete) panics during `WorkerPool::new()`, the pool SHALL be created with the remaining workers. If all workers fail, pool creation SHALL return an error.

#### Scenario: One bootstrap fails
- **WHEN** 1 of 4 worker bootstraps panics
- **THEN** the pool is created with 3 workers and a warning is logged

#### Scenario: All bootstraps fail
- **WHEN** all 4 worker bootstraps panic
- **THEN** `WorkerPool::new()` returns an error and the explorer falls back to sequential execution
