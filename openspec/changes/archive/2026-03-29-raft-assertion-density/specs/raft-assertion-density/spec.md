## ADDED Requirements

### Requirement: Handler reachability assertions
The Raft guest main loop SHALL assert `reachable` at the entry point of every message handler: RequestVote, RequestVoteResponse, AppendEntries, and AppendEntriesResponse. Each assertion MUST include the handler name, sender node id, and current term in its json details.

#### Scenario: RequestVote handler reached
- **WHEN** a node processes a RequestVote message
- **THEN** a `reachable("request_vote handler", ...)` assertion fires with the candidate's term and id

#### Scenario: RequestVoteResponse handler reached
- **WHEN** a node processes a RequestVoteResponse message
- **THEN** a `reachable("request_vote_response handler", ...)` assertion fires with the vote_granted value

#### Scenario: AppendEntries handler reached
- **WHEN** a node processes an AppendEntries message
- **THEN** a `reachable("append_entries handler", ...)` assertion fires with the leader's term and entry count

#### Scenario: AppendEntriesResponse handler reached
- **WHEN** a node processes an AppendEntriesResponse message
- **THEN** a `reachable("append_entries_response handler", ...)` assertion fires with success/failure and match_index

### Requirement: State transition assertions
The Raft guest SHALL assert `reachable` on every valid state transition and `unreachable` on invalid transitions. Valid transitions: follower→candidate, candidate→leader, leader→follower, candidate→follower. Invalid: follower→leader (must go through candidate).

#### Scenario: Follower starts election
- **WHEN** a follower's election timer expires and it calls `become_candidate`
- **THEN** a `reachable("follower started election", ...)` assertion fires with the new term

#### Scenario: Candidate wins election
- **WHEN** a candidate reaches quorum and calls `become_leader`
- **THEN** a `reachable("candidate won election", ...)` assertion fires with the winning term

#### Scenario: Node steps down to follower
- **WHEN** any node transitions to follower via `become_follower` due to a higher term
- **THEN** a `reachable("stepped down to follower", ...)` assertion fires with the old and new term

#### Scenario: Candidate steps down on AppendEntries
- **WHEN** a candidate receives an AppendEntries with equal or higher term
- **THEN** a `reachable("candidate stepped down on append_entries", ...)` assertion fires

### Requirement: Sometimes-pair branch coverage
The Raft guest SHALL use sometimes-pairs on every binary branch that represents meaningfully different system behavior. Each pair consists of `sometimes(cond, ...)` and `sometimes(!cond, ...)` with distinct message strings.

#### Scenario: Quorum reached vs not reached
- **WHEN** a leader calls `try_advance_commit`
- **THEN** both `sometimes(commit_advanced, "commit index advanced", ...)` and `sometimes(!commit_advanced, "commit index not advanced", ...)` fire

#### Scenario: Election timeout vs tick decrement
- **WHEN** a follower/candidate ticks its election timer
- **THEN** both `sometimes(timer_expired, "election timeout fired", ...)` and `sometimes(!timer_expired, "election timer decremented", ...)` fire

#### Scenario: Message delivered vs dropped
- **WHEN** the main loop decides whether to deliver a message
- **THEN** both `sometimes(delivered, "message delivered", ...)` and `sometimes(!delivered, "message dropped", ...)` fire

#### Scenario: Vote granted vs denied
- **WHEN** a node responds to a RequestVote
- **THEN** both `sometimes(granted, "vote granted", ...)` and `sometimes(!granted, "vote denied", ...)` fire

#### Scenario: AppendEntries accepted vs rejected
- **WHEN** a follower responds to AppendEntries
- **THEN** both `sometimes(success, "append accepted", ...)` and `sometimes(!success, "append rejected", ...)` fire

#### Scenario: Leader proposes vs skips
- **WHEN** a leader's tick runs
- **THEN** both `sometimes(proposed, "leader proposed value", ...)` and `sometimes(!proposed, "leader skipped proposal", ...)` fire

### Requirement: Data invariant assertions at mutation sites
The Raft guest SHALL assert `always` invariants at the point where data is mutated, not just in the post-tick sweep. Each assertion MUST fire immediately after the state change.

#### Scenario: commit_index bounded after advance
- **WHEN** a node's commit_index is updated (follower from leader_commit, leader from try_advance_commit)
- **THEN** `always(commit_index <= log.len(), "commit_index within log bounds", ...)` fires

#### Scenario: match_index bounded after replication
- **WHEN** a leader updates match_index for a peer
- **THEN** `always(match_index[peer] <= log.len(), "match_index within bounds", ...)` fires with the peer id

#### Scenario: next_index positive after decrement
- **WHEN** a leader decrements next_index after a rejected AppendEntries
- **THEN** `always(next_index[peer] >= 1, "next_index stays positive", ...)` fires

#### Scenario: voted_for consistent with term
- **WHEN** a node grants a vote
- **THEN** `always(voted_for == Some(candidate_id), "voted_for matches granted candidate", ...)` fires

#### Scenario: leader match_index self-consistent
- **WHEN** a leader appends a new entry to its own log
- **THEN** `always(match_index[self.id] == log.len(), "leader self match_index tracks log", ...)` fires

### Requirement: Log conflict path assertions
The Raft guest SHALL assert `reachable` on both the conflict and no-conflict paths during AppendEntries log processing. Log truncation on conflict SHALL be marked `reachable` to confirm the explorer exercises conflict resolution.

#### Scenario: Log conflict detected and truncated
- **WHEN** AppendEntries processing finds a conflicting entry and truncates
- **THEN** `reachable("log conflict: truncated", ...)` fires with the conflict index

#### Scenario: Log entries consistent
- **WHEN** AppendEntries processing finds all entries match
- **THEN** `reachable("log entries consistent", ...)` fires

#### Scenario: New entries appended
- **WHEN** AppendEntries processing appends entries beyond the current log
- **THEN** `reachable("new entries appended", ...)` fires with the count
