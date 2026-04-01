## ADDED Requirements

### Requirement: Stale-round-limit CLI flag
The `campaign` subcommand SHALL accept `--stale-round-limit <N>` to control how many consecutive rounds with zero new edges and zero new bugs each seed tolerates before stopping. Default SHALL be 10. Setting 0 SHALL disable early stopping.

#### Scenario: Custom stale limit
- **WHEN** `campaign --stale-round-limit 5` is run
- **THEN** each seed stops after 5 consecutive rounds with no new edges or bugs

#### Scenario: Stale limit disabled
- **WHEN** `campaign --stale-round-limit 0` is run
- **THEN** each seed runs all `--rounds` regardless of coverage plateau

### Requirement: Stale-round-limit on run subcommand
The `run` subcommand SHALL also accept `--stale-round-limit <N>` with the same semantics and default as the campaign subcommand.

#### Scenario: Single-seed stale limit
- **WHEN** `run --stale-round-limit 3 --rounds 100` is run and coverage plateaus after round 8
- **THEN** exploration stops at round 11 (3 consecutive stale rounds)

### Requirement: Workers-per-seed in campaign mode
The `campaign` subcommand SHALL accept `--workers-per-seed <N>`. Default SHALL be 0 (auto-compute). Auto-compute SHALL use `max(1, available_cores / (num_seeds × num_vms))`.

#### Scenario: Auto-compute on 32-core machine
- **WHEN** `campaign --campaign-seeds 4 --vms 3` is run on a 32-core machine with `--workers-per-seed 0`
- **THEN** each seed runs with `max(1, 32 / (4 × 3))` = 2 workers

#### Scenario: Explicit workers-per-seed
- **WHEN** `campaign --workers-per-seed 3` is specified
- **THEN** each seed's `ExplorerConfig` has `num_workers = 3`

#### Scenario: Workers-per-seed=1 matches current behavior
- **WHEN** `campaign --workers-per-seed 1` is specified
- **THEN** each seed runs branches sequentially, identical to current default

### Requirement: Seed failure does not abort campaign
When a seed thread panics or returns an error, the campaign runner SHALL record the failure and continue with remaining seeds. The final campaign report SHALL include results from all successful seeds.

#### Scenario: Error return from seed
- **WHEN** seed 44 returns `Err(ExploreError::Vm(...))` due to a kernel load failure
- **THEN** seeds 42, 43, 45, 46 continue, the campaign report includes their results, and seed 44 is listed as failed

## MODIFIED Requirements

### Requirement: Within-seed worker parallelism disabled
Campaign mode SHALL default to `workers-per-seed = 0` (auto-compute) on each seed's `ExplorerConfig`. If the user passes `--workers N` with N > 1 alongside `--campaign-seeds`, the runner SHALL log a warning that `--workers` is ignored in campaign mode and suggest `--workers-per-seed` instead.

#### Scenario: Workers flag ignored in campaign mode
- **WHEN** `--campaign-seeds 5 --workers 4` is specified
- **THEN** a warning is logged suggesting `--workers-per-seed` and each seed uses the auto-computed worker count

#### Scenario: Workers-per-seed takes precedence
- **WHEN** `--workers-per-seed 2` is specified alongside `--workers 4`
- **THEN** each seed uses 2 workers, `--workers` is ignored with a warning
