## ADDED Requirements

### Requirement: Assertion exercise gauge
The UI SHALL display a compact progress indicator above the assertion table showing the fraction of registered assertions that have been exercised (hit at least once). The gauge SHALL show both a progress bar and a text label.

#### Scenario: Partial exercise coverage
- **WHEN** 32 of 45 assertions have been hit at least once
- **THEN** the gauge shows a progress bar filled to ~71% and text "32 / 45 exercised (71%)"

#### Scenario: Full exercise coverage
- **WHEN** all registered assertions have been hit
- **THEN** the gauge shows a full progress bar with green accent color

#### Scenario: No catalog registered
- **WHEN** the assertion catalog size is 0
- **THEN** the gauge is hidden

#### Scenario: Live updates
- **WHEN** a round completes and the exercised count increases
- **THEN** the gauge updates its bar width and text without a full page reload
