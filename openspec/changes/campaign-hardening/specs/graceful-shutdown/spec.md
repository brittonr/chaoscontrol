## ADDED Requirements

### Requirement: Signal handler installation
The exploration binary SHALL install a signal handler for SIGINT and SIGTERM at process startup (before any KVM VMs are created). The handler SHALL set a global `AtomicBool` flag and return without blocking. The handler SHALL NOT call async-signal-unsafe functions.

#### Scenario: First Ctrl-C sets flag
- **WHEN** the user presses Ctrl-C during exploration
- **THEN** the `SHUTDOWN` atomic flag is set to `true` and the current VM exit completes normally

#### Scenario: Handler coexists with SIGALRM
- **WHEN** SIGINT arrives while a SMP VM's SIGALRM preemption timer is armed
- **THEN** the SIGINT handler runs, sets the flag, and SIGALRM delivery continues unaffected afterward

### Requirement: Explorer checks shutdown flag after each round
The `Explorer::run()` main loop SHALL check the shutdown flag after each round completes. If the flag is set, the explorer SHALL save the checkpoint (if an output directory is configured), emit a `Finished { reason: "interrupted" }` dashboard event, and return the report with results accumulated so far.

#### Scenario: Interrupted mid-campaign
- **WHEN** Ctrl-C is pressed after round 15 of 100
- **THEN** the checkpoint contains 15 rounds of data, the report contains 15 rounds of results, and the process exits with code 0

#### Scenario: No output directory configured
- **WHEN** Ctrl-C is pressed and no `--output` was given
- **THEN** the explorer still returns the accumulated report (printed to stdout) but no checkpoint file is written

### Requirement: Campaign runner checks shutdown flag between seeds
The `CampaignRunner::run()` SHALL check the shutdown flag after each seed completes. If set, it SHALL skip remaining seeds, save `campaign_progress.json` with completed seeds, aggregate partial results into a campaign report, and return.

#### Scenario: Interrupted after 2 of 5 seeds
- **WHEN** Ctrl-C is pressed while seed 3 is running (seeds 1, 2 already complete)
- **THEN** seed 3 finishes its current round and stops, seeds 4 and 5 are skipped, `campaign_progress.json` lists seeds 1-3 as completed, and the campaign report aggregates all three seeds

### Requirement: Second signal forces exit
A second SIGINT or SIGTERM SHALL call `std::process::exit(1)` immediately. This provides an escape hatch if the graceful shutdown path is stuck (e.g., a VM is in an infinite loop that doesn't yield).

#### Scenario: Double Ctrl-C
- **WHEN** the user presses Ctrl-C twice within 1 second
- **THEN** the process exits immediately with code 1 on the second signal

### Requirement: Shutdown reason in reports
The exploration report and campaign report SHALL include the reason for stopping. Valid reasons SHALL include: `"completed"` (all rounds finished), `"frontier_exhausted"`, `"coverage_plateau"`, `"interrupted"` (signal received), `"bug_found"` (short-run early stop).

#### Scenario: Report shows interruption
- **WHEN** exploration is interrupted by Ctrl-C
- **THEN** the text report includes "Stopped: interrupted" and the JSON checkpoint includes `"finish_reason": "interrupted"`
