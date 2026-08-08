## ADDED Requirements

### Requirement: Replay readiness operator command [r[replay-parent-snapshots.readiness-operator]]
The system MUST expose one operator-facing command that runs the committed replay readiness gates before any optional selected dogfood proof rail.

#### Scenario: Checks-only readiness [r[replay-parent-snapshots.readiness-operator.checks-only]]
- **GIVEN** an operator wants to know whether the committed replay/evidence slice is ready
- **WHEN** the readiness command runs without a selected dogfood workload
- **THEN** it MUST run the contract registry check, evidence contract check, aggregate replay proof coverage check, generated readiness report check, and dogfood artifact size check
- **AND** it MUST report success only if every static readiness gate succeeds

#### Scenario: Optional selected dogfood [r[replay-parent-snapshots.readiness-operator.selected-dogfood]]
- **GIVEN** static readiness gates pass
- **WHEN** the operator selects one supported workload dogfood rail
- **THEN** the command MUST invoke the matching accepted-verdict dogfood wrapper for only that workload
- **AND** it MUST leave evidence curation and manifest promotion as explicit follow-up work rather than automatically committing outputs

#### Scenario: Slow VM proof remains explicit [r[replay-parent-snapshots.readiness-operator.slow-explicit]]
- **GIVEN** selected dogfood may require KVM and uncached kernel or initrd builds
- **WHEN** an operator reads or invokes the readiness command surface
- **THEN** the default behavior MUST avoid launching slow VM proof runs
- **AND** documentation MUST identify selected dogfood as the slow optional path
