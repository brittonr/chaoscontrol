## Why

The dashboard frontend has several gaps: bug markers on the coverage chart silently fail (annotation plugin never loaded), bugs are only a header counter with no list or detail, the exploration config is served but never rendered, assertion exercise coverage has no visual indicator, and Chart.js loads from a CDN which breaks offline use. These are all straightforward fixes that make the dashboard actually usable for reviewing exploration results.

## What Changes

- Fix Chart.js annotation plugin — load the plugin script so bug markers render on the coverage chart
- Inline Chart.js + annotation plugin into the HTML (no CDN dependency, works offline since the HTML is already `include_str!`'d into the binary)
- Add a bugs panel showing a list of discovered bugs with assertion message, round, tick, and fault schedule length; clicking a bug scrolls to or highlights its marker on the chart
- Add a config panel showing the exploration configuration (VMs, seed, rounds, branches, ticks, mode, kernel path)
- Add an assertion exercise gauge — a compact progress bar or fraction showing exercised/catalog_size alongside the existing assertion table
- Sort assertion table: failed first, then unexercised, then passed (spec already requires this but verify the frontend does it)

## Capabilities

### New Capabilities

- `dashboard-bugs-panel`: Bugs list panel in the dashboard UI with per-bug detail and chart marker interaction
- `dashboard-config-panel`: Configuration info panel showing exploration parameters
- `dashboard-assertion-gauge`: Visual assertion exercise coverage indicator

### Modified Capabilities

- `dashboard-ui`: Bug markers on coverage chart require the annotation plugin to actually be loaded; Chart.js must be inlined rather than CDN-fetched; assertion table sort order enforced

## Impact

- `crates/chaoscontrol-explore/assets/index.html` — all frontend changes land here
- `crates/chaoscontrol-explore/assets/` — may add vendored JS files if inlining via separate `include_str!` calls
- `crates/chaoscontrol-explore/src/server.rs` — may need new routes if bug detail returns fault schedule info
- `crates/chaoscontrol-dashboard/` — no changes needed (it's a thin wrapper)
- No Rust API changes, no protocol changes, no guest-side changes
