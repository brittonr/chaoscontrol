# Assertion Json Details Specification

## Purpose

Defines the `assertion-json-details` capability.

## Requirements

### Requirement: Assertion details preserve bounded JSON values

r[chaoscontrol.assertion_json_details.shapes]

Unscoped catalog-bound SDK assertion emission and strict local evidence MUST accept explicit bounded JSON null, booleans, numbers, strings, arrays and objects as assertion details. Shape MUST NOT suppress an assertion or alter its accepted identity, outcome or counts. Metadata MUST only come from top-level object fields or the accepted catalog, never scalar text or nested array members. Legacy diagnostic streams MAY carry these shapes but MUST remain legacy-ambiguous and ineligible for collision-safe promotion.

#### Scenario: A workload supplies scalar or sequence diagnostics

- GIVEN an admitted assertion site and JSON details of any supported shape
- WHEN the unscoped SDK emits an observation and the strict report is checked
- THEN the serialized details retain their shape and the report retains the observation, catalog identity and verdict

### Requirement: Flexible details retain authority and resource boundaries

r[chaoscontrol.assertion_json_details.bounds]

Assertion details MUST remain explicit. Object guest/category overrides MUST retain existing type, bound and catalog-consistency checks. Identity, display type, must-hit, exact-record and generic JSON/stream limits MUST remain enforced. Process-scoped SDK transport MUST retain its valid-token and object-details requirements so shape flexibility cannot erase process ownership. Oversized SDK details MUST NOT emit an accepted assertion.

#### Scenario: Invalid diagnostics do not weaken admission

- GIVEN omitted details, invalid object metadata, a conflicting identity, over-limit JSON, or process-scoped non-object diagnostics
- WHEN SDK emission or evidence admission is attempted
- THEN the invalid observation is rejected under the existing boundary rather than emitted or promoted with invented metadata
