# exploration-throughput-metrics Specification

## Purpose
TBD - created by archiving change exploration-profiling. Update Purpose after archive.
## Requirements
### Requirement: Per-branch timing breakdown
Each branch execution SHALL record a `BranchTimings` struct with `restore_ms`, `run_ms`, `snapshot_ms`, and `coverage_ms` fields (all `f64`), measuring wall-clock time for each phase using `std::time::Instant`.

#### Scenario: Branch with expensive snapshot
- **WHEN** a branch runs against a VM with 10MB of dirty pages
- **THEN** `BranchTimings.snapshot_ms` reflects the higher snapshot cost while `run_ms` and `coverage_ms` remain comparable to branches with fewer dirty pages

#### Scenario: Branch timings sum to wall time
- **WHEN** a branch completes with restore=5ms, run=200ms, snapshot=10ms, coverage=0.3ms
- **THEN** the sum of timing fields (215.3ms) SHALL be within 5% of the measured total branch wall time

### Requirement: Per-round timing aggregation in RoundHistory
`RoundHistory` SHALL include `restore_ms`, `run_ms`, `snapshot_ms`, and `coverage_ms` fields (all `f64`, `#[serde(default)]`) containing the sum of per-branch timings for that round.

#### Scenario: Round aggregates branch timings
- **WHEN** a round executes 8 branches with individual `run_ms` values of [200, 210, 195, 220, 205, 215, 190, 225]
- **THEN** `RoundHistory.run_ms` SHALL equal 1660.0 (the sum)

#### Scenario: Old checkpoint without timing fields
- **WHEN** a checkpoint saved by a version without per-phase timing is loaded
- **THEN** `restore_ms`, `run_ms`, `snapshot_ms`, and `coverage_ms` SHALL all default to 0.0

### Requirement: JSON-lines metrics output
When `--emit-metrics` is passed to `chaoscontrol-explore run`, the explorer SHALL write one JSON object per completed round to stderr (or to the file specified by `--metrics-file <path>`). Each object SHALL contain: `round`, `branches`, `new_edges`, `cumulative_edges`, `bugs_found`, `restore_ms`, `run_ms`, `snapshot_ms`, `coverage_ms`, `wall_ms`.

#### Scenario: Metrics to stderr
- **WHEN** `chaoscontrol-explore run --emit-metrics --rounds 3` completes
- **THEN** stderr contains exactly 3 JSON lines, each parseable as a JSON object with all required fields

#### Scenario: Metrics to file
- **WHEN** `chaoscontrol-explore run --emit-metrics --metrics-file /tmp/metrics.jsonl --rounds 5` completes
- **THEN** `/tmp/metrics.jsonl` contains exactly 5 JSON lines and stderr does not contain metrics output

#### Scenario: Metrics disabled by default
- **WHEN** `chaoscontrol-explore run` is invoked without `--emit-metrics`
- **THEN** no JSON metrics lines SHALL be written to stderr or any file

### Requirement: Throughput rates in ExplorationReport
`ExplorationReport` SHALL include `branches_per_second` and `edges_per_second` fields (`f64`) computed as `total_branches / wall_clock_seconds` and `total_edges / wall_clock_seconds` respectively.

#### Scenario: Throughput computation
- **WHEN** exploration completes with 800 branches, 1200 edges, in 120.0 seconds
- **THEN** `branches_per_second` SHALL be approximately 6.67 and `edges_per_second` SHALL be approximately 10.0

#### Scenario: Zero wall time
- **WHEN** `wall_clock_seconds` is 0.0 (e.g., resumed from old checkpoint with no timing)
- **THEN** both throughput fields SHALL be 0.0 (not infinity or NaN)
