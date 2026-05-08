# Exploration Timing Specification

## Purpose

Defines the canonical ChaosControl requirements for exploration timing.

## Purpose

Track wall-clock timing for exploration runs, rounds, and user-facing reports while preserving compatibility with older checkpoints.
## Requirements
### Requirement: Wall-clock timing in ExplorationReport
`ExplorationReport` SHALL include a `wall_clock_seconds: f64` field measuring total elapsed time from the start of `Explorer::run()` (after config validation, before bootstrap) to the return of the report.

#### Scenario: Single-seed run reports timing
- **WHEN** a single-seed `chaoscontrol-explore run` completes in 47.3 seconds
- **THEN** the text report includes "Wall-clock time: 47.3s" and the checkpoint JSON includes `"wall_clock_seconds": 47.3`

#### Scenario: Timing includes bootstrap
- **WHEN** bootstrap takes 5 seconds and 10 rounds take 40 seconds
- **THEN** `wall_clock_seconds` is approximately 45.0 (bootstrap + exploration)

### Requirement: Per-round wall-clock timing in RoundHistory
`RoundHistory` SHALL include `wall_clock_seconds: f64`, `restore_ms: f64`, `run_ms: f64`, `snapshot_ms: f64`, and `coverage_ms: f64` fields. All timing fields SHALL use `#[serde(default)]` for backward compatibility. The per-phase fields represent the sum of per-branch timings for that round.

#### Scenario: Slow round is visible
- **WHEN** round 5 includes a ProcessRestart fault that triggers a full VM reboot during branch execution, taking 12 seconds, while other rounds take 2 seconds each
- **THEN** the round history table shows round 5 with 12.0s and other rounds with ~2.0s

#### Scenario: Phase breakdown visible in round
- **WHEN** round 3 completes with 4 branches, total restore=20ms, run=800ms, snapshot=40ms, coverage=1.2ms
- **THEN** `RoundHistory` for round 3 SHALL have `restore_ms=20.0`, `run_ms=800.0`, `snapshot_ms=40.0`, `coverage_ms=1.2`

#### Scenario: Load old checkpoint
- **WHEN** a checkpoint from a previous version (without per-phase timing) is loaded via `chaoscontrol-explore resume`
- **THEN** the checkpoint loads successfully with all timing fields defaulting to 0.0

### Requirement: Backward-compatible serialization
New timing fields SHALL use `#[serde(default)]` so that checkpoints saved by older versions (without timing data) can still be loaded. Missing timing fields SHALL default to `0.0`.

#### Scenario: Load old checkpoint
- **WHEN** a checkpoint from a previous version (without `wall_clock_seconds`) is loaded via `chaoscontrol-explore resume`
- **THEN** the checkpoint loads successfully with `wall_clock_seconds` defaulting to 0.0

### Requirement: Timing in report formatting
The human-readable report SHALL include a "Performance" section showing total wall-clock time, throughput rates (branches/sec, edges/sec), and a per-phase time breakdown (percentage of wall time spent in restore, run, snapshot, coverage). The per-round history table SHALL include per-phase columns when any round has non-zero timing data.

#### Scenario: Report with timing and phase breakdown
- **WHEN** exploration completes with 120s wall time, 60% in run, 25% in snapshot, 12% in restore, 3% in coverage
- **THEN** the report performance section shows wall time, throughput, and a breakdown like "Run: 72.0s (60%) | Snapshot: 30.0s (25%) | Restore: 14.4s (12%) | Coverage: 3.6s (3%)"

#### Scenario: Report without timing (resumed from old checkpoint)
- **WHEN** a resumed exploration has 0.0 for pre-checkpoint rounds
- **THEN** the time column shows "—" for rounds with no timing data and the performance section computes rates from post-resume data only

#### Scenario: Round history table columns
- **WHEN** exploration completes with per-phase timing data
- **THEN** the round history table SHALL include columns for Restore, Run, Snapshot, and Coverage times alongside existing columns (Round, Branches, New Edges, Cum. Edges, Bugs, Frontier, Corpus)
