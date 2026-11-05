## ADDED Requirements

### Requirement: Boolean event predicates compose under shared bounds

r[chaoscontrol.compound_event_query.composition]

Retained event filters MUST support conjunction, disjunction and negation of existing predicates. Children MUST evaluate left-to-right with short-circuiting; empty conjunction MUST match and empty disjunction MUST not match. All descendants MUST pass admission even when evaluation would skip them or the source is empty. Predicate descendants MUST share the query node/depth bounds and consume one operation per visited child, without charging skipped children. Filter roots and existing atomic predicates MUST retain their historical structural and operation costs. Budget exhaustion MUST reject evaluation, not return a partial answer. Filtering MUST preserve canonical event fields, deterministic ordering, existing atomic semantics and query/source/result-bound receipt validation.

#### Scenario: Compound operational selection

- GIVEN memory observations across selected VMs and maintenance/non-maintenance processes
- WHEN a conjunction selects high-memory events in either selected VM and negates the maintenance process predicate
- THEN only events satisfying the full expression are returned with their original details and identities

#### Scenario: Short-circuiting retains admission and operation safety

- GIVEN compound predicates with skipped valid or malformed descendants
- WHEN admission and evaluation run under explicit bounds
- THEN malformed descendants are rejected before evaluation, visited descendants consume operations, skipped descendants do not, and insufficient operation budgets fail

#### Scenario: Structural limits and receipt identity remain binding

- GIVEN nested query and predicate expressions at the shared node/depth bounds and a valid receipt
- WHEN a descendant exceeds either bound or a different expression selects the same rows
- THEN admission rejects the excess and receipt validation rejects the changed expression
