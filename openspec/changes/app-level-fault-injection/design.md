## Context

The Raft guest runs 3 nodes in a single VM. The exploration loop calls
`random_choice()` at 4 points per tick: active node selection (3), jitter
(6), proposal decision (4), message delivery (100). The input-tree explorer
branches at these points to explore different execution paths.

VMM-level faults target VMs, not application nodes. With 1 VM, DiskFull
freezes all 3 nodes, CpuBitflip crashes the whole VM, and
NetworkPartition has no effect (no inter-VM network). The explorer wastes
most branches on DiskFull which produces identical "commit stalled" results
regardless of which Raft bug variant is active.

The Raft lib has `Node::new_with_bug(id, bug_mode)` but no crash/restart
API. Nodes accumulate state monotonically — once a term is seen, the node
never forgets it.

## Goals / Non-Goals

**Goals:**
- Per-node crash, restart, and partition via `random_choice()` so the
  input-tree explorer can systematically enumerate fault scenarios
- Assertions that distinguish "DiskFull stalled everything" (expected) from
  "leader lost a committed entry" (safety violation)
- ≥5/7 bug variants caught with safety violations in 20 rounds

**Non-Goals:**
- Changing the VMM fault injection infrastructure (keep it for multi-VM)
- Modifying the SDK API
- Disk persistence simulation (nodes have no durable storage in the current
  in-process model)
- Multi-VM Raft (separate future change)

## Decisions

### 1. Fault injection lives in the guest main loop, not lib.rs

**Choice:** Add fault logic to `main.rs`'s tick loop, wrapping calls to
`Node` methods. `lib.rs` stays pure Raft logic.

**Rationale:** lib.rs has 78 unit tests and 100% line coverage. Mixing
fault injection into the core Raft logic would contaminate the pure
protocol implementation. The main loop already controls node activation,
message delivery, and tick timing — fault injection is an extension of
that orchestration.

**Alternative:** Adding `is_crashed` / `is_partitioned` fields to Node and
gating message handling. Rejected because it couples fault injection to
the Raft protocol, making unit tests harder and the lib less reusable.

### 2. Fault decisions via random_choice() per tick

**Choice:** Each tick, draw from `random_choice()` to decide:
- Should a node crash this tick? (`random_choice(200) == 0` → 0.5% per tick)
- Should a crashed node restart? (`random_choice(50) == 0` → 2% per tick)
- Is there a partition between node A and B? (`random_choice(100) < partition_rate`)
- Should this message be reordered? (deliver to a delay queue instead of inbox)
- Should this message be duplicated? (deliver twice)

**Rationale:** Every decision goes through `random_choice()` which the
input-tree explorer can override at specific sequence IDs. This lets the
explorer systematically try "what if node 1 crashes at tick 50" vs "what if
node 2 crashes at tick 50" by overriding a single choice point.

**Alternative:** Use a pre-generated fault schedule (like the VMM does).
Rejected because it doesn't integrate with input-tree exploration — the
schedule would be fixed per branch, not explorable via choice overrides.

### 3. Crash = clear volatile state, keep log + votedFor + currentTerm

**Choice:** `crash_node(i)` sets a `crashed[i] = true` flag. While crashed,
the node doesn't process messages and its inbox is dropped. On restart,
the node keeps `log`, `voted_for`, `current_term` (persistent state per
Raft spec) but resets `commit_index`, `role`, `election_timer`, `leader_id`,
`next_index`, `match_index` (volatile state).

**Rationale:** This matches the Raft paper's persistence model. Real bugs
like `fig8_commit` and `premature_commit` manifest when a node with stale
persistent state rejoins the cluster and the leader makes incorrect commit
decisions.

**Alternative:** Full restart from empty state (lose everything). Rejected
because Raft guarantees safety only if persistent state survives crashes.

### 4. Per-link partitions instead of global partition

**Choice:** Maintain a `partitioned: [[bool; 3]; 3]` matrix. When
`partitioned[a][b]` is true, messages from a to b are dropped (but b to a
may still work — asymmetric partitions). Partitions heal after a random
duration via `random_choice()`.

**Rationale:** Per-link partitions create more interesting scenarios than
all-or-nothing. A leader partitioned from one follower but not the other
is exactly the scenario that triggers `fig8_commit` and `premature_commit`
bugs.

### 5. Replace "commit index advanced" with fault-aware liveness

**Choice:** Remove the `sometimes(commit_index_advanced)` assertion.
Replace with:
- `always`: "no committed entry overwritten" — check that `log[i].value` at
  any committed index never changes across ticks (data integrity)
- `sometimes`: "commits advance when all nodes alive" — only assert
  liveness during periods when no node is crashed and no partition is active
- `sometimes`: "crashed node restarted" — ensure restarts happen (reachability)
- `always`: "restarted node preserves persistent state" — on restart, verify
  `current_term`, `voted_for`, and `log` haven't changed

**Rationale:** The current assertion triggers on every variant under
DiskFull because it doesn't know about the fault context. Conditioning
liveness on "quorum is reachable" avoids false positives from intentional
fault injection.

### 6. Bug deduplication by (assertion_id, fault_type_set)

**Choice:** In `Explorer::process_branch_result()`, hash each bug by
`(assertion_id, sorted set of fault type names)`. Skip bugs whose hash
matches an existing corpus entry.

**Rationale:** 56 bugs for leader_no_stepdown were all the same root cause
(DiskFull → log matching violation). Dedup reduces corpus noise and keeps
the frontier focused on genuinely different failure modes.

**Alternative:** Dedup by assertion_id only. Rejected because the same
assertion can fail through different fault paths (DiskFull vs Partition),
which are independently interesting.

## Risks / Trade-offs

**[Determinism]** All fault decisions go through `random_choice()`, so
determinism is preserved. The choice sequence is part of the snapshot,
so restore+replay works.

**[Crash rate tuning]** If crashes are too frequent, no leader is ever
stable long enough to commit. If too rare, faults never happen in 1000
ticks. The 0.5% crash rate gives ~5 crashes per 1000-tick branch, which
is enough to trigger multi-crash scenarios without permanent liveness
failure.

**[Unit test coverage]** lib.rs is unchanged, so all 78 unit tests pass
unmodified. The new fault logic in main.rs is covered by the end-to-end
exploration runs.

**[VMM fault interaction]** When running with `--mode hybrid`, VMM-level
faults still fire alongside guest-level faults. DiskFull from the VMM has
no effect on the in-process Raft (there's no actual disk I/O). The main
concern is CpuBitflip crashing the VM before the guest has time to exercise
its own faults. Mitigation: run with `--mode input-tree` instead of hybrid
to disable VMM fault schedules entirely.
