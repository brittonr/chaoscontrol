## ADDED Requirements

### Requirement: Assertion exercise floor for automation
The `run` and `campaign` commands SHALL accept an optional `--min-assertion-exercise <ratio>` flag. The ratio is computed as `exercised_assertions / catalog_size` for a single run or the merged campaign report. Commands SHALL still write their normal report artifacts before evaluating the floor.

#### Scenario: Campaign misses the floor
- **WHEN** a campaign is launched with `--min-assertion-exercise 0.70`
- **AND** the merged campaign report exercises only 60% of cataloged assertions
- **THEN** the campaign report is still written to disk
- **AND** the command exits with a distinct floor-failure status

#### Scenario: Run meets the floor
- **WHEN** a single-seed run is launched with `--min-assertion-exercise 0.50`
- **AND** the resulting report exercises at least 50% of cataloged assertions
- **THEN** the command preserves its normal success or bug-found exit semantics

#### Scenario: Floor skipped when no catalog is available
- **WHEN** `--min-assertion-exercise` is supplied for a guest with `catalog_size = 0`
- **THEN** the command writes a warning explaining that no assertion catalog was available
- **AND** it skips floor evaluation instead of failing silently
