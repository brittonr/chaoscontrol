## MODIFIED Requirements

### Requirement: Replay readiness operator command [r[replay-parent-snapshots.readiness-operator]]
The system MUST expose one operator-facing command and check surface that runs the committed replay readiness gates before any optional selected dogfood proof rail.

#### Scenario: CI/check receipt artifact [r[replay-parent-snapshots.readiness-operator.ci-receipt-artifact]]
- **GIVEN** CI or a local operator builds the replay readiness check surface
- **WHEN** the checks-only readiness rail completes
- **THEN** the check MUST retain a JSON replay readiness receipt artifact and a text file containing the stable one-line summary
- **AND** CI MUST print the summary line and upload both artifacts without launching an implicit slow KVM dogfood workload
