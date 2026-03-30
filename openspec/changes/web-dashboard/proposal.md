## Why

Exploration runs produce rich data — coverage growth curves, bug discovery timelines, assertion verdicts, network fault statistics, per-round history — but the only way to see it is after the run finishes (report.txt) or by tailing env_logger output. There's no way to watch an exploration in progress, compare runs, or drill into assertion failures interactively. A live web dashboard makes long-running explorations observable and makes the data useful for decisions (stop early, adjust parameters, investigate a bug).

## What Changes

- New `chaoscontrol-dashboard` crate with an embedded HTTP server that serves a single-page web UI
- Explorer emits structured progress events during execution (round completions, bug discoveries, coverage updates)
- Dashboard reads live event stream via SSE (Server-Sent Events) and renders charts/tables
- Dashboard can also load completed exploration results from `report.txt` / `assertions.json` / `checkpoint.json` for post-hoc analysis
- CLI flag `--dashboard [port]` on `chaoscontrol-explore run` and `resume` to enable the dashboard alongside exploration
- Standalone `chaoscontrol-dashboard serve --corpus <dir>` mode for reviewing past results

## Capabilities

### New Capabilities
- `dashboard-server`: Embedded HTTP server with SSE event stream, static asset serving, and REST endpoints for exploration state
- `dashboard-ui`: Single-page browser UI with coverage curves, bug timeline, assertion heat map, round-by-round progress table, and network stats
- `exploration-events`: Structured event emitter in the explorer that publishes round completions, bug discoveries, and stats snapshots to subscribers

### Modified Capabilities

## Impact

- New crate: `crates/chaoscontrol-dashboard/`
- New dependencies: `axum` (HTTP), `tokio` (async runtime for server only — explorer stays synchronous), `tower-http` (static files/CORS)
- Frontend: vanilla JS + lightweight charting (no npm/node build step — assets embedded via `include_str!` or `rust-embed`)
- Explorer gains a `ProgressSubscriber` trait or channel-based event sink — existing behavior unchanged when dashboard is disabled
- CLI changes: new `--dashboard` flag on explore subcommands, new `chaoscontrol-dashboard` binary
