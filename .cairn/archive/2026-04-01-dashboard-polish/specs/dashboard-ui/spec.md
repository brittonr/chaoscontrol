## MODIFIED Requirements

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

### MODIFIED Requirements

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

### MODIFIED Requirements

### Requirement: Dark color scheme
The UI SHALL use a dark background color scheme suitable for terminal-adjacent tooling. All JavaScript dependencies (Chart.js, annotation plugin) SHALL be inlined in the HTML — no external CDN requests.

#### Scenario: Page load appearance
- **WHEN** the dashboard loads
- **THEN** the page uses a dark background (#12121a) with light text and accent colors for charts

#### Scenario: Offline functionality
- **WHEN** the dashboard is served on a machine without internet access
- **THEN** the page renders correctly with all charts and interactivity functional
