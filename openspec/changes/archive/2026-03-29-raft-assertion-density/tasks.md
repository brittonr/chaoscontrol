## 1. Handler Reachability Assertions

- [x] 1.1 Add `reachable` assertion at RequestVote handler entry in the inbox drain loop, with candidate term and id in json details
- [x] 1.2 Add `reachable` assertion at RequestVoteResponse handler entry, with vote_granted value in json details
- [x] 1.3 Add `reachable` assertion at AppendEntries handler entry, with leader term and entry count in json details
- [x] 1.4 Add `reachable` assertion at AppendEntriesResponse handler entry, with success and match_index in json details

## 2. State Transition Assertions

- [x] 2.1 Add `reachable` assertion before `become_candidate` call (follower→candidate), with new term in details
- [x] 2.2 Add `reachable` assertion after `become_leader` return (candidate→leader), with winning term in details
- [x] 2.3 Add `reachable` assertion at `become_follower` calls triggered by higher-term messages, with old and new term in details
- [x] 2.4 Add `reachable` assertion when a candidate steps down on receiving AppendEntries with equal/higher term

## 3. Sometimes-Pair Branch Coverage

- [x] 3.1 Add sometimes-pair on commit index advancement (commit_advanced / not advanced) after leader's try_advance_commit
- [x] 3.2 Add sometimes-pair on election timeout (timer expired / timer decremented) in follower/candidate timer logic
- [x] 3.3 Add sometimes-pair on message delivery (delivered / dropped) in the outbox delivery loop
- [x] 3.4 Add sometimes-pair on vote outcome (granted / denied) wrapping the RequestVote response
- [x] 3.5 Add sometimes-pair on AppendEntries outcome (accepted / rejected) wrapping the AppendEntries response
- [x] 3.6 Add sometimes-pair on leader proposal (proposed value / skipped proposal) in leader tick logic

## 4. Data Invariant Assertions at Mutation Sites

- [x] 4.1 Add `always(commit_index <= log.len())` after every commit_index update (follower from leader_commit, leader from try_advance_commit)
- [x] 4.2 Add `always(match_index[peer] <= log.len())` after leader updates match_index on successful AppendEntriesResponse
- [x] 4.3 Add `always(next_index[peer] >= 1)` after leader decrements next_index on rejected AppendEntriesResponse
- [x] 4.4 Add `always(voted_for == Some(candidate_id))` after granting a vote in RequestVote handling
- [x] 4.5 Add `always(match_index[self.id] == log.len())` after leader appends entry to its own log

## 5. Log Conflict Path Assertions

- [x] 5.1 Add `reachable("log conflict: truncated")` when AppendEntries detects and truncates a conflicting entry
- [x] 5.2 Add `reachable("log entries consistent")` when AppendEntries finds existing entries match
- [x] 5.3 Add `reachable("new entries appended")` when AppendEntries appends entries beyond current log length

## 6. Verification

- [x] 6.1 Build the raft guest binary (`scripts/build-raft-guest.sh`) and confirm it compiles
- [x] 6.2 Run `cargo test -p chaoscontrol-raft-guest` to confirm lib.rs unit tests still pass (no SDK leakage)
- [x] 6.3 Run `cargo clippy --workspace` clean
- [x] 6.4 Count total assertion calls in main.rs and confirm ≥25 (up from 6)
