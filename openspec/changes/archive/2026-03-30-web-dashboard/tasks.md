## 1. Exploration Events

- [x] 1.1 Define `DashboardEvent` enum (Started, RoundComplete, BugFound, Finished) with serde Serialize in `chaoscontrol-explore`
- [x] 1.2 Define `DashboardState` struct (cumulative snapshot of all exploration data) with serde Serialize/Deserialize
- [x] 1.3 Add `Option<SyncSender<DashboardEvent>>` field to `Explorer`, wire `set_event_sink()` method
- [x] 1.4 Emit `Started` event after bootstrap, `RoundComplete` after each round, `BugFound` on discovery, `Finished` at end
- [x] 1.5 Implement `snapshot_state()` on Explorer returning `DashboardState`
- [x] 1.6 Implement `DashboardState::from_checkpoint()` to construct state from saved checkpoint + assertions files
- [x] 1.7 Unit tests: event emission, snapshot_state content, from_checkpoint roundtrip

## 2. Dashboard Server Crate

- [x] 2.1 Create `crates/chaoscontrol-dashboard/` with Cargo.toml (axum, tokio, tower-http, serde_json deps)
- [x] 2.2 Implement `DashboardServer` struct holding shared state (`Arc<RwLock<DashboardState>>`) and event broadcast channel
- [x] 2.3 Implement `GET /` route serving embedded `index.html` via `include_str!`
- [x] 2.4 Implement `GET /api/state` route returning `DashboardState` as JSON
- [x] 2.5 Implement `GET /api/events` SSE route using `axum::response::Sse`, fan out events from broadcast channel
- [x] 2.6 Implement `GET /api/bugs` and `GET /api/bugs/:id` routes
- [x] 2.7 Implement event receiver loop: read from `mpsc::Receiver<DashboardEvent>`, update shared state, broadcast to SSE clients
- [x] 2.8 Implement `start()` method that spawns tokio runtime on a background thread, returns `SyncSender<DashboardEvent>`
- [x] 2.9 Handle port-in-use gracefully (log error, return None from start)
- [x] 2.10 Unit tests: state serialization, SSE event formatting, route responses

## 3. Standalone Binary

- [x] 3.1 Create `src/bin/chaoscontrol-dashboard.rs` with clap CLI: `serve --corpus <dir> [--port <port>]`
- [x] 3.2 Implement corpus directory loading: read checkpoint.json, assertions.json, bug_*.json
- [x] 3.3 Construct `DashboardState` from loaded files, start server without SSE events
- [x] 3.4 Error handling for missing/malformed corpus files

## 4. CLI Integration

- [x] 4.1 Add `--dashboard` and `--dashboard-port <port>` flags to `chaoscontrol-explore run` and `resume` subcommands
- [x] 4.2 Feature-gate dashboard dependency behind `dashboard` feature flag on `chaoscontrol-explore`
- [x] 4.3 When `--dashboard` is set: start `DashboardServer`, pass `SyncSender` to explorer via `set_event_sink()`
- [x] 4.4 Log dashboard URL on startup: "Dashboard: http://localhost:{port}"

## 5. Frontend UI

- [x] 5.1 Create `index.html` with dark theme layout: header summary bar, main content area with chart + tables
- [x] 5.2 Embed Chart.js (minified, ~200KB) as inline script for offline support
- [x] 5.3 Implement coverage growth line chart (X: round, Y: cumulative edges) with bug discovery markers
- [x] 5.4 Implement assertion status table: icon (✓/✗/○), message, kind, hit count, verdict — sorted failed-first
- [x] 5.5 Implement round progress table: round, branches, new edges, cum. edges, bugs, frontier, corpus — latest on top
- [x] 5.6 Implement network stats panel (hidden when packets_sent == 0)
- [x] 5.7 Implement summary header: rounds, branches, edges, bugs, corpus, status indicator (Running/Completed)
- [x] 5.8 Wire `EventSource` to `/api/events`, update all components on each event
- [x] 5.9 Implement reconnect logic: on SSE disconnect, fetch `/api/state` then resume event stream
- [x] 5.10 Bug hover tooltips on chart markers (assertion message, kind, round, tick)

## 6. Workspace Integration

- [x] 6.1 Add `chaoscontrol-dashboard` to workspace Cargo.toml members
- [x] 6.2 Add `dashboard` feature flag to `chaoscontrol-explore/Cargo.toml` with optional dep on `chaoscontrol-dashboard`
- [x] 6.3 Update README with dashboard section (usage, screenshots placeholder)
- [x] 6.4 Add `nix run .#dashboard` to flake.nix for standalone mode
