## ADDED Requirements

### Requirement: Retained queries project finite numeric field differences

r[chaoscontrol.map_difference.projection]

MapDifference MUST admit input, destination, minuend and subtrahend field names even on empty sources. For each child row it MUST admit the original envelope, resolve the finite binary64 minuend then subtrahend using complete JSON-number text and canonical-name precedence before any overwrite, and annotate their finite difference as JSON-number text. Missing/malformed/nonfinite operands or a nonfinite result MUST retain the row while removing stale destination projections. Output aliases of either operand MUST read original values. Canonical evidence and other projections MUST remain unchanged. Each materialized child row MUST cost one operation beyond child work; immediate intermediate/generated capacity admission and existing structural/scan/result/prefix/moment/receipt rules MUST apply, without partial evaluation error artifacts. Receipts MUST bind operand order, all names, input and canonical source/result facts.

#### Scenario: Retain metric deltas without corrupting evidence

- GIVEN finite named fields from original projections, JSON extraction, counts or earlier witness context
- WHEN MapDifference computes minuend minus subtrahend and downstream queries select or narrow it
- THEN finite binary64 differences are retained with alias-safe lookup and immutable canonical evidence

#### Scenario: Remove undefined stale differences

- GIVEN invalid/missing/nonfinite operands or overflowing subtraction
- WHEN MapDifference recomputes a stale destination
- THEN the candidate remains but the stale projection is removed, rather than surviving a numeric filter

#### Scenario: Preserve bounded reproducible projection facts

- GIVEN empty inputs, invalid names, hidden intermediate/generated overflow, shared limits, moments and receipts
- WHEN admission/capacity/work fails, moments conflict or operand/query/source/result facts drift
- THEN evaluation or validation rejects while valid projections remain reproducible
