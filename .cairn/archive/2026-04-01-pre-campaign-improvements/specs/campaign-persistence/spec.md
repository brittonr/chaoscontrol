## ADDED Requirements

### Requirement: Campaign report saved to disk
The campaign runner SHALL save the aggregated `CampaignReport` to `{output}/campaign_report.json` (machine-readable) and `{output}/campaign_report.txt` (human-readable) upon completion of all seeds.

#### Scenario: JSON report written
- **WHEN** a campaign with `--output results/` completes
- **THEN** `results/campaign_report.json` exists and deserializes to a valid `CampaignReport`

#### Scenario: Human-readable report written
- **WHEN** a campaign completes
- **THEN** `results/campaign_report.txt` contains the formatted output of `format_campaign_report()`

#### Scenario: Partial completion
- **WHEN** a campaign crashes after 3 of 5 seeds complete
- **THEN** no `campaign_report.json` is written (only per-seed checkpoints exist)

### Requirement: Incremental campaign checkpoint
The campaign runner SHALL write a `campaign_progress.json` file to the output directory after each seed completes. This file SHALL record which seeds have finished and their individual results, enabling partial resume.

#### Scenario: Progress file updated after each seed
- **WHEN** seed 42 finishes in a 5-seed campaign
- **THEN** `{output}/campaign_progress.json` is written with seed 42 marked as complete, including its `SeedSummary`

#### Scenario: Progress file accumulates
- **WHEN** seeds 42, 43, and 44 have each finished sequentially in a parallel campaign
- **THEN** `campaign_progress.json` contains entries for all three completed seeds

### Requirement: Campaign resume from checkpoint
The campaign runner SHALL support a `campaign resume` CLI subcommand that reads `campaign_progress.json`, skips already-completed seeds, and runs only the remaining seeds. The final report SHALL merge results from the checkpoint and the newly completed seeds.

#### Scenario: Resume after crash
- **WHEN** `campaign resume --corpus results/` is run and `campaign_progress.json` shows seeds 42, 43 completed
- **AND** the original campaign had seeds 42, 43, 44, 45, 46
- **THEN** only seeds 44, 45, 46 are launched

#### Scenario: Resume with all seeds done
- **WHEN** `campaign resume` is run and all seeds are already marked complete
- **THEN** the runner aggregates existing results and writes `campaign_report.json` without launching any new exploration

#### Scenario: Resume preserves original config
- **WHEN** `campaign resume` is run
- **THEN** the `ExplorerConfig` for remaining seeds matches the original campaign's config (stored in `campaign_progress.json`)

### Requirement: Campaign progress serialization format
`campaign_progress.json` SHALL contain: the full list of seed values, the `ExplorerConfig` (excluding non-serializable fields), and a map of completed seeds to their `SeedSummary` plus individual `ExplorationReport` references (paths to per-seed checkpoint files).

#### Scenario: Serde roundtrip
- **WHEN** `campaign_progress.json` is written and then read back
- **THEN** the deserialized `CampaignProgress` matches the original
