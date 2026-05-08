# Raft Assertions V2 Specification

## Purpose

Defines the canonical ChaosControl requirements for raft assertions v2.

## Purpose

Define Raft guest assertion coverage for integrity, liveness, fault reachability, and per-node state exploration.
## Requirements
### Requirement: Data integrity assertion

The guest SHALL assert that no committed log entry is ever overwritten
with a different value. For each node, if `log[i]` was committed (i ≤
commit_index) at tick T, then `log[i].value` MUST be the same at tick T+1.

#### Scenario: Committed entry preserved across ticks

- **WHEN** node N has `commit_index >= i` and `log[i].value == V` at tick T
- **THEN** `log[i].value == V` at tick T+1 (always-assertion)

#### Scenario: Detection across crash/restart

- **WHEN** node N crashes and restarts with persistent state
- **THEN** all previously committed entries in the log still have the same
  values as before the crash

### Requirement: Fault-aware liveness assertion

The guest SHALL assert that commits advance during periods when a quorum
of nodes is alive and no majority partition exists. This replaces the
unconditional "commit index advanced" sometimes-assertion.

#### Scenario: Commits advance with healthy quorum

- **WHEN** all 3 nodes have been alive (not crashed) for at least 200 ticks
- **WHEN** no partition isolates any node from a majority
- **THEN** `commit_index` on the leader has advanced since the start of that
  healthy period (sometimes-assertion)

#### Scenario: No liveness assertion during faults

- **WHEN** any node is currently crashed or a majority-isolating partition
  is active
- **THEN** the liveness assertion is not evaluated (no false positive)

### Requirement: Remove commit-index-advanced assertion

The unconditional `sometimes("commit index advanced")` assertion SHALL be
removed. It fires on all variants under DiskFull and provides no
discriminating power.

#### Scenario: Assertion removed

- **WHEN** the Raft guest runs under any bug variant
- **THEN** there is no assertion named "commit index advanced"

### Requirement: Crash and restart reachability assertions

The guest SHALL assert that node crashes and restarts actually occur during
exploration, confirming the fault injection is exercised.

#### Scenario: Crashes are reachable

- **WHEN** the guest runs for 1000+ ticks
- **THEN** at least one `reachable("node crashed")` assertion fires

#### Scenario: Restarts are reachable

- **WHEN** the guest runs and at least one crash has occurred
- **THEN** at least one `reachable("node restarted")` assertion fires

### Requirement: Partition reachability assertions

The guest SHALL assert that partitions are created and healed.

#### Scenario: Partitions are reachable

- **WHEN** the guest runs for 1000+ ticks
- **THEN** at least one `reachable("link partitioned")` assertion fires

#### Scenario: Healing is reachable

- **WHEN** a partition has been active
- **THEN** at least one `reachable("partition healed")` assertion fires

### Requirement: Per-node state coverage

The guest SHALL record coverage edges for per-node fault states to guide
the explorer toward under-explored fault combinations.

#### Scenario: Coverage edges for fault states

- **WHEN** a tick executes
- **THEN** coverage edges are recorded for the tuple
  (node_0_alive, node_1_alive, node_2_alive, partition_count)
- **THEN** the explorer can distinguish "1 node crashed" from "2 nodes
  crashed" from "partition active" in the coverage bitmap

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
