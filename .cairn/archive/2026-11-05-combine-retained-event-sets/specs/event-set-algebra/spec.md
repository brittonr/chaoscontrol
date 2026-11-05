## ADDED Requirements

### Requirement: Retained event queries support intersection and difference

r[chaoscontrol.event_set_algebra.combination]

Intersect MUST select input rows whose (source_id, source_sequence) occurs in other; Difference MUST select input rows whose position does not occur in other. Both MUST preserve input rows/projections and their original order before existing canonical ordering and first-wins deduplication. Membership MUST NOT depend on derived fields or tick equality alone. Both child expressions MUST be admitted/evaluated, hidden right-hand projected envelopes MUST be admitted, and shared work/structural/result bounds MUST remain binding. Combination MUST use in-place right-side sorting and binary search rather than Cartesian joins, extra key collections or candidate copies. Existing moment/UpTo semantics and source/query/result receipt binding MUST remain effective.

#### Scenario: Independently transformed selections combine by source identity

- GIVEN overlapping or disjoint retained selections, including multiple sources sharing sequence numbers and conflicting projections
- WHEN Intersect or Difference evaluates
- THEN only left-hand rows satisfying membership or absence are emitted with original left projections in canonical order; empty and identical operands follow ordinary set rules

#### Scenario: Hidden branches remain bounded and admitted

- GIVEN an empty left selection or nonemitted right-hand rows
- WHEN right-side parsing/admission fails or a shared bound is exhausted
- THEN evaluation fails instead of silently treating the other side as empty; CLI failures emit no partial result or receipt

#### Scenario: Moments and identity drift remain binding

- GIVEN branch moments and retained query/source/result receipts
- WHEN direct child moments disagree or operator/operand/source/result facts change
- THEN ambiguity or drift is rejected; explicit outer UpTo keeps its existing expression scope
