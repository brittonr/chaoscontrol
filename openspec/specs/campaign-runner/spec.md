## ADDED Requirements

### Requirement: Multi-seed parallel exploration
The campaign runner SHALL launch N independent `Explorer` instances, each with a distinct seed, running concurrently in OS threads. Each Explorer SHALL own its own `SimulationController` instances with independent KVM VM file descriptors. No mutable state SHALL be shared between seeds.

#### Scenario: Seeds run in parallel
- **WHEN** a campaign is launched with `--campaign-seeds 5` and `--seed 42`
- **THEN** 5 Explorer instances run concurrently with seeds 42, 43, 44, 45, 46

#### Scenario: Each seed is independent
- **WHEN** seed 42's exploration modifies its coverage bitmap or fault engine state
- **THEN** seed 43's state is unaffected

#### Scenario: Sequential seeds produce identical results
- **WHEN** seed 42 is run as part of a 5-seed campaign and then run standalone with `chaoscontrol-explore run --seed 42`
- **THEN** both runs find the same bugs and coverage edges

### Requirement: Seed generation
The campaign runner SHALL generate seeds as `base_seed, base_seed+1, ..., base_seed+N-1` where `base_seed` is the `--seed` CLI flag. Alternatively, an explicit seed list SHALL be accepted via `--seeds 42,99,137`.

#### Scenario: Default seed sequence
- **WHEN** `--seed 100 --campaign-seeds 3` is specified
- **THEN** seeds 100, 101, 102 are used

#### Scenario: Explicit seed list
- **WHEN** `--seeds 42,99,137` is specified
- **THEN** exactly those three seeds are used, ignoring `--campaign-seeds`

### Requirement: Bug deduplication across seeds
Bugs SHALL be deduplicated across seeds using the same key as within-seed dedup: `hash(assertion_id, sorted fault type names)`. The campaign report SHALL record which seeds triggered each unique bug.

#### Scenario: Same bug found by multiple seeds
- **WHEN** seeds 42 and 43 both find "leader completeness" violated via `[NetworkPartition, ProcessKill]`
- **THEN** the campaign report contains one bug entry with `found_by_seeds: [42, 43]`

#### Scenario: Different bugs found by different seeds
- **WHEN** seed 42 finds "log matching" violated and seed 43 finds "election safety" violated
- **THEN** the campaign report contains two distinct bug entries

### Requirement: Report aggregation
The campaign runner SHALL produce a `CampaignReport` containing: seeds run, per-seed summaries (rounds, branches, edges, bugs, wall-clock time), deduplicated bugs with seed provenance, merged assertion details (summed counts, worst-case verdict per assertion), and total wall-clock time.

#### Scenario: Per-seed summary table
- **WHEN** a 3-seed campaign completes
- **THEN** the human-readable report contains a table with one row per seed showing rounds, branches, edges, bugs, and elapsed time

#### Scenario: Merged assertion verdicts
- **WHEN** seed 42 sees assertion X pass (100 hits, 100 true) and seed 43 sees assertion X fail (200 hits, 190 true, 10 false)
- **THEN** the merged campaign report shows assertion X as failed with 300 hits, 290 true, 10 false

### Requirement: Output directory structure
Each seed's Explorer SHALL write to `{output}/seed_{N}/`. The campaign runner SHALL write aggregated reports to `{output}/campaign_report.json` and `{output}/campaign_report.txt`.

#### Scenario: Per-seed output isolation
- **WHEN** a campaign runs seeds 42 and 43 with `--output results/`
- **THEN** `results/seed_42/report.txt`, `results/seed_42/assertions.json`, `results/seed_43/report.txt`, `results/seed_43/assertions.json` exist independently

#### Scenario: Campaign-level aggregation
- **WHEN** a campaign completes
- **THEN** `results/campaign_report.json` contains the full `CampaignReport` and `results/campaign_report.txt` contains the human-readable aggregate

