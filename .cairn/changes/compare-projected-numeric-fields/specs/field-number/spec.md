## ADDED Requirements

### Requirement: Retained predicates compare numeric canonical and derived string fields

r[chaoscontrol.field_number.comparison]

FieldNumber MUST accept an admitted field name, an existing NumberComparison and a finite numeric threshold. It MUST resolve fields using existing canonical-field precedence and compare complete JSON-number text using the existing finite binary64 comparison semantics. Missing, malformed, nonnumeric and overflow/nonfinite text MUST not match any relation, including NotEqual. Ordinary Not MUST retain complement semantics. Valid decimal/exponent forms and surrounding JSON whitespace MUST be supported without altering source fields or projections. Field and threshold admission MUST apply even to empty input and skipped compound branches. Existing operation/structural/result/scan bounds, moment handling and expression/source/result receipt checks MUST remain effective; JsonNumber and JsonTextNumber semantics MUST remain unchanged.

#### Scenario: Filter running counts numerically

- GIVEN events annotated with RunningCount and a derived count field
- WHEN FieldNumber selects counts greater than or equal to two
- THEN only rows with numeric counts at least two remain, without lexical ordering or mutation of canonical evidence

#### Scenario: Numeric grammar and field precedence

- GIVEN canonical and projected strings including decimals, exponents, missing/nonnumeric text and canonical-name collisions
- WHEN FieldNumber evaluates a comparison
- THEN finite numeric text follows the shared six relations, nonnumeric values never match, and canonical fields retain lookup precedence

#### Scenario: Admission and retained receipts

- GIVEN bounded queries and retained source/result receipts
- WHEN invalid fields/thresholds are supplied, shared limits are exhausted or expression/source/result facts drift
- THEN admission, evaluation or receipt validation fails without partial CLI evaluation artifacts
