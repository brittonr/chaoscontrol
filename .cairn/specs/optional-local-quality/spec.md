# Optional Local Quality Specification

## Purpose

Defines the `optional-local-quality` capability.

## Requirements

### Requirement: Optional no-hit assertions do not block local quality

r[chaoscontrol.optional_local_quality.blockers]

Strict local report observation/progress blockers MUST agree with the existing admitted property verdict semantics, including optional no-hit always assertions. A passing optional site MUST retain its catalog identity, zero counts and unobserved status without creating a quality blocker. Required or unknown-metadata no-hit sites MUST remain blocking; observed violations MUST remain failures. Report validation MUST recompute the same lists and totals and reject conflicting caller-owned aggregates. Identity, metadata, schema and resource admission MUST remain unchanged.

#### Scenario: Quiet optional property

- GIVEN a completed catalog with an admitted optional always declaration and setup completion
- WHEN no hit occurs
- THEN its verdict and local quality gate pass without inventing observations

#### Scenario: Required or unknown property

- GIVEN an unobserved required property or catalog-only property without must-hit metadata
- WHEN local quality is evaluated
- THEN the observation/progress gaps remain blocking

#### Scenario: Violations and tampering

- GIVEN an observed optional always violation or altered report totals/blocker lists
- WHEN the strict report or quality validator is evaluated
- THEN the violation still fails or the inconsistent report is rejected
