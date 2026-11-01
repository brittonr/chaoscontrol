## Context

`validate_history` keys each replica's observations by command index. When an index was already present with equal facts, it returned early as an exact duplicate before the rollback comparison against the replica's maximum observed index ran.

## Decisions

### Decision: Check rollback before accepting an exact duplicate

**Choice:** When an observation equals the stored observation for its index, compare its index with the replica's maximum observed index first. Record a `Rollback` violation when it is lower, then still count the exact duplicate.

**Rationale:** Equality with an old report does not make a regression to that report harmless. The exact-duplicate counter stays truthful, and the verdict reflects the rollback.

### Decision: Keep current-index repetition idempotent

**Choice:** A repeat of the replica's current maximum index with identical facts remains a passing exact duplicate.

**Rationale:** Observers can legitimately report the current committed state twice. Only a return to an earlier index is a rollback.

## Risks / Trade-offs

- Histories that previously passed with stale exact repeats now fail. That is the intended correction, and the regression suite distinguishes both cases.
