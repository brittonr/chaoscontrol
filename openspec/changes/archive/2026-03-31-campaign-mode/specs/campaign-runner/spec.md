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
Campaign mode SHALL set `num_workers: 1` on each seed's `ExplorerConfig`. If the user passes `--workers N` with N > 1 alongside `--campaign-seeds`, the runner SHALL log a warning and ignore `--workers`.

#### Scenario: Workers flag ignored in campaign mode
- **WHEN** `--campaign-seeds 5 --workers 4` is specified
- **THEN** a warning is logged and each seed runs with 1 worker (sequential branches)

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