### Requirement: CLI subcommand
The campaign runner SHALL be invoked via `chaoscontrol-explore campaign`. It SHALL accept all flags that `run` accepts (kernel, initrd, vms, rounds, branches, ticks, quantum, vcpus, mode, etc.) plus `--campaign-seeds N` (number of seeds) and `--seeds LIST` (explicit comma-separated seed list).

#### Scenario: Minimal campaign invocation
- **WHEN** `chaoscontrol-explore campaign --kernel vmlinux --initrd initrd.gz --campaign-seeds 5 --output results/`
- **THEN** 5 seeds run with default parameters, output goes to `results/`

#### Scenario: Missing output directory
- **WHEN** `--output` is not specified
- **THEN** the campaign runner exits with an error message requiring `--output` for campaign mode

### Requirement: Within-seed worker parallelism disabled
Campaign mode SHALL default to `workers-per-seed = 0` (auto-compute) on each seed's `ExplorerConfig`. If the user passes `--workers N` with N > 1 alongside `--campaign-seeds`, the runner SHALL log a warning that `--workers` is ignored in campaign mode and suggest `--workers-per-seed` instead.

#### Scenario: Workers flag ignored in campaign mode
- **WHEN** `--campaign-seeds 5 --workers 4` is specified
- **THEN** a warning is logged suggesting `--workers-per-seed` and each seed uses the auto-computed worker count

#### Scenario: Workers-per-seed takes precedence
- **WHEN** `--workers-per-seed 2` is specified alongside `--workers 4`
- **THEN** each seed uses 2 workers, `--workers` is ignored with a warning

### Requirement: Progress reporting
The campaign runner SHALL print a summary line to stderr when each seed completes, including the seed value, rounds completed, branches run, edges found, bugs found, and elapsed time.

#### Scenario: Seed completion logged
- **WHEN** seed 42 finishes exploration
- **THEN** a line like `[seed 42] done: 10 rounds, 80 branches, 256 edges, 1 bug (23.4s)` is printed to stderr

#### Scenario: Campaign completion summary
- **WHEN** all seeds complete
- **THEN** a final line summarizes total seeds, total bugs, total unique bugs, and wall-clock time

### Requirement: Memory estimation logging
The campaign runner SHALL log an estimated total memory usage at startup based on `campaign_seeds × num_vms × vm_memory_mb`.

#### Scenario: Memory estimate printed
- **WHEN** a campaign of 5 seeds with 3 VMs of 256 MB each starts
- **THEN** a log line shows "Estimated memory: 3.8 GB (5 seeds × 3 VMs × 256 MB)"

### Requirement: Exit code
The campaign runner SHALL exit with code 0 if any seed found at least one bug, and exit with code 1 if no bugs were found across all seeds.

#### Scenario: Bugs found
- **WHEN** seed 43 finds a bug and seeds 42, 44, 45, 46 find none
- **THEN** exit code is 0

#### Scenario: No bugs found
- **WHEN** all 5 seeds complete without finding any bugs
- **THEN** exit code is 1

## ADDED Requirements

### Requirement: Campaign resume subcommand
The CLI SHALL support `chaoscontrol-explore campaign resume --corpus <dir>` that reads `campaign_progress.json`, skips completed seeds, and runs only remaining seeds. The final report SHALL merge checkpoint results with newly completed seeds.

#### Scenario: Resume after interruption
- **WHEN** `campaign resume --corpus results/` is run and 2 of 5 seeds are complete
- **THEN** only the 3 remaining seeds are launched

#### Scenario: All seeds already complete
- **WHEN** `campaign resume` is run and all seeds are marked complete in the checkpoint
- **THEN** the aggregated report is written without launching any new exploration

### Requirement: Dashboard support in campaign mode
The `campaign` subcommand SHALL accept `--dashboard` and `--dashboard-port` flags, starting a dashboard server that aggregates events across all seeds.

#### Scenario: Campaign with dashboard
- **WHEN** `chaoscontrol-explore campaign --dashboard --campaign-seeds 3 --output results/`
- **THEN** a dashboard server starts and receives events from all seed explorations

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
