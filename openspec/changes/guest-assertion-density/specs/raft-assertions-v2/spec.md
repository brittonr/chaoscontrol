## ADDED Requirements

### Requirement: Paired branch assertions cover core Raft outcomes
The Raft guest SHALL register paired branch assertions for its core binary outcomes so the catalog can distinguish both sides of important decisions. At minimum this includes vote grant vs vote denial, append acceptance vs append rejection, timeout fired vs timer decremented, proposal emitted vs proposal skipped, and message delivered vs message dropped.

#### Scenario: Vote handling exposes both outcomes
- **WHEN** a node handles a `RequestVote` flow
- **THEN** the guest has distinct assertion sites for `vote granted` and `vote denied`

#### Scenario: Replication handling exposes both outcomes
- **WHEN** a node handles an `AppendEntriesResponse`
- **THEN** the guest has distinct assertion sites for `append accepted` and `append rejected`

#### Scenario: Scheduler-visible control paths are paired
- **WHEN** an active node executes its timer, proposal, or message-delivery path
- **THEN** the guest has distinct assertion sites for the true and false outcomes of each path

### Requirement: Transition and recovery paths are assertion-visible
The Raft guest SHALL register reachability assertions for meaningful state transitions and fault/recovery paths, including election start, election win, leader stepdown, crash, restart, partition, heal, message reorder, and message duplication.

#### Scenario: Election lifecycle is visible
- **WHEN** a follower times out, becomes candidate, and later wins or steps down
- **THEN** the guest emits distinct reachability assertions for election start, election win, and stepdown paths

#### Scenario: Fault and network perturbation paths are visible
- **WHEN** the workload injects crashes, restarts, partitions, heals, reorders, or duplicates messages
- **THEN** each of those paths is represented by a distinct assertion site in the catalog

### Requirement: Replication bookkeeping is asserted at mutation sites
The Raft guest SHALL assert local bookkeeping invariants at the point where replication state changes, rather than relying only on post-tick sweeps. At minimum this includes `commit_index <= log.len()`, `next_index >= 1`, `match_index <= log.len()`, and the leader's self `match_index` tracking its own log length.

#### Scenario: Commit index remains bounded
- **WHEN** a node updates `commit_index`
- **THEN** the guest immediately asserts that `commit_index <= log.len()` at that mutation site

#### Scenario: Leader replication indexes remain coherent
- **WHEN** a leader updates `next_index` or `match_index` after processing follower responses or local proposals
- **THEN** the guest immediately asserts that `next_index >= 1`
- **AND** `match_index <= log.len()` for the affected peer
- **AND** the leader's own `match_index` equals its current log length after a local append
