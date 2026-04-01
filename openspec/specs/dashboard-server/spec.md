## ADDED Requirements

### Requirement: HTTP server with embedded static assets
The dashboard server SHALL serve a single-page HTML application from embedded assets (compiled into the binary via `include_str!` or `rust-embed`). No external file dependencies SHALL be required at runtime.

#### Scenario: Serve index page
- **WHEN** a browser requests `GET /`
- **THEN** the server returns the embedded `index.html` with `Content-Type: text/html` and status 200

#### Scenario: Serve static assets
- **WHEN** a browser requests `GET /assets/<file>`
- **THEN** the server returns the embedded file with the correct MIME type

### Requirement: REST endpoint for current exploration state
The server SHALL expose `GET /api/state` returning the full current exploration state as JSON. This endpoint SHALL return the latest cumulative snapshot, usable for initial page load and reconnection recovery.

#### Scenario: State during active exploration
- **WHEN** a client requests `GET /api/state` while an exploration is running
- **THEN** the server returns JSON containing: rounds completed, total branches, total edges, bugs list, corpus size, assertion stats, assertion details, round history, and network stats

#### Scenario: State for completed exploration (standalone mode)
- **WHEN** the server is running in standalone mode with `--corpus <dir>`
- **AND** the client requests `GET /api/state`
- **THEN** the server returns JSON loaded from `checkpoint.json` and `assertions.json` in the corpus directory

#### Scenario: State before exploration starts
- **WHEN** a client requests `GET /api/state` before any rounds have completed
- **THEN** the server returns JSON with zero values for all counters and empty arrays

### Requirement: SSE event stream for live updates
The server SHALL expose `GET /api/events` as a Server-Sent Events stream. Each event SHALL be a JSON-encoded `DashboardEvent` with an `event` type field and a `data` payload.

#### Scenario: Client connects to event stream
- **WHEN** a browser opens an `EventSource` connection to `GET /api/events`
- **THEN** the server holds the connection open and pushes events as they occur

#### Scenario: Round completion event
- **WHEN** the explorer completes a round
- **THEN** the server pushes an SSE event with type `round` containing the round number, new edges, cumulative edges, bugs found, frontier size, and corpus size

#### Scenario: Bug discovery event
- **WHEN** the explorer discovers a bug
- **THEN** the server pushes an SSE event with type `bug` containing the bug's assertion ID, message, round, and tick

#### Scenario: Multiple clients
- **WHEN** multiple browsers connect to `GET /api/events`
- **THEN** all connected clients SHALL receive the same events

### Requirement: Bug detail endpoint
The server SHALL expose `GET /api/bugs` returning the list of all discovered bugs as JSON, and `GET /api/bugs/:id` returning a single bug's detail including its fault schedule.

#### Scenario: List all bugs
- **WHEN** a client requests `GET /api/bugs`
- **THEN** the server returns a JSON array of bug summaries (id, assertion message, round, tick)

#### Scenario: No bugs found
- **WHEN** a client requests `GET /api/bugs` and no bugs have been discovered
- **THEN** the server returns an empty JSON array `[]`

### Requirement: Dashboard runs on configurable port
The server SHALL listen on a configurable TCP port, defaulting to 8080. The port SHALL be configurable via `--dashboard-port` CLI flag.

#### Scenario: Default port
- **WHEN** the user runs `chaoscontrol-explore run --dashboard` without specifying a port
- **THEN** the server listens on port 8080

#### Scenario: Custom port
- **WHEN** the user runs `chaoscontrol-explore run --dashboard --dashboard-port 9090`
- **THEN** the server listens on port 9090

#### Scenario: Port in use
- **WHEN** the specified port is already in use
- **THEN** the server logs an error with the port number and the exploration continues without the dashboard

### Requirement: Standalone serve mode
A standalone binary `chaoscontrol-dashboard` SHALL support `serve --corpus <dir>` to serve a static dashboard from previously saved exploration results. No running exploration is required.

#### Scenario: Serve from corpus directory
- **WHEN** the user runs `chaoscontrol-dashboard serve --corpus results/`
- **THEN** the server reads `checkpoint.json`, `assertions.json`, and `bug_*.json` from `results/`
- **AND** serves the dashboard UI on the configured port

#### Scenario: Missing corpus files
- **WHEN** the corpus directory does not contain `checkpoint.json`
- **THEN** the server exits with an error message indicating the missing file

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
