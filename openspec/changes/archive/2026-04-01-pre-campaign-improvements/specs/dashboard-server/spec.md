## ADDED Requirements

### Requirement: Campaign-mode SSE event types
The dashboard server SHALL support additional SSE event types for campaign mode: `campaign_started`, `seed_started`, `seed_complete`, and `campaign_finished`. These events SHALL coexist with existing per-round/bug events.

#### Scenario: Campaign started event emitted
- **WHEN** a campaign begins with 5 seeds
- **THEN** the server pushes an SSE event with type `campaign_started` containing the seed list and base config

#### Scenario: Seed completion event emitted
- **WHEN** seed 42 finishes its exploration
- **THEN** the server pushes an SSE event with type `seed_complete` containing seed 42's summary

### Requirement: Campaign state in GET /api/state
When running in campaign mode, `GET /api/state` SHALL include campaign-level fields: `mode`, `seeds_total`, `seeds_completed`, per-seed summaries, and the active seed's live state. The single-run response format SHALL be unchanged when not in campaign mode.

#### Scenario: Campaign state mid-run
- **WHEN** 2 of 5 seeds are complete and one is running
- **THEN** `GET /api/state` returns `mode: "campaign"`, `seeds_total: 5`, `seeds_completed: 2`, and live data for the running seed

#### Scenario: Single-run mode backward compatible
- **WHEN** dashboard is used with `run` subcommand
- **THEN** `GET /api/state` returns the existing format with no `mode` or seed fields

### Requirement: Seed attribution on per-round events
In campaign mode, existing SSE event types (`round`, `bug`, `started`, `finished`) SHALL include a `seed` field identifying which seed produced the event.

#### Scenario: Round event tagged with seed
- **WHEN** seed 43 completes round 7
- **THEN** the SSE round event includes `"seed": 43`
