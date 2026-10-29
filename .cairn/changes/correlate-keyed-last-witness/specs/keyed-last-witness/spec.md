## ADDED Requirements

### Requirement: Retained candidates receive same-key prior witness context

r[chaoscontrol.keyed_last_witness.annotation]

WithLastValueBy MUST preserve candidates and annotate a selected scalar from the greatest canonical (tick, source_id, source_sequence) witness strictly earlier than the candidate and within the inclusive tick-distance window whose key_field value equals the candidate's. Canonical selector precedence and exact string equality MUST apply. Missing keys MUST NOT match; present empty keys MUST match other present empty keys. Absent, out-of-range or missing-value nearest witnesses MUST remove stale output without older-value fallback. Output aliases MUST use the original key; all other projections and canonical evidence MUST remain unchanged. Witness source positions MUST be deduplicated with first-projection priority before key grouping. Both child queries and all three field names MUST validate even with empty input or zero windows; all materialized candidates/witnesses MUST be charged and admitted before deduplication/narrowing. Existing structural/scan/work/derived/result/prefix limits, moment reconciliation and complete query/key/source/result receipt binding MUST apply; evaluation errors MUST NOT publish partial result/receipt artifacts. Existing WithLastValue semantics MUST remain unchanged. Key equality MUST NOT establish inferred causality or history lineage.

#### Scenario: Interleaved owners retain their own prior context

- GIVEN interleaved processes or explicitly projected service keys, tied witnesses and boundary ticks
- WHEN a keyed annotation runs
- THEN each candidate receives only the nearest eligible same-key scalar independent of source permutation

#### Scenario: Missing context and conflicting projections remain explicit

- GIVEN missing/empty keys or values, output aliases and duplicate witness positions with different projected keys
- WHEN witnesses are selected
- THEN missing keys never correlate, empty keys remain explicit, no older-value fallback occurs and first-projection priority precedes grouping

#### Scenario: Bounded composed evidence is reproducible

- GIVEN JSON projections, downstream selection, admission/capacity limits, moments and receipts
- WHEN valid queries run or their facts drift
- THEN complete evidence survives and invalid inputs, hidden overflows, budget/moment conflicts and drift reject
