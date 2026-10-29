## ADDED Requirements

### Requirement: Retained queries annotate finite ratios without stale projections

r[chaoscontrol.map_ratio.projection]

MapRatio MUST accept an input query and admitted output/numerator/denominator field names, validating every name even on empty input. It MUST preserve canonical source evidence and apply existing field lookup and finite binary64 FieldRatio arithmetic. It MUST read both operands before updating the destination. A finite quotient MUST replace the destination with JSON-number text; missing/nonnumeric/nonfinite operands, either signed-zero denominator or a nonfinite quotient MUST remove the destination projection without dropping the row. Signed-zero/underflow/rounding behavior MUST follow existing ratio semantics. Existing canonical-name precedence, intermediate envelope admission, structural/work/scan/derived/result bounds, canonical ordering, prefix truncation, moment propagation/conflict rejection and complete-expression/source/result receipt binding MUST apply. Evaluation failure MUST publish neither CLI result nor receipt.

#### Scenario: Reuse utilization in downstream queries

- GIVEN numeric fields projected from details or JSON text
- WHEN MapRatio computes used/total and a downstream FieldNumber filter selects values above 0.1
- THEN matching complete canonical rows retain a finite utilization projection usable by Narrow or display

#### Scenario: Recompute without stale output or operand aliasing

- GIVEN an existing destination that may alias an operand
- WHEN MapRatio recomputes a finite or undefined quotient
- THEN finite output uses the original operands and replaces the destination while undefined output removes only that projection

#### Scenario: Bound intermediate projections and receipt facts

- GIVEN valid queries with optional moments and retained receipts
- WHEN names are invalid, intermediate/generated envelope capacity or shared bounds are exceeded, moments conflict or expression/source/result facts drift
- THEN evaluation or validation rejects the supplied facts without partial evaluation artifacts
