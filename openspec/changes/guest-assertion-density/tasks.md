## 1. Catalog metadata and report plumbing

- [ ] 1.1 Extend SDK catalog entries, hypercall emission, oracle records, and serialized report types with guest/category metadata plus a backward-compatible `uncategorized` fallback
- [ ] 1.2 Add per-guest and per-category assertion exercise summaries to single-run and campaign reports
- [ ] 1.3 Add `--min-assertion-exercise` handling to `run` and `campaign`, including a distinct floor-failure exit status after report artifacts are written
- [ ] 1.4 Add unit tests for metadata round-trips, legacy assertion fallback, grouped report aggregation, and floor evaluation

## 2. Raft guest densification

- [ ] 2.1 Add category-aware assertion helpers or wrappers for the Raft guest
- [ ] 2.2 Add paired branch assertions for vote, append, timer, proposal, and delivery/drop outcomes
- [ ] 2.3 Add reachability assertions for election transitions, crash/restart, partition/heal, reorder, and duplication paths
- [ ] 2.4 Add mutation-site invariants for `commit_index`, `next_index`, `match_index`, and leader self-replication bookkeeping
- [ ] 2.5 Extend Raft guest tests or smoke coverage checks so the new assertions stay stable

## 3. Redb guest densification

- [ ] 3.1 Add category-aware assertion helpers or wrappers for the redb guest and register all workload families
- [ ] 3.2 Add explicit success vs failure-or-abort assertions at insert and batch commit boundaries
- [ ] 3.3 Add reopen, repair, restart, and crash-recovery assertions that compare durable state against the shadow oracle
- [ ] 3.4 Add maintenance-path assertions for range scan, compaction, savepoint, and rollback behavior
- [ ] 3.5 Extend redb guest tests or smoke scenarios to cover the new assertion sites

## 4. Docs and validation

- [ ] 4.1 Update `docs/assertion-guidelines.md` with the density-category model and recommended placement patterns
- [ ] 4.2 Run targeted tests for `chaoscontrol-sdk`, `chaoscontrol-explore`, `chaoscontrol-raft-guest`, and `chaoscontrol-redb-guest`
- [ ] 4.3 Run `cargo clippy --all-targets -- -D warnings`
- [ ] 4.4 Run `cargo fmt --all --check`
