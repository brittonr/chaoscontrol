## MODIFIED Requirements

### Requirement: Output directory structure
Each seed's Explorer SHALL write to `{output}/seed_{N}/`. The campaign runner SHALL write aggregated reports to `{output}/campaign_report.json` and `{output}/campaign_report.txt`. A `campaign_progress.json` SHALL be updated after each seed completes for incremental checkpointing.

#### Scenario: Per-seed output isolation
- **WHEN** a campaign runs seeds 42 and 43 with `--output results/`
- **THEN** `results/seed_42/report.txt`, `results/seed_42/assertions.json`, `results/seed_43/report.txt`, `results/seed_43/assertions.json` exist independently

#### Scenario: Campaign-level aggregation
- **WHEN** a campaign completes
- **THEN** `results/campaign_report.json` contains the full `CampaignReport` and `results/campaign_report.txt` contains the human-readable aggregate

#### Scenario: Progress checkpoint exists
- **WHEN** at least one seed has completed
- **THEN** `results/campaign_progress.json` exists and contains the completed seeds' summaries

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
