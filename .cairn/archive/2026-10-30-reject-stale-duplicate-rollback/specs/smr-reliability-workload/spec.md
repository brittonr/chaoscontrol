# SMR Reliability Workload Specification Delta

## ADDED Requirements

### Requirement: Stale exact duplicates are rollbacks

r[chaoscontrol.smr_chain.duplicate_rollback] An observation whose facts equal an earlier observation from the same replica MUST count as an exact duplicate. If its command index is lower than that replica's highest observed command index, history validation MUST also report a rollback safety violation. A repeat of the highest observed index with identical facts MUST remain idempotent.

#### Scenario: Replica repeats an older observation
- GIVEN a replica reports index 1, then index 2, then the identical index 1 observation again
- WHEN history validation runs
- THEN it counts one exact duplicate and fails safety with a rollback violation.

#### Scenario: Replica repeats its current observation
- GIVEN a replica reports its highest observed index again with identical facts
- WHEN history validation runs
- THEN it counts one exact duplicate and reports no rollback.
