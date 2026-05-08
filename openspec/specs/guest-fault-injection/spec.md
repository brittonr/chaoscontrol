# Guest Fault Injection Specification

## Purpose

Defines the canonical ChaosControl requirements for guest fault injection.

## Requirements
### Requirement: Per-node crash injection

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

The Raft guest SHALL support crashing individual nodes via
`random_choice()`. A crashed node drops all incoming messages, does not
participate in elections, and does not process AppendEntries.

#### Scenario: Node crash during normal operation

- **WHEN** `random_choice(200) == 0` for node N on a given tick
- **THEN** node N is marked as crashed
- **THEN** node N's inbox is cleared
- **THEN** messages addressed to node N are silently dropped

#### Scenario: Crash does not affect other nodes

- **WHEN** node 1 crashes
- **THEN** nodes 0 and 2 continue processing messages and elections normally

### Requirement: Node restart with persistent state

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

The guest SHALL support restarting crashed nodes. On restart, the node
retains persistent Raft state (log, currentTerm, votedFor) and resets
volatile state (commitIndex, role, electionTimer, nextIndex, matchIndex).

#### Scenario: Restart preserves persistent state

- **WHEN** a crashed node restarts via `random_choice(50) == 0`
- **THEN** the node's `log`, `current_term`, and `voted_for` are unchanged
- **THEN** the node's `commit_index` resets to 0
- **THEN** the node's `role` resets to Follower
- **THEN** the node's `election_timer` resets to a base value

#### Scenario: Restart timing is explorable

- **WHEN** a node crashes at tick T
- **THEN** restart decisions use `random_choice()` each subsequent tick
- **THEN** the input-tree explorer can override the restart decision to
  control exactly when the node comes back

### Requirement: Per-link network partitions

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

The guest SHALL support asymmetric per-link network partitions. A partition
between node A and node B drops messages from A to B without affecting other
links. Partitions are created and healed via `random_choice()`.

#### Scenario: Asymmetric partition

- **WHEN** `partitioned[0][1]` is true
- **THEN** messages from node 0 to node 1 are dropped
- **THEN** messages from node 1 to node 0 may still be delivered
- **THEN** messages between nodes 0↔2 and 1↔2 are unaffected

#### Scenario: Partition creation via random_choice

- **WHEN** `random_choice(300) == 0` on a given tick
- **THEN** a random link (source, target) is partitioned
- **THEN** the partition persists until healed

#### Scenario: Partition healing via random_choice

- **WHEN** an active partition exists and `random_choice(30) == 0`
- **THEN** the partition is removed and messages flow again

### Requirement: Message reordering

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

The guest SHALL support message reordering by delivering some messages to a
delay queue instead of the destination inbox.

#### Scenario: Delayed delivery

- **WHEN** `random_choice(20) == 0` for a message
- **THEN** the message is placed in a delay queue
- **THEN** the message is delivered after a random number of ticks (1-5)

### Requirement: Message duplication

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

The guest SHALL support message duplication by delivering a message twice.

#### Scenario: Duplicate delivery

- **WHEN** `random_choice(50) == 0` for a message
- **THEN** the message is delivered to the destination node's inbox twice

### Requirement: All fault decisions use random_choice

This requirement MUST be satisfied by the corresponding ChaosControl implementation and validation evidence.

Every fault injection decision (crash, restart, partition create, partition
heal, reorder, duplicate) SHALL use `random_choice()` from the SDK. No
fault decisions use host-side randomness or VMM-level fault schedules.

#### Scenario: Input-tree explorer can override faults

- **WHEN** the explorer overrides `random_choice()` sequence ID K with value V
- **THEN** the corresponding fault decision uses value V deterministically
- **THEN** the exploration can systematically enumerate all fault
  combinations at each tick
