## ADDED Requirements

### Requirement: Retained queries annotate deterministic finite numeric prefix sums

r[chaoscontrol.running_sum.prefixes]

RunningSum MUST validate its input/predicate and destination/value field names even on empty sources. It MUST admit intermediate rows, order by (tick,source_id,source_sequence), retain the first projection per source position and annotate every inclusive unique-event prefix with a binary64 total seeded at +0.0. Only predicate-matching rows MUST contribute their selected finite JSON-number value; nonmatching operands MUST NOT be parsed. Predicates and operands MUST be read before destination overwrite using canonical-name precedence. Missing/malformed/nonfinite matching operands and nonfinite intermediate totals MUST fail InvalidNumber. Canonical source evidence MUST remain unchanged. Existing predicate evaluation, shared work/structural/scan/derived/result limits, canonical-prefix truncation, moments and complete query/source/result receipt rules MUST apply. Generated capacity failures MUST NOT be hidden by downstream narrowing. CLI evaluation failures MUST emit neither result nor receipt. This is bounded ordinary binary64 accumulation, not a general reducer, exact arithmetic or inferred timeline/partition claim.

#### Scenario: Accumulate projected metrics

- GIVEN numeric metric projections and typed predicates
- WHEN RunningSum visits unique canonical events
- THEN every row retains its evidence and the inclusive total of matching metric values in canonical order

#### Scenario: Reject undefined aggregate values without parsing skipped operands

- GIVEN malformed/missing/nonfinite operands or an overflowing intermediate total
- WHEN a matching row contributes
- THEN evaluation rejects without partial artifacts, while malformed values on nonmatching rows do not affect the total

#### Scenario: Preserve bounded reproducible aggregate facts

- GIVEN permuted/duplicate sources, optional moments and receipts
- WHEN names/predicates/capacity/shared bounds are invalid, moments conflict or query/source/result facts drift
- THEN first-projection/canonical-order semantics hold for valid inputs and invalid facts are rejected
