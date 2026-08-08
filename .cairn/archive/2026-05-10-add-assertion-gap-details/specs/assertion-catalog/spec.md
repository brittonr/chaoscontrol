## MODIFIED Requirements

### Requirement: Coverage Tracking [r[coverage-tracking]]

The PropertyOracle MUST distinguish between exercised and unexercised assertions in coverage reports, and the generated assertion-readiness surface MUST preserve gap evidence before any workload is promoted beyond bounded replay proof.

#### Scenario: Exercised assertion tracking [r[coverage-tracking.exercised]]

- GIVEN an assertion is registered in the catalog and fires during execution
- WHEN a coverage report is generated
- THEN the assertion MUST be marked as exercised with execution details

#### Scenario: Unexercised assertion reporting [r[coverage-tracking.unexercised]]

- GIVEN an assertion is registered in the catalog but never fires
- WHEN a coverage report is generated
- THEN the assertion MUST be reported as unexercised with catalog metadata

#### Scenario: Assertion readiness gaps remain promotion blockers [r[coverage-tracking.promotion-blockers]]

- GIVEN an accepted workload proof has unhit, uncategorized, or non-passing assertion gaps
- WHEN assertion-readiness status or promotion checks are generated
- THEN the system MUST report those gaps as promotion blockers unless explicit workload-specific rationale is present
- AND the workload MUST NOT be described as richer-than-bounded replay support solely because its accepted proof exercised cataloged assertions

#### Scenario: Assertion readiness gap details identify remediation targets [r[coverage-tracking.gap-details]]

- GIVEN an accepted workload proof has unhit or non-passing assertions
- WHEN assertion-readiness status is generated
- THEN the system MUST include deterministic gap details that identify the workload, gap class, assertion ID or message, kind, category, verdict, and hit count when those fields are present
- AND the details MUST be derived from committed accepted-proof assertion artifacts rather than fresh VM execution
