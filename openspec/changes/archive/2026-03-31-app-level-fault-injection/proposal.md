## Why

The bug hunt found safety violations in only 2 of 7 known Raft bug variants.
The root cause: VMM-level fault injection is wrong-grained for in-process
Raft. With `--vms 1`, faults like DiskFull/CpuBitflip/NetworkPartition affect
ALL 3 nodes simultaneously (they share a VM), while real Raft bugs require
per-node failures — kill node 2 while 0 and 1 keep running. 85-95% of
exploration budget goes to DiskFull which uniformly freezes the whole cluster,
a scenario that can't distinguish buggy from correct Raft.

## What Changes

- **Per-node fault injection in the guest**: The Raft guest uses
  `random_choice()` to decide per-node crashes, restarts, partitions, and
  message corruption — replacing VMM-level faults as the primary fault source.
  All decisions flow through the SDK random tree, so the input-tree explorer
  can systematically explore them.

- **Richer message-level faults**: Beyond the current 5% uniform drop rate,
  add per-link partitions (A can't talk to B but can talk to C), message
  reordering, message duplication, and temporary partitions that heal.

- **Node crash and restart**: Simulate node crashes (clear volatile state,
  keep persistent log) and delayed restarts. The current model never restarts
  crashed nodes — real Raft bugs often need a node to come back with stale
  state.

- **Better assertions**: Replace the noisy "commit index advanced"
  sometimes-assertion with targeted liveness checks that tolerate expected
  fault behavior (e.g., "after all nodes are alive for N ticks, commits
  should advance"). Add data integrity assertions (committed value at index
  I is never replaced by a different value).

- **Bug deduplication**: The explorer currently treats each assertion failure
  as a unique bug even when they share the same fault schedule and root cause.
  Deduplicate by (assertion_id, schedule_hash) to avoid flooding the corpus
  with duplicates.

## Capabilities

### New Capabilities
- `guest-fault-injection`: Per-node fault injection inside the Raft guest
  using SDK random_choice() — node crashes, restarts, per-link partitions,
  message reordering/duplication, and temporary fault windows with healing.
- `raft-assertions-v2`: Stronger assertion suite — data integrity checks,
  fault-aware liveness, node-level reachability, and per-node state machine
  coverage.
- `bug-deduplication`: Deduplicate bugs by (assertion_id, schedule_hash) in
  the explorer to prevent duplicate corpus entries.

### Modified Capabilities

## Impact

- **Files**: `crates/chaoscontrol-raft-guest/src/main.rs` (guest fault loop),
  `crates/chaoscontrol-raft-guest/src/lib.rs` (node crash/restart API),
  `crates/chaoscontrol-explore/src/explorer.rs` (dedup logic),
  `crates/chaoscontrol-explore/src/corpus.rs` (BugReport dedup)
- **APIs**: Node struct gets `crash()` and `restart_from_persistent()` methods.
  No SDK API changes — uses existing `random_choice()`.
- **Dependencies**: None
- **Testing**: Rerun the 7-variant bug hunt. Success metric: safety violations
  found in ≥5 of 7 variants (currently 2/7).
