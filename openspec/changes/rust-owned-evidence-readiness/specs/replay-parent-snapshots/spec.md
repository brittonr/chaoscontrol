## MODIFIED Requirements

### Requirement: Replay readiness operator command [r[replay-parent-snapshots.readiness-operator]]

The system MUST expose one operator-facing command and check surface that runs Rust-owned committed replay readiness gates before any optional selected dogfood proof rail.

#### Scenario: Checks-only readiness [r[replay-parent-snapshots.readiness-operator.checks-only]]

- **GIVEN** an operator wants to know whether the committed replay/evidence slice is ready
- **WHEN** the readiness command runs without a selected dogfood workload
- **THEN** it MUST run the contract registry check, evidence contract check, aggregate replay proof coverage check, generated replay readiness report check, generated assertion readiness report check, and dogfood artifact size check
- **AND** migrated structured evidence/readiness policy MUST be evaluated by Rust-owned validators rather than Python or Bash proof-policy scripts
- **AND** it MUST report success only if every static readiness gate succeeds

### Requirement: Replay readiness status report [r[replay-parent-snapshots.readiness-status]]

ChaosControl MUST publish operator-facing replay readiness and replay proof coverage status reports generated from committed proof evidence rather than hand-authored claims.

#### Scenario: Stale report fails validation [r[replay-parent-snapshots.readiness-status.stale]]

- **GIVEN** the manifest or any generated readiness/coverage report content changes
- **WHEN** the Rust readiness or proof coverage report check runs
- **THEN** it MUST fail unless the committed Markdown report exactly matches the generated content
- **AND** `docs/replay-proof-coverage.md`, `docs/replay-readiness-status.md`, and `docs/assertion-readiness-status.md` must derive supported workload rows from the same accepted proof manifest
