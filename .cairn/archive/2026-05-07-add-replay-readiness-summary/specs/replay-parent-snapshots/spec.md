## MODIFIED Requirements

### Requirement: Replay readiness operator command [r[replay-parent-snapshots.readiness-operator]]
The system MUST expose one operator-facing command that runs the committed replay readiness gates before any optional selected dogfood proof rail.

#### Scenario: Receipt summary consumer [r[replay-parent-snapshots.readiness-operator.summary]]
- **GIVEN** a replay readiness receipt JSON artifact exists
- **WHEN** CI or a dashboard invokes the summary consumer on that receipt
- **THEN** the consumer MUST print one concise operator summary line containing final status, static gate pass count, dogfood status, and failed phase when present
- **AND** malformed or incomplete receipts MUST fail closed instead of producing a successful summary
