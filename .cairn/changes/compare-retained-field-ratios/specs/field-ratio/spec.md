## ADDED Requirements

### Requirement: Retained predicates compare finite ratios of selected fields

r[chaoscontrol.field_ratio.comparison]

FieldRatio MUST accept numerator and denominator names, an existing NumberComparison and a finite numeric threshold. Both names MUST satisfy existing field admission even for empty input or skipped branches. The predicate MUST resolve existing canonical/derived fields with canonical-name precedence, parse complete finite JSON-number text and compare the binary64 quotient with the existing six relations. Missing/nonnumeric/nonfinite operands, positive/negative zero denominators and nonfinite quotients MUST not match any relation, including NotEqual. Negative and signed-zero finite quotients and underflow MUST follow shared binary64 comparison semantics. Not MUST retain ordinary complement semantics. Canonical evidence and projections MUST remain unchanged. Existing bounds, moment handling and complete-expression/canonical-source/result receipt checks MUST remain effective.

#### Scenario: Select utilization above ten percent

- GIVEN retained events with projected numeric used and total strings
- WHEN FieldRatio compares used/total greater than 0.1
- THEN only observations with a finite quotient above the threshold remain with full canonical evidence unchanged

#### Scenario: Undefined ratios are not numeric inequalities

- GIVEN missing/nonnumeric operands, zero denominators or overflowing quotients
- WHEN any ratio relation including NotEqual is evaluated
- THEN undefined ratios do not match, while Not around a ratio predicate still complements that nonmatch

#### Scenario: Bounded admission and receipts

- GIVEN admitted sources, bounded ratio queries and optional retained receipts
- WHEN names/thresholds are invalid, shared bounds are exhausted, moments conflict or expression/source/result facts drift
- THEN evaluation or validation rejects the input without partial CLI evaluation artifacts
