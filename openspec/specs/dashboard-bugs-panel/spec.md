# Dashboard Bugs Panel Specification

## Purpose

Defines the canonical ChaosControl requirements for dashboard bugs panel.

## Requirements
### Requirement: Bugs list panel
The UI SHALL display a panel listing all discovered bugs in a table with columns: bug ID, assertion message, round, tick, and fault schedule length. The panel SHALL be visible whenever at least one bug exists.

#### Scenario: Bugs panel with discoveries
- **WHEN** the exploration has found one or more bugs
- **THEN** the bugs panel is visible with one row per bug showing its ID, assertion message (truncated to 60 chars), round number, tick, and number of faults in its schedule

#### Scenario: No bugs found
- **WHEN** the exploration has found zero bugs
- **THEN** the bugs panel shows an empty-state message "No bugs found"

#### Scenario: Bug discovered during live exploration
- **WHEN** a BugFound SSE event arrives
- **THEN** the bugs panel updates to include the new bug without a full page reload

### Requirement: Bug row links to chart marker
The UI SHALL highlight the corresponding round on the coverage chart when the user clicks a bug row.

#### Scenario: Click bug row
- **WHEN** the user clicks a bug row for a bug discovered at round R
- **THEN** the coverage chart scrolls into view and the annotation marker at round R is visually highlighted
