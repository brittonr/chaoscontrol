## ADDED Requirements

### Requirement: Wall-clock timing in ExplorationReport
`ExplorationReport` SHALL include a `wall_clock_seconds: f64` field measuring total elapsed time from the start of `Explorer::run()` (after config validation, before bootstrap) to the return of the report.

#### Scenario: Single-seed run reports timing
- **WHEN** a single-seed `chaoscontrol-explore run` completes in 47.3 seconds
- **THEN** the text report includes "Wall-clock time: 47.3s" and the checkpoint JSON includes `"wall_clock_seconds": 47.3`

#### Scenario: Timing includes bootstrap
- **WHEN** bootstrap takes 5 seconds and 10 rounds take 40 seconds
- **THEN** `wall_clock_seconds` is approximately 45.0 (bootstrap + exploration)

### Requirement: Per-round wall-clock timing in RoundHistory
`RoundHistory` SHALL include a `wall_clock_seconds: f64` field measuring the elapsed time for that round, from snapshot selection through branch execution and result processing.

#### Scenario: Slow round is visible
- **WHEN** round 5 includes a ProcessRestart fault that triggers a full VM reboot during branch execution, taking 12 seconds, while other rounds take 2 seconds each
- **THEN** the round history table shows round 5 with 12.0s and other rounds with ~2.0s

#### Scenario: Round timing in dashboard events
- **WHEN** round 7 completes
- **THEN** the `RoundComplete` dashboard event includes `wall_clock_seconds` for that round

### Requirement: Backward-compatible serialization
New timing fields SHALL use `#[serde(default)]` so that checkpoints saved by older versions (without timing data) can still be loaded. Missing timing fields SHALL default to `0.0`.

#### Scenario: Load old checkpoint
- **WHEN** a checkpoint from a previous version (without `wall_clock_seconds`) is loaded via `chaoscontrol-explore resume`
- **THEN** the checkpoint loads successfully with `wall_clock_seconds` defaulting to 0.0

### Requirement: Timing in report formatting
The human-readable report SHALL display wall-clock time in the summary section. The per-round history table SHALL include a time column when any round has non-zero timing.

#### Scenario: Report with timing
- **WHEN** exploration completes with timing data
- **THEN** the report summary shows "Wall-clock time: 2m 13s" and the round table includes a "Time" column

#### Scenario: Report without timing (resumed from old checkpoint)
- **WHEN** a resumed exploration has 0.0 for pre-checkpoint rounds
- **THEN** the time column shows "—" for rounds with no timing data
