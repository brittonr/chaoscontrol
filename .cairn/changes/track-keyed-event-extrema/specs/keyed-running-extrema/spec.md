## ADDED Requirements

### Requirement: Retained queries track same-key numeric prefix extrema

r[chaoscontrol.keyed_running_extrema.prefixes]

RunningMinBy and RunningMaxBy MUST preserve unique canonical input evidence while annotating independent inclusive finite binary64 extrema for exact selected string keys. Canonical-name lookup precedence MUST apply to keys and operands. Empty keys MUST form a group; missing keys and rows before their key's first matching value MUST remove stale output without a zero or infinite seed. Every unique row MUST evaluate its predicate against original values, including missing-key rows; matching operands MUST be complete finite JSON numbers even for missing keys, and nonmatching operands MUST NOT be parsed. Predicate error, work and short-circuit semantics MUST remain unchanged. Strict comparison MUST retain the earliest numerically equal value, including signed zero. Output aliases MUST NOT change original key, operand or predicate membership. First duplicate projection priority MUST precede grouping. Other projections and canonical evidence MUST remain unchanged and output MUST be canonical. All three names, predicate and input MUST validate on empty sources. Every materialized row MUST be charged/admitted before deduplication and each generated row MUST be admitted before downstream filtering/narrowing. Shared structure/scan/work/derived/result/prefix limits, moments and full query/operator/key/value/predicate/source/result receipt binding MUST apply without partial evaluation-error artifacts. Existing RunningMin/RunningMax behavior MUST remain unchanged. Grouping MUST NOT imply VM/history coherence, exact arithmetic or causal/multiverse lineage.

#### Scenario: Interleaved owners retain independent extrema

- GIVEN multiple keys, tied/boundary ticks and finite signed measurements
- WHEN minima or maxima are annotated
- THEN each key retains its inclusive extremum in canonical order independent of source permutation, without a zero seed

#### Scenario: Missingness and aliases do not hide errors or regroup rows

- GIVEN missing/empty keys, aliases, duplicate projections and invalid operands
- WHEN extrema are computed
- THEN original keys and first projections define groups, missing keys clear output without hiding matching operand errors, skipped operands remain unread and equal signed zero retains its earliest sign

#### Scenario: Bounded reproducible composition

- GIVEN JSON projections, downstream queries, limits, moments and receipts
- WHEN queries run or their facts drift
- THEN complete valid results compose while invalid admission/capacity/work/moments/receipts reject
