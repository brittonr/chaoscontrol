## ADDED Requirements

### Requirement: Retained JSON numbers support bounded comparisons

r[chaoscontrol.numeric_event_query.comparison]

Event queries MUST support less-than, less-than-or-equal, equal, greater-than-or-equal and greater-than comparisons of a JSON number selected from event details by a confined bounded JSON pointer against a finite JSON-number threshold. Operands MUST compare as finite IEEE-754 binary64 values, without claiming exact integers beyond that representation. Missing or non-numeric operands MUST NOT be coerced or matched. Invalid pointers and malformed predicates MUST be rejected. Filtering MUST retain canonical event fields, deterministic order, existing query budgets and query/result/source-bound receipt validation. Existing text operators MUST retain their semantics.

#### Scenario: Structured memory threshold

- GIVEN numeric observations below, equal to and above a threshold, plus missing or non-numeric values
- WHEN an operator queries for values strictly above the threshold
- THEN only numeric values above it are returned with their original event identities and details

#### Scenario: Boundary and negative comparisons

- GIVEN negative and fractional numeric observations and a finite threshold
- WHEN strict, inclusive and equality operators evaluate them
- THEN each operator follows its declared binary64 comparison, including exact threshold boundaries

#### Scenario: Bounds and receipt drift

- GIVEN a numeric query and a validated receipt
- WHEN the pointer is invalid or overlong, the operation budget is insufficient, or the supplied query threshold/result changes
- THEN admission/evaluation or receipt validation rejects the input instead of silently accepting a different result
