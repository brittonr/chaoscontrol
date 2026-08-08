# Parallel Exploration Specification

## Purpose

Defines the canonical ChaosControl requirements for parallel exploration.

## Requirements
### Requirement: Parallel branch execution
The explorer SHALL execute branches within a single round concurrently
across multiple OS threads. Each thread SHALL own its own
`SimulationController` with independent KVM VM file descriptors.

#### Scenario: Branches run on separate threads
- **WHEN** a round has 8 branches and 4 worker threads
- **THEN** up to 4 branches execute simultaneously and the round
  completes in approximately the wall-clock time of 2 sequential
  branches (plus overhead)

#### Scenario: Results are deterministic regardless of parallelism
- **WHEN** the same exploration seed is run with 1 worker and then
  with 4 workers
- **THEN** the set of bugs found, coverage edges discovered, and
  final assertion verdicts are identical

### Requirement: Worker pool with bootstrapped controllers
Workers SHALL be pre-bootstrapped: each boots the kernel and runs to
`setup_complete` once at pool creation time, then reuses its controller
via snapshot restore for every branch. Kernel boot SHALL NOT occur per
branch.

#### Scenario: Pool amortizes boot cost
- **WHEN** a pool of 4 workers is created
- **THEN** 4 kernel boots occur at startup and zero kernel boots occur
  during the exploration rounds

#### Scenario: Worker controller reuse
- **WHEN** a worker finishes branch B1 and starts branch B2
- **THEN** it restores the shared snapshot on its existing controller
  without creating a new VM

### Requirement: Shared snapshot distribution
The snapshot taken after bootstrap SHALL be shared with all workers.
Workers SHALL receive a read-only reference to the base memory and
reconstruct their overlay independently.

#### Scenario: Snapshot shared without full copy per worker
- **WHEN** the bootstrap snapshot is distributed to 4 workers
- **THEN** the base memory is reference-counted, not copied 4 times

### Requirement: Coverage and bug merging
After all branches in a round complete, the explorer SHALL merge
coverage bitmaps and bug reports from all workers into the global state.
The merge order SHALL be deterministic (sorted by branch index).

#### Scenario: Coverage merge after parallel round
- **WHEN** 4 branches complete in parallel, each discovering different
  edges
- **THEN** the global coverage bitmap is the union of all 4 branch
  bitmaps and `new_coverage_edges` reflects the total new edges found

#### Scenario: Bug reports collected from all workers
- **WHEN** branches 2 and 3 each find a bug
- **THEN** both bugs appear in the round report, ordered by branch
  index

### Requirement: Configurable worker count
The number of worker threads SHALL be configurable via CLI flag
(`--workers N`). Default SHALL be 1 (sequential, matching current
behavior). Setting `--workers 0` SHALL auto-detect based on available
cores.

#### Scenario: Default is sequential
- **WHEN** `--workers` is not specified
- **THEN** exploration runs sequentially (1 worker), producing
  identical results to the current implementation

#### Scenario: Auto-detect workers
- **WHEN** `--workers 0` is specified on a 16-core machine
- **THEN** the explorer uses a heuristic based on available cores
  (e.g., cores / VMs-per-simulation, capped at branch_factor)

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
