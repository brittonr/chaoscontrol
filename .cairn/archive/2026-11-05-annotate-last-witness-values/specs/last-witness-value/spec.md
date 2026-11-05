## ADDED Requirements

### Requirement: Retained events carry a value from their nearest earlier witness

r[chaoscontrol.last_witness_value.annotation]

WithLastValue MUST admit both child queries, destination/witness field names and a u64 tick window. It MUST preserve every candidate's canonical evidence and select the witness with greatest (tick, source_id, source_sequence) among strictly earlier ticks within the inclusive distance bound. Repeated witness positions MUST retain their first projection. It MUST copy the selected field using canonical-name precedence, including empty strings, or remove stale destination output if no witness/value exists. A missing nearest value MUST NOT fall back to older witnesses. Both children MUST be evaluated and admitted even for empty candidates or zero window. Existing intermediate/generated envelope, structural/work/scan/result bounds, final canonical ordering/duplicate precedence/prefix truncation, moments and expression/source/result receipt binding MUST apply. Tick arithmetic MUST remain safe at zero and u64::MAX. Evaluation failure MUST publish neither CLI result nor receipt. This contract assumes a coherent caller-supplied tick domain; it does not infer causality, branch lineage or implicit VM/process partitioning.

#### Scenario: Annotate errors with nearest restart context

- GIVEN independently filtered errors and restart witnesses
- WHEN WithLastValue copies witness text within five ticks
- THEN each error remains and carries the nearest strictly earlier restart text if it is in range

#### Scenario: Missing context clears stale projections

- GIVEN a candidate with an existing destination and an absent, out-of-range or valueless nearest witness
- WHEN annotation is recomputed
- THEN only the destination is removed without using an older witness value or dropping the candidate

#### Scenario: Bounded deterministic context

- GIVEN permuted sources, tied/duplicate witnesses, child moments and optional receipts
- WHEN names/capacities/shared bounds are invalid, moments conflict or expression/source/result facts drift
- THEN deterministic tie/first-duplicate rules apply to valid input and invalid facts are rejected without partial evaluation artifacts
