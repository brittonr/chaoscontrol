## Why

The Raft guest has 6 assertions (3 safety, 3 liveness) that only fire in the post-tick sweep. The explorer finds bugs through assertion violations, so assertion placement directly determines what it can detect. Right now it's blind to whether handlers are being reached, which state transitions are exercised, and whether data invariants hold at the point of mutation. The `docs/assertion-guidelines.md` already documents the gap and the patterns to fill it — this change implements them.

## What Changes

- Add `reachable` assertions at every message handler entry point in `main.rs` (RequestVote, RequestVoteResponse, AppendEntries, AppendEntriesResponse)
- Add `sometimes` pairs on quorum/timeout/delivery branches (quorum reached vs not, election timeout fired vs not, message delivered vs dropped)
- Add `reachable` assertions on all state transitions (follower→candidate, candidate→leader, any→follower, and `unreachable` for impossible transitions)
- Add inline `always` assertions at data mutation sites (commit_index ≤ log.len after advance, match_index ≤ log.len after replication, next_index > 0 after decrement)
- Add `sometimes` assertions for leader proposal and heartbeat paths
- Add `reachable`/`unreachable` for log conflict detection and truncation paths
- Wire `json!({...})` details with relevant state (term, node id, commit index) into every new assertion for post-mortem triage

## Capabilities

### New Capabilities
- `raft-assertion-density`: Assertion placement patterns and density targets for the Raft guest program

### Modified Capabilities

(none — no existing specs to modify)

## Impact

- **Code changed**: `crates/chaoscontrol-raft-guest/src/main.rs` (the VM guest binary that wires lib.rs to SDK)
- **No API changes**: All new assertions use existing `chaoscontrol_sdk::assert::*` functions
- **No new dependencies**: `serde_json::json!` already in scope
- **Explorer impact**: More assertion sites means finer-grained bug detection and better exploration quality signals (sometimes-never-fired = blind spot)
- **Performance**: Negligible — each assertion is a single hypercall, same cost as existing 6
