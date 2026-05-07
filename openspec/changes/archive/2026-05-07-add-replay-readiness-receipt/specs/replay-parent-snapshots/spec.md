## MODIFIED Requirements

### Requirement: Replay readiness operator command [r[replay-parent-snapshots.readiness-operator]]
The system MUST expose one operator-facing command that runs the committed replay readiness gates before any optional selected dogfood proof rail.

#### Scenario: Machine-readable receipt [r[replay-parent-snapshots.readiness-operator.receipt]]
- **GIVEN** CI or a dashboard requests a receipt path for a replay readiness invocation
- **WHEN** the readiness command completes or fails after argument parsing
- **THEN** it MUST write a JSON receipt containing the final status, static gate outcomes, selected dogfood workload when any, and the failed phase when applicable
- **AND** it MUST keep receipt emission separate from dogfood evidence curation or manifest promotion
