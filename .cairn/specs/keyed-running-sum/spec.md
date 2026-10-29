# Keyed Running Sum Specification

## Purpose

Defines the `keyed-running-sum` capability.

## Requirements

### Requirement: Retained queries sum same-key numeric prefixes

r[chaoscontrol.keyed_running_sum.prefixes]

RunningSumBy MUST preserve unique canonical input observations and annotate independent inclusive finite binary64 totals for exact selected string keys, seeded at positive zero. Canonical-name precedence MUST apply to keys and operands. Empty keys MUST form a group; missing keys MUST remove stale total output without accumulating. Every unique row MUST evaluate its predicate against original values, including missing-key rows; every matching operand MUST be a complete finite JSON number even on missing-key rows, while nonmatching operands MUST NOT be parsed. Predicate short-circuit/work/error semantics MUST remain unchanged. Any nonfinite intermediate group total MUST fail immediately with InvalidNumber. Original key/value/predicate membership MUST survive output aliases. First duplicate projection priority MUST precede grouping. Other projections and canonical evidence MUST remain unchanged and output MUST be canonical. All names, predicate and child MUST validate on empty input. Every materialized row MUST be charged and admitted before deduplication; generated rows MUST be admitted before downstream narrowing/filtering. Shared structural/scan/work/derived/result/prefix limits, moments and full query/key/value/predicate/source/result receipt binding MUST apply without partial evaluation error artifacts. Existing RunningSum semantics MUST remain unchanged. Keyed aggregation MUST NOT imply exact arithmetic, source/history coherence or causal/multiverse lineage.

#### Scenario: Interleaved owners retain independent totals

- GIVEN multiple keys, tied/boundary ticks and finite signed measurements
- WHEN matching numeric prefixes are summed
- THEN each present key has its own inclusive total in canonical order independent of source permutation

#### Scenario: Missingness and invalid numbers remain explicit

- GIVEN missing/empty keys, aliases, duplicate projections and invalid operands
- WHEN summation runs
- THEN first projections and original keys define grouping, missing keys clear stale output without hiding matching operand errors, skipped operands remain unread and overflowing groups reject

#### Scenario: Bounded reproducible composition

- GIVEN projected JSON measurements, downstream queries, shared limits, moments and receipts
- WHEN queries run or facts drift
- THEN complete valid evidence composes while invalid admission/capacity/work/moments/receipts reject
