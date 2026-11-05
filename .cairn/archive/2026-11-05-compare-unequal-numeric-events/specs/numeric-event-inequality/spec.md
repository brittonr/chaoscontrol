## ADDED Requirements

### Requirement: Numeric event predicates support typed inequality

r[chaoscontrol.numeric_event_query.inequality]

JsonNumber and JsonTextNumber MUST support NotEqual through the shared finite IEEE-754 binary64 comparison implementation. Only admitted numeric operands that compare unequal MUST match. Missing paths, null, strings, booleans and containers MUST NOT match. Signed zero and integers rounded to the same binary64 value MUST compare equal. Existing Not semantics, source separation, strict visited JSON parsing, pointer/threshold admission, work/structural/result bounds, canonical rows, moments and source/query/result receipt binding MUST remain unchanged.

#### Scenario: Typed numeric deviations are selected

- GIVEN retained details or canonical JSON text containing equal and unequal numeric values alongside absent or nonnumeric values
- WHEN NotEqual evaluates against a finite numeric threshold
- THEN only the unequal numeric observations are retained unchanged, without implicit coercion or inclusion of missing fields

#### Scenario: Binary64 equality boundaries remain authoritative

- GIVEN signed zeros, equivalent numeric representations or wide integers rounded to the same binary64 value
- WHEN NotEqual evaluates
- THEN equal binary64 operands do not match; distinct representable numeric values do match

#### Scenario: Errors and receipts remain binding

- GIVEN a bounded inequality query and retained source/query/result receipts
- WHEN visited text is invalid, a pointer/threshold is inadmissible, work is exhausted or bound facts drift
- THEN evaluation or validation fails without partial CLI evaluation artifacts; short-circuiting and skipped-descendant admission retain existing behavior
