# Auto Minimize Specification

## Purpose

Defines the canonical ChaosControl requirements for auto minimize.

## Requirements
### Requirement: Auto-minimize flag
The `run` and `campaign` subcommands SHALL accept an `--auto-minimize` flag. When set, the explorer SHALL run the delta-debugging minimizer on each discovered bug after exploration completes.

#### Scenario: Auto-minimize after single-seed run
- **WHEN** `run --auto-minimize --output results/` finds 3 bugs
- **THEN** after the exploration report is printed, the minimizer runs on each bug and saves `bug_0_min.json`, `bug_1_min.json`, `bug_2_min.json` alongside the originals

#### Scenario: Auto-minimize after campaign
- **WHEN** `campaign --auto-minimize --output results/` finds 5 unique bugs across 4 seeds
- **THEN** after the campaign report is written, the minimizer runs on each of the 5 deduplicated bugs and saves minimized schedules to `results/minimized/`

### Requirement: Minimized bugs saved alongside originals
Minimized bug reports SHALL be saved as `bug_N_min.json` in the same directory as the original `bug_N.json`. The minimized file SHALL use the same `SerializableBug` format. If minimization fails (bug no longer reproduces), the original SHALL be kept and a warning logged.

#### Scenario: Successful minimization
- **WHEN** `bug_0.json` has 15 faults and minimization reduces it to 3
- **THEN** `bug_0_min.json` contains a schedule with 3 faults and the same `assertion_id`

#### Scenario: Minimization fails to reproduce
- **WHEN** the minimizer cannot reproduce `bug_1.json` (non-determinism or timing sensitivity)
- **THEN** no `bug_1_min.json` is written, a warning is logged, and the original `bug_1.json` is preserved

### Requirement: Minimization respects shutdown flag
If the SIGINT/SIGTERM shutdown flag is set, the auto-minimize pass SHALL be skipped entirely. A message SHALL be logged indicating minimization was skipped due to interruption.

#### Scenario: Ctrl-C during exploration skips minimization
- **WHEN** Ctrl-C is pressed during round 15 and `--auto-minimize` was set
- **THEN** the checkpoint is saved, the report is generated, but minimization does not run

#### Scenario: Ctrl-C during minimization
- **WHEN** Ctrl-C is pressed while minimizing bug 2 of 5
- **THEN** minimized results for bugs 0 and 1 are kept, bugs 2-4 are skipped

### Requirement: Minimization progress reporting
Each bug minimization SHALL log a progress line showing: bug index, original fault count, minimized fault count, and wall-clock time.

#### Scenario: Minimization logged
- **WHEN** bug 0 is minimized from 15 faults to 3 in 8.2 seconds
- **THEN** a log line shows "Minimized bug 0: 15 → 3 faults (8.2s)"

### Requirement: Sequential minimization
Bugs SHALL be minimized sequentially, not in parallel. Each minimization creates its own VMs and runs branches. Parallel minimization would double peak memory usage.

#### Scenario: Minimization runs after all VMs are dropped
- **WHEN** auto-minimize runs after a campaign with 4 seeds × 3 VMs
- **THEN** the campaign's VMs are dropped before minimization starts, so peak memory is max(campaign_vms, minimize_vms) not their sum
