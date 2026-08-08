## Why

Coverage-guided exploration plateaus after 1-2 rounds for Raft. The frontier empties fast because the guest's state space is protocol-driven, not code-path-driven. What actually finds rare multi-step bugs (fig8_commit needing 5 nodes, leader completeness under specific message orderings) is brute-force: many independent seeds running long. Today there's no way to orchestrate this — you'd manually launch N processes and manually aggregate the results.

## What Changes

- New `campaign` CLI subcommand on `chaoscontrol-explore` that launches N independent exploration runs with different seeds in parallel, aggregates bugs and coverage across all seeds, and produces a unified report.
- New `CampaignRunner` in `chaoscontrol-explore` that manages seed-level parallelism (distinct from the existing within-round `WorkerPool` which parallelizes branches within a single seed).
- New `CampaignReport` that merges `ExplorationReport`s across seeds: deduplicated bugs, union coverage, per-seed summaries, and a "which seed found what" timeline.
- Aggregate `assertions.json` and `campaign_report.json` machine-readable outputs for CI.

## Capabilities

### New Capabilities
- `campaign-runner`: Orchestrates multiple independent exploration runs across different seeds, manages process-level parallelism, and aggregates results into a unified report.

### Modified Capabilities
(none — campaign mode composes existing exploration; it does not change the Explorer or WorkerPool internals)

## Impact

- `crates/chaoscontrol-explore/src/campaign.rs` — new module
- `crates/chaoscontrol-explore/src/bin/chaoscontrol-explore.rs` — new `Campaign` subcommand variant
- `crates/chaoscontrol-explore/src/report.rs` — new `format_campaign_report()` function
- `crates/chaoscontrol-explore/src/lib.rs` — re-export campaign module
- No changes to `chaoscontrol-vmm`, `chaoscontrol-fault`, `chaoscontrol-sdk`, or guest crates.
- KVM file descriptors are per-process safe; each seed's Explorer gets its own controllers. No shared mutable state between seeds.
