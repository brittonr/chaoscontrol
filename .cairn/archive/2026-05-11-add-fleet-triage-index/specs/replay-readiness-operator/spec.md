## ADDED Requirements

### Requirement: Static fleet triage index [r[static-fleet-triage-index]]

The system MUST render one or more replay-readiness receipts into a static multi-receipt fleet triage index without claiming hosted service, scheduler, shared decision-store, or full product parity support.

#### Scenario: Index renders multiple receipt summaries [r[static-fleet-triage-index.render]]

- GIVEN one or more valid replay-readiness receipt paths
- WHEN the fleet triage index renderer is invoked with an output path
- THEN it writes a static HTML artifact listing each receipt path, status, selected workload, replay class, replay-parent depth, and stable summary line

#### Scenario: Empty receipt set fails closed [r[static-fleet-triage-index.empty-fails]]

- GIVEN no replay-readiness receipt paths
- WHEN the fleet triage index renderer is invoked
- THEN it exits nonzero instead of emitting an empty or misleading fleet artifact

#### Scenario: Hosted fleet parity remains unpromoted [r[static-fleet-triage-index.anti-overclaim]]

- GIVEN a generated static fleet triage index
- WHEN an operator reviews the artifact
- THEN the artifact states that it is bounded static multi-receipt review, not universal replay evidence, hosted service, scheduler integration, shared decision store, or full Antithesis-style product replacement
