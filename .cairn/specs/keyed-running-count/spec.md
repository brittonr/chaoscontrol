# Keyed Running Count Specification

## Purpose

Defines the `keyed-running-count` capability.

## Requirements

### Requirement: Retained queries count same-key canonical prefixes

r[chaoscontrol.keyed_running_count.prefixes]

RunningCountBy MUST retain unique input positions in canonical order and annotate the inclusive checked u64 count of predicate-matching positions with the same original key. Canonical selector precedence and exact string equality MUST apply. Explicit empty keys MUST form a group; missing keys MUST remove stale count output without counting. Every unique row's predicate MUST evaluate against original content, including missing-key rows, with existing short-circuit/work/error semantics. Output aliases MUST use original key and predicate values. Repeated positions MUST retain first-projection priority before grouping. All other projections and canonical evidence MUST remain unchanged. Both field names, predicate and child MUST validate on empty sources. Every materialized child row MUST be charged and admitted before deduplication; generated rows MUST be admitted before downstream filtering/narrowing. Existing structural/scan/work/derived/result/prefix bounds, moments and full query/key/predicate/source/result receipt binding MUST apply without partial evaluation error artifacts. Existing RunningCount semantics MUST remain unchanged. Key equality MUST NOT establish inferred VM/source identity, causal history or multiverse lineage.

#### Scenario: Count interleaved owners independently

- GIVEN interleaved keys and canonical tie/boundary ticks
- WHEN matching rows are counted
- THEN each present key has its own zero-seeded inclusive prefix count independent of source permutation

#### Scenario: Preserve original membership and explicit missingness

- GIVEN missing/empty keys, aliases, duplicate projections and predicate errors
- WHEN a keyed count runs
- THEN first projections define grouping, original values define predicates, missing keys clear stale output without hiding visited errors, and empty keys count normally

#### Scenario: Compose bounded retained counts

- GIVEN JSON projections, downstream filters, capacity/shared limits, moments and receipts
- WHEN queries run or facts drift
- THEN valid complete rows compose and invalid admission, hidden overflows, work/moment conflicts and drift reject
