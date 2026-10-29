# Running Count Specification

## Purpose

Defines the `running-count` capability.

## Requirements

### Requirement: Retained queries annotate inclusive ordered match counts

r[chaoscontrol.running_count.prefixes]

RunningCount MUST accept an input query, an existing EventPredicate and an admitted target field. It MUST admit all materialized child rows, order them by (tick, source_id, source_sequence) and retain the first encountered projection per source position before counting. It MUST evaluate the predicate before overwriting the target derived entry and annotate every unique row with the decimal u64 number of matches at or before that row, starting from zero. Canonical source evidence MUST remain unchanged. Empty input MUST yield no rows; all/none predicates MUST yield ordinal/zero annotations. The query MUST share existing predicate admission, regex caching, short-circuiting, structural/work/result limits, moment propagation and complete-expression/canonical-source/result receipt checks. It MUST charge one additional logical operation per materialized candidate plus visited predicate children and admit generated rows before outer operations can discard them.

#### Scenario: Count matching logs before errors

- GIVEN a retained coherent event stream with severity-20 and severity-30 observations in arbitrary supplied order
- WHEN RunningCount matches severity-20 and a downstream filter selects severity-30
- THEN selected events retain canonical evidence and report inclusive counts from their unique canonical prefixes

#### Scenario: Duplicates and tied ticks

- GIVEN repeated source positions with differing query-local projections and distinct sources at the same tick
- WHEN RunningCount evaluates the input
- THEN first encountered projections win per position, duplicates count once, and source ID then sequence deterministically orders ties

#### Scenario: Invalid data, limits and drift

- GIVEN an empty or populated source, declared bounds and an optional receipt
- WHEN fields/predicates are invalid, visited JSON is malformed, intermediate/generated data exceeds bounds, child moments conflict or receipt facts drift
- THEN evaluation or validation fails without hiding invalid annotations through downstream pruning and without partial CLI evaluation artifacts
