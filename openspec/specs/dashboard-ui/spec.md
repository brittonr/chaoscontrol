## ADDED Requirements

### Requirement: Coverage growth chart
The UI SHALL display a line chart showing cumulative unique edges over rounds. The X axis is round number, Y axis is edge count. The chart SHALL update live as new rounds complete.

#### Scenario: Live coverage curve
- **WHEN** the explorer completes round N
- **THEN** the chart adds a data point at (N, cumulative_edges) and redraws

#### Scenario: Static view from past results
- **WHEN** the dashboard loads a completed exploration
- **THEN** the chart renders the full coverage curve from round_history data

### Requirement: Bug discovery timeline
The UI SHALL display bug discoveries as markers on the coverage chart or as a separate timeline. Each marker SHALL show the round number and assertion message on hover. The Chart.js annotation plugin SHALL be loaded so that vertical line annotations render correctly.

#### Scenario: Bug marker on chart
- **WHEN** a bug is discovered at round R
- **THEN** a vertical red dashed line annotation appears at round R on the coverage chart

#### Scenario: Bug hover detail
- **WHEN** the user hovers over a bug marker
- **THEN** a tooltip shows the assertion message, round number, and tick

#### Scenario: Annotation plugin loaded
- **WHEN** the dashboard page loads
- **THEN** the Chart.js annotation plugin is registered and available for chart configuration

### Requirement: Assertion status table
The UI SHALL display a table of all registered assertions with columns: status icon (✓/✗/○), message, kind, hit count, and verdict. Failed assertions SHALL appear first, then unexercised, then passed. Within each group, assertions SHALL be sorted alphabetically by message.

#### Scenario: Failed assertion row
- **WHEN** an assertion has verdict "failed"
- **THEN** its row shows a ✗ icon, red text color, and the hit count

#### Scenario: Unexercised assertion row
- **WHEN** an assertion has verdict "unexercised"
- **THEN** its row shows a ○ icon and gray text

#### Scenario: Sort order
- **WHEN** the assertion table renders
- **THEN** rows are ordered: all failed assertions first, then unexercised, then passed

#### Scenario: Live assertion updates
- **WHEN** a round completes and assertion stats change
- **THEN** the table updates hit counts and verdicts without full page reload

### Requirement: Round progress table
The UI SHALL display a table of per-round history with columns: round, branches run, new edges, cumulative edges, bugs found, frontier size, corpus size.

#### Scenario: Table during active exploration
- **WHEN** the exploration is in progress
- **THEN** the table shows all completed rounds, with the latest round at the top

#### Scenario: Long history scrolling
- **WHEN** there are more than 20 completed rounds
- **THEN** the table is scrollable and shows all rounds (no truncation)

### Requirement: Summary header
The UI SHALL display a summary bar at the top showing: rounds completed, total branches, unique edges, bugs found, corpus size, and elapsed wall-clock time. Values SHALL update live.

#### Scenario: Summary during exploration
- **WHEN** the exploration is running
- **THEN** the header shows current values and a "Running" status indicator

#### Scenario: Summary for completed exploration
- **WHEN** the dashboard shows a completed exploration (standalone mode)
- **THEN** the header shows final values and a "Completed" status indicator

### Requirement: Network stats panel
The UI SHALL display a panel with network fabric statistics when packets have been sent. The panel SHALL show: packets sent, delivered, dropped (partition + loss), corrupted, duplicated, reordered, bandwidth-delayed, and jittered.

#### Scenario: Network stats visible
- **WHEN** the exploration involves networked VMs (packets_sent > 0)
- **THEN** the network stats panel is visible with non-zero counters

#### Scenario: No network traffic
- **WHEN** the exploration has no network traffic (packets_sent == 0)
- **THEN** the network stats panel is hidden

### Requirement: Auto-reconnect on connection loss
The UI SHALL automatically reconnect to the SSE event stream if the connection drops, and reload the full state via `GET /api/state` on reconnection.

#### Scenario: Network interruption
- **WHEN** the SSE connection drops
- **THEN** the browser's EventSource reconnects automatically
- **AND** on reconnect, the UI fetches `GET /api/state` to sync full state before resuming SSE

### Requirement: Dark color scheme
The UI SHALL use a dark background color scheme suitable for terminal-adjacent tooling. All JavaScript dependencies (Chart.js, annotation plugin) SHALL be inlined in the HTML — no external CDN requests.

#### Scenario: Page load appearance
- **WHEN** the dashboard loads
- **THEN** the page uses a dark background (#12121a) with light text and accent colors for charts

#### Scenario: Offline functionality
- **WHEN** the dashboard is served on a machine without internet access
- **THEN** the page renders correctly with all charts and interactivity functional
