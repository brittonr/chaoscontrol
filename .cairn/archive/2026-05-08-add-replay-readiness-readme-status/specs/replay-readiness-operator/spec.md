## ADDED Requirements

### Requirement: README replay-readiness status snippet [r[replay-readiness-readme-status]]

The system MUST provide a deterministic README status snippet derived from a validated replay-readiness receipt summary so the repository front page exposes the current bounded readiness claim without requiring CI artifact inspection.

#### Scenario: README status refreshes from a valid receipt [r[replay-readiness-readme-status.refresh]]

- GIVEN a README containing the replay-readiness status marker block and a valid replay-readiness receipt
- WHEN the README status updater is invoked
- THEN only the marker-bounded status block is replaced with a Markdown snippet containing the stable summary line and bounded scope language

#### Scenario: README status updater fails closed [r[replay-readiness-readme-status.fail-closed]]

- GIVEN a malformed receipt or a README missing the marker block
- WHEN the README status updater is invoked
- THEN it exits nonzero and does not emit a misleading status update
