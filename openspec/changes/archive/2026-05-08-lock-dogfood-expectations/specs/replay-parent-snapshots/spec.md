## MODIFIED Requirements

### Requirement: Replay readiness operator command [r[replay-parent-snapshots.readiness-operator]]
The system MUST expose one operator-facing command and check surface that runs the committed replay readiness gates before any optional selected dogfood proof rail.

#### Scenario: Dogfood expectation drift fails before slow proof [r[replay-parent-snapshots.readiness-operator.dogfood-expectation-drift]]
- **GIVEN** a committed accepted dogfood expectation lockfile exists
- **WHEN** replay readiness validates the generated accepted-verdict wrapper configuration
- **THEN** it MUST fail before selected KVM dogfood execution if wrapper probe defaults, assertion IDs, expected replay class, or workload probe keys differ from the lockfile
- **AND** the diagnostic MUST name the workload and mismatched field

#### Scenario: Dogfood receipts bind expected and observed verdicts [r[replay-parent-snapshots.readiness-operator.dogfood-expectation-receipt]]
- **GIVEN** an operator runs replay readiness with a selected dogfood workload and receipt path
- **WHEN** the dogfood rail emits a compact accepted or failed summary
- **THEN** the receipt MUST include the selected workload's expected verdict and default probe parameters from the lockfile
- **AND** it MUST classify whether the observed accepted status, replay class, seed, and fail-after value match the expectation when those observed fields are available
