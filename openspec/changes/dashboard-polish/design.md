## Context

The dashboard is a single-page app served from `crates/chaoscontrol-explore/assets/index.html`, compiled into the binary via `include_str!`. The backend (axum, 294 lines) serves REST + SSE endpoints. The frontend (300 lines) uses Chart.js for the coverage curve and vanilla JS for table rendering.

The frontend references the Chart.js annotation plugin for bug markers but never loads it, so markers silently don't render. Chart.js itself is loaded from `cdn.jsdelivr.net`, which fails offline. Several data fields from `DashboardState` (config, bug list, assertion exercise rate) are served by the API but have no corresponding UI.

## Goals / Non-Goals

**Goals:**
- Fix broken bug markers on coverage chart (load annotation plugin)
- Eliminate CDN dependency (inline all JS)
- Add bugs panel, config panel, assertion exercise gauge
- Keep the single-file `index.html` architecture — everything inlined
- Zero new Rust dependencies

**Non-Goals:**
- Campaign comparison view (multi-seed side-by-side)
- Build toolchain (webpack, vite, npm) — keep it a single HTML file
- Responsive mobile layout
- Bug detail with full fault schedule JSON (the `/api/bugs/:id` endpoint already exists; the UI just needs to show what's in `DashboardBug`)

## Decisions

### Inline Chart.js + annotation plugin as vendored files

The HTML already uses `include_str!` in `server.rs`. Two options:

1. **Inline JS directly in the HTML** — single `<script>` blocks with minified Chart.js (~200KB) and annotation plugin (~30KB)
2. **Separate vendored files** served via `/assets/chart.min.js` etc., fetched by the browser on load

Option 1 keeps the single-file architecture but bloats the HTML from 10KB to 240KB. This is fine — it's served from memory once per page load, no disk I/O, and gzip cuts it to ~60KB on the wire.

**Decision: Option 1** — inline everything in index.html. No new server routes, no file management, consistent with existing pattern.

We'll vendor the minified JS by downloading Chart.js 4.4.7 UMD bundle and chartjs-plugin-annotation 3.1.0 UMD bundle, then embedding them in `<script>` tags at the top of `index.html`.

### Bugs panel layout

The bugs panel shows a table: bug ID, assertion message (truncated), round, tick, faults count. Clicking a row scrolls the coverage chart into view and highlights the corresponding round marker.

Placed in the 2-column grid alongside the assertion table. When bugs exist, the layout becomes:

```
┌──────────────────────────────────┐
│         Coverage Chart           │  (full width)
├────────────────┬─────────────────┤
│  Bugs (new)    │  Assertions     │
├────────────────┴─────────────────┤
│  Config (new)  │  Round History  │
├────────────────┴─────────────────┤
│        Network Stats             │  (full width, if applicable)
└──────────────────────────────────┘
```

### Config panel

Simple key-value pairs rendered from `state.config`. Shows: VMs, seed, mode, rounds, branches/round, ticks/branch, kernel path (basename only).

### Assertion exercise gauge

A compact inline element above the assertion table: a thin progress bar + text like "32/45 exercised (71%)". Uses `assertion_stats.catalog_size`, `passed + failed` as exercised count.

### Assertion sort order

The existing frontend doesn't sort — it renders `assertion_details` in server order. Add a client-side sort: failed → unexercised → passed, then alphabetical within each group.

## Risks / Trade-offs

- **Inline JS size**: Chart.js minified is ~200KB. Acceptable for a dev tool served from localhost. gzip compression is automatic with modern browsers.
- **Vendored JS version pinning**: Chart.js 4.4.7 and annotation 3.1.0 are pinned. Updates require re-downloading and re-embedding. This is fine for a rarely-updated dev dashboard.
- **No build step**: Editing inline minified JS is impractical. We treat it as opaque vendored content — only our ~150 lines of application JS at the bottom changes.
