## MODIFIED Requirements

### Requirement: Coverage Tracking [r[coverage-tracking]]

The PropertyOracle MUST distinguish between exercised and unexercised assertions in coverage reports, and the generated assertion-readiness surface MUST preserve gap evidence before any workload is promoted beyond bounded replay proof.

#### Scenario: Accepted assertion category inference [r[coverage-tracking.scenario.accepted-category-inference]]

- GIVEN a committed accepted-proof assertion artifact lacks category metadata for a known workload assertion
- WHEN assertion-readiness status is generated
- THEN the system MUST render a deterministic effective category for that assertion without modifying the committed artifact
- AND the gap detail MUST distinguish inferred categories from categories present in the artifact

#### Scenario: Unknown accepted assertion remains uncategorized [r[coverage-tracking.scenario.unknown-category-fail-closed]]

- GIVEN a committed accepted-proof assertion artifact lacks category metadata and has no deterministic category mapping
- WHEN assertion-readiness status and promotion checks are generated
- THEN the system MUST keep that assertion uncategorized so promotion remains fail-closed until metadata or explicit rationale exists
