## ADDED Requirements

### Requirement: Campaign and run CLI accept named helical scenarios
The `run`, `campaign`, and `campaign resume` flows SHALL accept a named helical scenario family plus basic materialization knobs. At minimum this includes `--scenario <name>`, `--scenario-phase-ticks <n>`, and `--scenario-turns <n>`. Omitting `--scenario` SHALL preserve the existing direct-schedule behavior.

#### Scenario: Campaign uses named scenario generator
- **WHEN** `chaoscontrol-explore campaign --scenario volatile-write-ring --scenario-phase-ticks 200 --scenario-turns 6 ...` is run
- **THEN** each seed materializes its initial fault schedule from the named scenario family before exploration starts

#### Scenario: Resume uses stored scenario config
- **WHEN** `campaign resume --corpus results/` is run for a campaign that was started with `--scenario degraded-io-ring`
- **THEN** the resumed campaign reuses the stored scenario family and knobs from the checkpoint even if the user does not repeat the flags

### Requirement: Reports include scenario family and phase summary
Human-readable and JSON reports for run and campaign modes SHALL record the selected scenario family, the materialization knobs, and a per-phase summary alongside the concrete bug or campaign results.

#### Scenario: Run report shows phase summary
- **WHEN** a single run completes under `network-ring`
- **THEN** `report.txt` identifies the scenario family and includes a per-phase summary
- **AND** the machine-readable report includes the same metadata in structured form

#### Scenario: Campaign report records seed-specific materialization
- **WHEN** a multi-seed campaign runs under a helical scenario family
- **THEN** the aggregated campaign report records the shared scenario config
- **AND** each seed summary records the materialized phase summary used for that seed
