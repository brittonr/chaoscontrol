# Campaign Dashboard Specification

## Purpose

Defines the canonical ChaosControl requirements for campaign dashboard.
## Requirements
### Requirement: Dashboard available in campaign mode
The `campaign` subcommand SHALL accept `--dashboard` and `--dashboard-port` flags. When `--dashboard` is set, a single dashboard server SHALL start before any seeds launch and remain running until all seeds complete.

#### Scenario: Dashboard flag on campaign
- **WHEN** `chaoscontrol-explore campaign --dashboard --kernel vmlinux --initrd initrd.gz --campaign-seeds 3 --output results/`
- **THEN** a dashboard server starts on the default port and serves live campaign progress

#### Scenario: No dashboard by default
- **WHEN** `--dashboard` is not passed to `campaign`
- **THEN** no HTTP server is started

### Requirement: Campaign-level SSE events
The dashboard server SHALL emit campaign-specific SSE events in addition to per-seed events. Campaign events SHALL include `campaign_started`, `seed_started`, `seed_complete`, and `campaign_finished`.

#### Scenario: Campaign started event
- **WHEN** the campaign begins
- **THEN** an SSE event with type `campaign_started` is pushed containing the seed list, total seeds, and base config summary

#### Scenario: Seed completion event
- **WHEN** seed 42 finishes exploration
- **THEN** an SSE event with type `seed_complete` is pushed containing seed 42's summary (rounds, branches, edges, bugs, elapsed time)

#### Scenario: Campaign finished event
- **WHEN** all seeds complete
- **THEN** an SSE event with type `campaign_finished` is pushed containing the deduplicated bug count and total wall-clock time

### Requirement: Campaign state in API
`GET /api/state` SHALL return campaign-level fields when running in campaign mode: `mode: "campaign"`, `seeds_total`, `seeds_completed`, `seeds_running`, per-seed summaries for completed seeds, and the currently active seed's live round-level state.

#### Scenario: Mid-campaign state request
- **WHEN** seed 42 is done, seed 43 is running at round 5
- **THEN** `GET /api/state` returns `seeds_completed: 1`, `seeds_running: 1`, seed 42's summary, and seed 43's live round history

#### Scenario: Single-run mode unchanged
- **WHEN** dashboard is used with `run` subcommand (not `campaign`)
- **THEN** `GET /api/state` returns the existing single-run format with no campaign fields

### Requirement: Per-seed event forwarding
Each seed's Explorer SHALL send its `DashboardEvent`s (Started, RoundComplete, BugFound, Finished) through the campaign's shared dashboard channel. Events SHALL be tagged with a `seed` field so the dashboard can attribute them.

#### Scenario: Round event from specific seed
- **WHEN** seed 43 completes round 5
- **THEN** the SSE event includes `"seed": 43` alongside the round data

#### Scenario: Bug from specific seed
- **WHEN** seed 42 finds a bug
- **THEN** the SSE bug event includes `"seed": 42`

### Requirement: Local multi-hypervisor dashboard [r[campaign-dashboard.local-multi-hypervisor]]

The campaign dashboard MUST render a static or local-only operator view for one-machine multi-hypervisor campaigns from validated queue, worker, run, bug, and receipt artifacts.

#### Scenario: Dashboard shows local worker and queue state [r[campaign-dashboard.local-multi-hypervisor.queue-workers]]

- GIVEN a valid local multi-hypervisor control-plane receipt and queue-state snapshot
- WHEN the dashboard renderer runs
- THEN it shows campaign status, worker IDs, resource budgets when present, leased/running/completed queue entries, run receipt summaries, bug counts, and reproduce/minimize follow-up status

#### Scenario: Dashboard preserves local-only scope [r[campaign-dashboard.local-multi-hypervisor.scope]]

- GIVEN the dashboard is rendered for a local multi-hypervisor campaign
- WHEN an operator reviews it
- THEN it states that the evidence covers one-machine local hypervisor orchestration only and does not claim SaaS, remote shared queues, cross-machine scheduling, or universal fleet throughput
