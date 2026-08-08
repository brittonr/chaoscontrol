## ADDED Requirements

### Requirement: Replay readiness status report [r[replay-parent-snapshots.readiness-status]]
ChaosControl MUST publish an operator-facing replay readiness status report generated from committed proof evidence rather than hand-authored claims.

#### Scenario: Report separates readiness surfaces [r[replay-parent-snapshots.readiness-status.surfaces]]
- **GIVEN** the accepted workload proof manifest contains committed proof entries
- **WHEN** the readiness report is generated
- **THEN** it lists supported snapshot-backed replay surfaces from those proofs
- **AND** it separately lists experimental or unproven surfaces without promoting them to supported status

#### Scenario: Stale report fails validation [r[replay-parent-snapshots.readiness-status.stale]]
- **GIVEN** the manifest or report content changes
- **WHEN** the readiness report check runs
- **THEN** it MUST fail unless the committed Markdown report exactly matches the generated content

#### Scenario: Scope anti-claims are preserved [r[replay-parent-snapshots.readiness-status.anti-claims]]
- **GIVEN** the report describes accepted workload proof coverage
- **WHEN** an operator reads the generated status
- **THEN** it MUST state that the evidence is not a mathematical proof and not a universal deterministic hypervisor claim
