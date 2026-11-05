## ADDED Requirements

### Requirement: Retained queries deduplicate ticks with declared branch priority

r[chaoscontrol.distinct_tick_query.precedence]

DistinctByTick MUST accept a nonempty ordered list of query inputs and retain exactly the first encountered row for each u64 simulation tick across those inputs. Earlier branches MUST take precedence; row encounter order MUST break ties within a branch before final canonical ordering. Winning envelopes/projections MUST be preserved. Tick identity MUST span all supplied sources/VMs unless callers filter them. All branches and materialized projected rows, including discarded collisions, MUST remain admitted/evaluated under shared structural/work/result bounds. Existing moments, UpTo scoping, prefix truncation and query/source/result receipt binding MUST remain effective. Existing Union behavior MUST remain unchanged.

#### Scenario: Enriched observations take priority

- GIVEN overlapping branches, including projected observations and multiple source positions at the same tick
- WHEN DistinctByTick evaluates in declared branch order
- THEN only the first encountered envelope at each tick survives with its projections; final rows are canonically ordered and downstream filters see winning projections

#### Scenario: Empty, extreme and discarded inputs remain bounded

- GIVEN zero/u64-maximum ticks, an empty source or discarded duplicate rows
- WHEN a nonempty expression evaluates, or a branch contains invalid JSON/projections or exhausts a shared bound
- THEN ordinary winner rules apply without arithmetic or row synthesis; invalid/disallowed inputs fail rather than being suppressed as duplicates, and CLI failures emit no partial artifacts

#### Scenario: Priority and receipt facts remain binding

- GIVEN branch order, moments and retained source/query/result receipts
- WHEN child moments directly conflict or branch/expression/source/result facts drift
- THEN ambiguity or drift is rejected; explicit outer UpTo retains existing expression scope
