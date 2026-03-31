## 1. Guest fault injection infrastructure

- [ ] 1.1 Add `crashed: [bool; 3]` and `partitioned: [[bool; 3]; 3]` state arrays to main.rs tick loop
- [ ] 1.2 Add `delay_queue: Vec<(usize, usize, Message, usize)>` for message reordering (from, to, msg, deliver_at_tick)
- [ ] 1.3 Implement crash decision: `random_choice(200) == 0` per alive node per tick — set `crashed[i] = true`, clear `nodes[i].inbox`
- [ ] 1.4 Implement restart decision: for each crashed node, `random_choice(50) == 0` — reset volatile state (commit_index=0, role=Follower, election_timer=base, clear next_index/match_index), keep log/current_term/voted_for, set `crashed[i] = false`
- [ ] 1.5 Skip ticking crashed nodes: guard the active-node processing block with `if crashed[active] { continue; }`
- [ ] 1.6 Implement per-link partition creation: `random_choice(300) == 0` → pick random (src, dst) via two `random_choice(3)` calls, set `partitioned[src][dst] = true`
- [ ] 1.7 Implement partition healing: for each active partition, `random_choice(30) == 0` → set `partitioned[src][dst] = false`
- [ ] 1.8 Wire partitions into message delivery: drop message if `partitioned[from][to] || crashed[to]`

## 2. Message-level faults

- [ ] 2.1 Replace fixed 5% drop rate with configurable rate: `random_choice(100) < 95` stays but add partition check first
- [ ] 2.2 Implement message reordering: `random_choice(20) == 0` → push to delay_queue with `deliver_at_tick = tick + 1 + random_choice(4)`
- [ ] 2.3 Implement message duplication: `random_choice(50) == 0` → push message to inbox twice
- [ ] 2.4 Drain delay_queue each tick: deliver messages whose `deliver_at_tick <= tick`

## 3. Assertion overhaul

- [ ] 3.1 Add data integrity assertion: track `committed_values: Vec<(usize, u64)>` — at each tick, for each node, verify `log[i].value` hasn't changed for any `i <= commit_index` that was previously committed
- [ ] 3.2 Add fault-aware liveness: track `ticks_all_alive: usize` counter — increment when no node is crashed and no majority partition exists, reset to 0 otherwise. Assert `sometimes(commit advanced)` only when `ticks_all_alive > 200`
- [ ] 3.3 Remove `cc_assert_sometimes!(... "commit index advanced" ...)` and `cc_assert_sometimes!(... "commit index not advanced" ...)`
- [ ] 3.4 Add crash/restart reachability: `cc_assert_reachable!("node crashed", ...)` on crash, `cc_assert_reachable!("node restarted", ...)` on restart
- [ ] 3.5 Add partition reachability: `cc_assert_reachable!("link partitioned", ...)` on partition creation, `cc_assert_reachable!("partition healed", ...)` on healing
- [ ] 3.6 Add per-node state coverage edges: `coverage::record_edge(9000 + alive_mask * 10 + partition_count)` where alive_mask is a 3-bit bitmap

## 4. Bug deduplication in explorer

- [ ] 4.1 Add `dedup_key: u64` field to BugReport — hash of (assertion_id, sorted fault type names)
- [ ] 4.2 Add `seen_dedup_keys: HashSet<u64>` to Explorer state
- [ ] 4.3 In `process_branch_result()`, skip bugs whose dedup_key is already in seen_dedup_keys
- [ ] 4.4 Add `dedup_key` field to SerializableBug JSON output
- [ ] 4.5 Persist seen_dedup_keys in checkpoint for resume support

## 5. Validation

- [ ] 5.1 Run the 7-variant bug hunt with `--mode input-tree --workers 4 --rounds 20`
- [ ] 5.2 Verify ≥5/7 variants produce safety violations (always-assertion failures)
- [ ] 5.3 Verify the "none" control variant produces zero safety violations
- [ ] 5.4 Verify deduplication reduces bug count (compare with and without dedup)
- [ ] 5.5 Verify crash/restart/partition reachability assertions fire in all variants
