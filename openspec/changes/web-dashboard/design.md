## Context

ChaosControl's explorer runs synchronously — boot VMs, fork snapshots, run branches, collect coverage, repeat. Progress is logged via `env_logger` (`info!` calls) and the final report is written to `report.txt` + `assertions.json`. Long explorations (200+ rounds, 16 branches each) can run for hours with no visibility into intermediate state.

The explorer already tracks per-round history (`RoundHistory`), per-assertion detail (`AssertionDetail`), and cumulative stats (`ExplorationStats`). The data exists; it just isn't accessible until the run completes.

## Goals / Non-Goals

**Goals:**
- Live visibility into running explorations (coverage growth, bugs found, assertion status)
- Post-hoc review of completed exploration results
- Zero impact on exploration performance when dashboard is disabled
- No Node.js / npm / bundler — the dashboard ships as a single Rust binary with embedded assets
- Works over SSH tunnels (single port, no WebSocket upgrade issues)

**Non-Goals:**
- Controlling the exploration from the dashboard (start/stop/adjust params) — read-only for v1
- Multi-user / authentication — local tool, single user
- Persistent storage / database — reads from filesystem (checkpoint.json, assertions.json, report output dir)
- Mobile-optimized layout

## Decisions

### 1. Server framework: axum + tokio

axum is already well-established in the Rust ecosystem, zero-cost when idle, and has native SSE support via `axum::response::Sse`. tokio is needed only for the dashboard server — the explorer's synchronous execution loop stays unchanged.

Alternative: `tiny_http` (no async runtime) — rejected because SSE requires holding connections open, which blocks threads in a sync server. Would need a thread per connected client.

Alternative: `warp` — similar to axum but less actively maintained, smaller ecosystem.

### 2. Live updates: Server-Sent Events (SSE)

SSE is HTTP/1.1, works through proxies and SSH tunnels without upgrade negotiation, and is natively supported by all browsers via `EventSource`. The explorer pushes events; the dashboard consumes them. One-directional (server → client) is all we need for a read-only dashboard.

Alternative: WebSocket — bidirectional (unnecessary), requires upgrade handshake (breaks some proxies), more complex error handling.

Alternative: Polling — simpler server, but adds latency and unnecessary load during idle periods.

### 3. Frontend: vanilla HTML/JS + Chart.js (CDN) with embedded fallback

No build step. A single `index.html` with inline CSS and JS, embedded in the binary via `include_str!`. Chart.js loaded from CDN with a `<noscript>` / offline fallback (embed a minimal copy or graceful degradation to tables). The page uses `EventSource` for live data and `fetch()` for initial state load.

Alternative: HTMX — good fit for server-rendered partials, but charting requires JS anyway.

Alternative: Leptos/Yew (Rust WASM) — way too heavy for a dashboard. Compile times, WASM bundle size, and complexity are all wrong.

### 4. Explorer ↔ dashboard bridge: channel-based event sink

The explorer gets an `Option<std::sync::mpsc::Sender<DashboardEvent>>`. When set, it sends structured events at round boundaries (not per-exit — that would be too noisy). The dashboard server holds the `Receiver` and fans out to SSE clients. When `None`, zero overhead — no channel allocation, no branching in the hot loop.

The dashboard server runs in a separate thread (spawned by tokio runtime). Explorer thread sends events via `mpsc::Sender::try_send()` (non-blocking, drops if channel full).

Alternative: shared `Arc<Mutex<State>>` — works but polling-based, harder to push to SSE clients efficiently.

### 5. Standalone mode for post-hoc review

`chaoscontrol-dashboard serve --corpus <dir>` reads `checkpoint.json`, `assertions.json`, and `bug_*.json` from the directory, serves them as static JSON endpoints, and renders the same UI without SSE (just a static snapshot). This means the dashboard binary is useful even without a running exploration.

### 6. Embedded in explore binary, not a separate process

`--dashboard [port]` on `chaoscontrol-explore run` starts the server in-process on a background thread. Avoids IPC, shared state files, or process coordination. The standalone `chaoscontrol-dashboard` binary reuses the same server code but without the explorer.

## Risks / Trade-offs

**[tokio in the dependency tree]** → The dashboard pulls in tokio, which is a large dependency. Mitigation: feature-gated behind `dashboard` feature flag on `chaoscontrol-explore`. Default builds don't pay the cost. The standalone `chaoscontrol-dashboard` crate always includes it.

**[Chart.js CDN dependency]** → Offline environments won't load charts. Mitigation: embed a minified Chart.js copy (~200KB) as fallback, served from the binary when CDN fails. Or just embed it always and skip the CDN.

**[SSE connection drops]** → Browser reconnects automatically (EventSource spec), but missed events during disconnect mean stale state. Mitigation: on reconnect, client fetches full current state via REST endpoint (`GET /api/state`), then resumes SSE for incremental updates.

**[Channel backpressure]** → If dashboard server is slow (many clients, slow network), `try_send` drops events. Mitigation: events are snapshots not deltas — each event contains cumulative stats, so a dropped event just means slightly stale display until the next one arrives.
