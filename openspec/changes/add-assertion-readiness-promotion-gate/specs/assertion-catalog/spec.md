## MODIFIED Requirements

### Requirement: Coverage Tracking [r[assertion-catalog.coverage-tracking]]

The PropertyOracle MUST distinguish between exercised and unexercised assertions in coverage reports, and the generated assertion-readiness surface MUST preserve gap evidence before any workload is promoted beyond bounded replay proof.

#### Scenario: Exercised assertion tracking [r[assertion-catalog.coverage-tracking.exercised]]

- GIVEN an assertion is registered in the catalog and fires during execution
- WHEN a coverage report is generated
- THEN the assertion MUST be marked as exercised with execution details

#### Scenario: Unexercised assertion reporting [r[assertion-catalog.coverage-tracking.unexercised]]

- GIVEN an assertion is registered in the catalog but never fires
- WHEN a coverage report is generated
- THEN the assertion MUST be reported as unexercised with catalog metadata

#### Scenario: Assertion readiness gaps remain promotion blockers [r[assertion-catalog.coverage-tracking.readiness-gap-promotion-blocker]]

- GIVEN an accepted workload proof has unhit, uncategorized, or non-passing assertion gaps
- WHEN assertion-readiness status or promotion checks are generated
- THEN the system MUST report those gaps as promotion blockers unless explicit workload-specific rationale is present
- AND the workload MUST NOT be described as richer-than-bounded replay support solely because its accepted proof exercised cataloged assertions

## ADDED Requirements

### Requirement: Assertion-readiness promotion gate [r[assertion-readiness-promotion-gate]]

The static readiness surface MUST fail closed when assertion-readiness evidence is weakened, hidden, or promoted beyond the accepted workload's documented instrumentation state.

#### Scenario: Generated report preserves anti-claims [r[assertion-readiness-promotion-gate.anti-claim-preserved]]

- GIVEN accepted workload proofs and their committed assertion artifacts
- WHEN the assertion-readiness report is generated or checked
- THEN it MUST preserve anti-claim text stating that assertion density is not replay proof or product parity by itself

#### Scenario: Gap removal fails closed [r[assertion-readiness-promotion-gate.gap-removal-fails]]

- GIVEN a workload has nonzero unhit, uncategorized, or non-passing assertion gaps
- WHEN a generated or checked assertion-readiness surface omits those gaps without explicit rationale
- THEN the promotion gate MUST exit nonzero and identify the workload and hidden gap class

#### Scenario: Promotion rationale is explicit [r[assertion-readiness-promotion-gate.rationale-required]]

- GIVEN a workload is proposed for an instrumentation-readiness claim stronger than bounded replay proof
- WHEN assertion-readiness promotion is evaluated
- THEN the gate MUST require either zero relevant gaps or a checked workload-specific rationale for each remaining gap class
