# Exploration Events Specification

## Purpose

Defines the canonical ChaosControl requirements for exploration events.

## Requirements
### Requirement: DashboardEvent enum
The explorer SHALL define a `DashboardEvent` enum representing all event types emittable to the dashboard. Each variant SHALL be serializable to JSON via serde.

#### Scenario: Round complete event
- **WHEN** the explorer finishes a round
- **THEN** it emits `DashboardEvent::RoundComplete` containing: round number, branches run, new edges, cumulative edges, bugs found this round, frontier size, corpus size, and assertion stats snapshot

#### Scenario: Bug found event
- **WHEN** the explorer discovers a new bug
- **THEN** it emits `DashboardEvent::BugFound` containing: bug index, assertion ID, assertion message, round, tick, and fault schedule length

#### Scenario: Exploration started event
- **WHEN** the explorer begins (after bootstrap)
- **THEN** it emits `DashboardEvent::Started` containing: config summary (num_vms, seed, branch_factor, ticks_per_branch, max_rounds, mode), kernel path, and catalog size

#### Scenario: Exploration finished event
- **WHEN** the explorer finishes all rounds or stops early
- **THEN** it emits `DashboardEvent::Finished` containing: total rounds, total branches, total edges, total bugs, and reason (completed, frontier exhausted, bug found in short run)

### Requirement: Event sink in Explorer
The Explorer struct SHALL accept an optional event channel via `Explorer::set_event_sink(sender: std::sync::mpsc::SyncSender<DashboardEvent>)`. When no sink is set, no events are emitted and no channel is allocated.

#### Scenario: Dashboard disabled
- **WHEN** no event sink is set on the explorer
- **THEN** the explorer runs identically to current behavior with no overhead

#### Scenario: Dashboard enabled
- **WHEN** an event sink is set
- **THEN** the explorer calls `try_send()` at each event point
- **AND** if the channel is full, the event is dropped silently (no blocking, no error)

### Requirement: State snapshot for REST endpoint
The explorer SHALL expose a method `Explorer::snapshot_state() -> DashboardState` that returns the current cumulative exploration state. This SHALL be callable from the dashboard server thread via a shared reference.

#### Scenario: Snapshot during exploration
- **WHEN** the dashboard server calls `snapshot_state()`
- **THEN** it receives a `DashboardState` containing: rounds completed, total branches, total edges, bugs list, corpus size, assertion stats, assertion details, round history, network stats, and exploration config

#### Scenario: Snapshot serialization
- **WHEN** `DashboardState` is serialized to JSON
- **THEN** the output matches the schema expected by `GET /api/state`

### Requirement: DashboardState constructable from checkpoint
`DashboardState` SHALL be constructable from a saved `ExplorationCheckpoint` and `assertions.json` file for standalone mode.

#### Scenario: Load from checkpoint
- **WHEN** standalone mode loads `checkpoint.json` and `assertions.json`
- **THEN** it constructs a valid `DashboardState` with round history, assertion details, bug list, and coverage stats
