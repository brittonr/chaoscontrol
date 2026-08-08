# redb-guest-assertions Specification

## Purpose
TBD - created by archiving change guest-assertion-density. Update Purpose after archive.
## Requirements
### Requirement: Redb workload families are assertion-visible
The redb guest SHALL register distinct assertion sites for each workload family it drives: insert, batch insert, read, delete, range scan, compact, savepoint, and rollback. Write-like operations SHALL also distinguish success from failure-or-abort paths so the catalog reveals which durability boundaries were exercised.

#### Scenario: Operation catalog includes all workload families
- **WHEN** the redb guest binary is compiled
- **THEN** its assertion catalog contains distinct assertion messages for insert, batch insert, read, delete, range scan, compact, savepoint, and rollback paths

#### Scenario: Commit outcome is distinguishable
- **WHEN** an insert or batch insert operation reaches its commit boundary
- **THEN** the guest records a success-path assertion when the commit succeeds
- **AND** the guest records a distinct failure-or-abort assertion when the commit does not succeed

### Requirement: Durable state is checked against the oracle across recovery boundaries
After reopen, repair, restart, or crash recovery, the redb guest SHALL compare every oracle-visible committed key against the database and SHALL assert that committed data survives while uncommitted data remains invisible.

#### Scenario: Committed data survives restart
- **WHEN** the guest reopens the database after a crash or restart
- **THEN** every key/value pair present in the shadow oracle is still readable from redb with the same value

#### Scenario: Uncommitted data does not become durable
- **WHEN** a write-like operation does not reach a successful commit before a crash or recovery path
- **THEN** the recovered database does not expose that write as committed data

### Requirement: Maintenance and rollback paths preserve oracle agreement
The redb guest SHALL assert that range scan, compaction, savepoint, and rollback paths preserve agreement with the shadow oracle before returning to the main workload loop.

#### Scenario: Range scan preserves ordering and contents
- **WHEN** the guest performs a range scan over `[lo, hi)`
- **THEN** the reported entry count matches the oracle slice
- **AND** each returned key/value pair matches the oracle in order

#### Scenario: Savepoint and rollback restore oracle-visible state
- **WHEN** the guest creates a savepoint, performs additional writes, and rolls back
- **THEN** the post-rollback database view matches the oracle snapshot taken at the savepoint boundary

#### Scenario: Compaction preserves committed values
- **WHEN** the guest runs compaction and then re-reads committed keys
- **THEN** the values returned after compaction still match the oracle
