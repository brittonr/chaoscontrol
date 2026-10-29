## ADDED Requirements

### Requirement: Retained queries annotate nearest later witness values

r[chaoscontrol.next_witness_value.annotation]

WithNextValue and WithNextValueBy MUST retain candidate evidence and copy the selected string from the least canonical (tick, source_id, source_sequence) witness satisfying 0 < witness.tick - candidate.tick <= within_ticks. The keyed form MUST require exact selected-key equality using canonical lookup precedence; missing keys MUST NOT match, including missing-to-missing, while empty keys MUST match. Both forms MUST exclude all same-tick witnesses, safely handle zero/maximal ticks and zero/inclusive windows, and keep first duplicate witness projection priority before optional key grouping. Missing/out-of-range witnesses or missing selected values on the nearest witness MUST remove stale destination output without farther-value fallback; present empty values MUST be copied. Original keys and witness values MUST define matches before output alias mutation. Other projections and canonical evidence MUST remain unchanged. All names and both children MUST validate and evaluate on empty candidate sets or zero windows; every materialized row MUST be charged/admitted before witness dedup and generated rows MUST be admitted before downstream narrowing/dedup. Shared structural/scan/work/derived/result/prefix bounds, moment reconciliation and full query/direction/key/names/window/source/result receipt binding MUST apply without partial evaluation-error artifacts. Existing earlier-witness semantics MUST remain unchanged. Outer UpTo MUST NOT implicitly constrain hidden witness history. These operators MUST NOT imply causality, VM/history coherence, matched-event output, synthetic timeouts or multiverse compatibility.

#### Scenario: Deterministic forward context

- GIVEN interleaved candidates/witnesses, tied/extreme ticks and optional keys
- WHEN nearest later values are annotated
- THEN strict future selection uses the least eligible canonical witness, inclusive window and exact same-key matching when requested

#### Scenario: Explicit missingness and aliases

- GIVEN absent/empty keys and values, duplicate projections and output aliases
- WHEN future context is selected
- THEN original membership and first projections apply, empty strings copy and missing nearest context clears output without fallback

#### Scenario: Bounded reproducible composition

- GIVEN projected witness JSON/ticks, downstream queries, limits, moments and receipts
- WHEN evaluation runs or facts drift
- THEN valid complete evidence composes while hidden invalid rows, capacity/work/moment violations and receipt drift reject
