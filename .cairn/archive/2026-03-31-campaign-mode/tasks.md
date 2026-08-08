## 1. Data types

- [x] 1.1 Add `CampaignConfig` struct: `seeds: Vec<u64>`, `base_explorer_config: ExplorerConfig`, `output_dir: String`
- [x] 1.2 Add `SeedSummary` struct: `seed: u64`, `rounds: u64`, `total_branches: u64`, `total_edges: usize`, `bugs_found: usize`, `wall_clock_seconds: f64`
- [x] 1.3 Add `CampaignBug` struct: wraps `SerializableBug` + `found_by_seeds: Vec<u64>`, `first_seed: u64`, `dedup_key: u64`
- [x] 1.4 Add `CampaignReport` struct: `seeds_run`, `seeds_with_bugs`, `total_rounds`, `total_branches`, `bugs: Vec<CampaignBug>`, `per_seed: Vec<SeedSummary>`, `assertion_details: Vec<AssertionDetail>`, `assertion_stats: AssertionStats`, `wall_clock_seconds: f64`. Derive `Serialize, Deserialize`.

## 2. Campaign runner

- [x] 2.1 Create `crates/chaoscontrol-explore/src/campaign.rs` with `CampaignRunner` struct holding `CampaignConfig`
- [x] 2.2 Implement `CampaignRunner::run()`: log memory estimate, spawn one `std::thread::scope` thread per seed, each thread creates an `Explorer` with `num_workers: 1` and seed-specific `output_dir`, calls `explorer.run()`, returns `(seed, ExplorationReport, wall_clock_duration)`
- [x] 2.3 Implement seed generation: if explicit seeds provided use those, else generate `base_seed..base_seed+N`
- [x] 2.4 Print per-seed completion line to stderr as each thread joins
- [x] 2.5 Implement `aggregate_reports()`: merge `Vec<(u64, ExplorationReport)>` into `CampaignReport` — deduplicate bugs by `dedup_key`, merge assertion details by summing counts and taking worst verdict, sum rounds/branches across seeds

## 3. Report formatting

- [x] 3.1 Add `format_campaign_report(report: &CampaignReport) -> String` in `report.rs`: per-seed summary table, merged bugs section, merged assertion verdicts, campaign-level stats
- [x] 3.2 Write `campaign_report.json` (serde) and `campaign_report.txt` (formatted) to output dir
- [x] 3.3 Write merged `assertions.json` to output dir

## 4. CLI integration

- [x] 4.1 Add `Campaign` variant to `Commands` enum in `chaoscontrol-explore.rs` with all shared flags plus `--campaign-seeds N` and `--seeds LIST`
- [x] 4.2 Require `--output` for campaign mode (exit with error if missing)
- [x] 4.3 Warn and ignore `--workers` if > 1 in campaign mode
- [x] 4.4 Wire `Commands::Campaign` match arm: build `CampaignConfig`, call `CampaignRunner::run()`, write reports, set exit code (0 = bugs found, 1 = no bugs)

## 5. Module wiring

- [x] 5.1 Add `pub mod campaign;` to `crates/chaoscontrol-explore/src/lib.rs`
- [x] 5.2 Re-export `CampaignRunner`, `CampaignConfig`, `CampaignReport` from lib

## 6. Tests

- [x] 6.1 Unit test: seed generation (default sequence + explicit list)
- [x] 6.2 Unit test: bug deduplication across seeds (same dedup_key merged, different keys kept separate)
- [x] 6.3 Unit test: assertion merging (sum counts, worst verdict wins)
- [x] 6.4 Unit test: `CampaignReport` serde roundtrip
- [x] 6.5 Unit test: `format_campaign_report` output contains per-seed table, bug list, assertion verdicts
- [x] 6.6 Unit test: memory estimation string format
