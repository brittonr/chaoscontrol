# consistency-checker-ecosystem Specification

## Purpose
TBD - created by archiving change add-consistency-checker-ecosystem. Update Purpose after archive.
## Requirements
### Requirement: Typed operation history [r[consistency-checker.history]]
The system MUST provide a typed operation-history format for workload-level semantic checking without relying on raw-log scraping.

#### Scenario: History records invocation and completion events [r[consistency-checker.history.invocation-completion]]
- **GIVEN** a workload adapter observes client operations
- **WHEN** it emits a semantic history
- **THEN** each operation records a stable operation ID, process or client identity, invocation metadata, completion outcome, workload model, and source artifact reference

#### Scenario: Malformed histories fail closed [r[consistency-checker.history.fail-closed]]
- **GIVEN** a history with duplicate operation IDs, missing completion outcomes, ambiguous process identities, or missing source artifact references
- **WHEN** the history validator runs
- **THEN** it MUST reject the history before any checker report can cite it as evidence

### Requirement: Bounded consistency checker reports [r[consistency-checker.reports]]
The system MUST provide bounded consistency checker reports that identify the checked model, input history digest, verdict, limitations, and counterexample evidence for failures.

#### Scenario: Passing history emits bounded semantic evidence [r[consistency-checker.reports.pass]]
- **GIVEN** a valid history for a supported checker model
- **WHEN** the checker runs successfully
- **THEN** the report records the model, history digest, checked operation count, verdict, and limitations
- **AND** it states that semantic checker evidence is not snapshot replay proof by itself

#### Scenario: Failing history emits counterexample evidence [r[consistency-checker.reports.fail]]
- **GIVEN** a valid history that violates the checker model
- **WHEN** the checker runs
- **THEN** the report records a failing verdict and a bounded counterexample trace that references operation IDs from the input history

### Requirement: Checker evidence promotion guard [r[consistency-checker.promotion-guard]]
The system MUST fail closed if checker evidence is used to imply deterministic replay, hosted product parity, or unsupported model coverage.

#### Scenario: Unsupported model overclaim fails [r[consistency-checker.promotion-guard.unsupported-model]]
- **GIVEN** a readiness surface that claims checker support for a model without a checked report and fixtures
- **WHEN** the promotion guard runs
- **THEN** it MUST exit nonzero and identify the unsupported checker model

