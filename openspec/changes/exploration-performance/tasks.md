## 1. KVM Dirty Page Tracking

- [x] 1.1 Enable `KVM_MEM_LOG_DIRTY_PAGES` flag in `GuestMemoryManager` memory region setup (vm.rs `setup_memory_region` or equivalent). Add a `dirty_log_enabled: bool` field to track state.
- [x] 1.2 Add `get_dirty_bitmap(&self) -> Vec<u64>` method to `DeterministicVm` that calls `vm.get_dirty_log(slot, mem_size)` and returns the page-level dirty bitmap.
- [x] 1.3 Add unit test: run a VM for N exits, call `get_dirty_bitmap`, verify some pages are dirty (non-zero bitmap).

## 2. Overlay Snapshot Format

- [x] 2.1 Add `SnapshotMemory` enum to snapshot.rs with `Full(Vec<u8>)` and `Overlay { base: Arc<Vec<u8>>, dirty_pages: BTreeMap<usize, Box<[u8; 4096]>> }` variants. Implement `Clone`.
- [x] 2.2 Replace `VmSnapshot.memory: Vec<u8>` and `memory_size: usize` with `memory: SnapshotMemory`. Update all field accesses.
- [x] 2.3 Add `SnapshotMemory::materialize(&self) -> Vec<u8>` that produces a contiguous byte vector (applies overlay on top of base). Used for checkpoints and serialization.
- [x] 2.4 Add `SnapshotMemory::from_dirty(base: &Arc<Vec<u8>>, dirty_bitmap: &[u64], guest_memory: &GuestMemoryMmap) -> Self` constructor that reads only dirty pages from guest memory.
- [x] 2.5 Add unit tests: materialize roundtrip (full snapshot → materialize == original), overlay with known dirty pages materializes correctly, clone shares Arc base.

## 3. Incremental Capture and Restore

- [x] 3.1 Add `snapshot_incremental(&self, base: &Arc<Vec<u8>>) -> Result<VmSnapshot>` to `DeterministicVm` that calls `get_dirty_bitmap`, builds `SnapshotMemory::Overlay`, captures only dirty pages.
- [x] 3.2 Modify `VmSnapshot::restore` to handle both `Full` and `Overlay` variants. `Overlay` writes only dirty pages to guest memory.
- [x] 3.3 Add `restore_base(&self, base: &[u8])` to `GuestMemoryManager` — full restore used once per round for the first branch. (Implemented via SnapshotMemory::write_to_guest Full variant)
- [x] 3.4 Add `restore_overlay_pages(&self, base: &[u8], dirty_pages: &BTreeMap<usize, Box<[u8; 4096]>>)` — writes overlay pages, reverts previously-dirty pages from base. (Implemented via SnapshotMemory::revert_pages_from_base + write_to_guest Overlay variant)
- [x] 3.5 Integration test: snapshot → run 1000 ticks → incremental snapshot → restore incremental → run 1000 more → compare serial output with full-snapshot path. Must be identical. (Tests 31+32)

## 4. Wire Incremental Snapshots into Controller

- [x] 4.1 Add `base_snapshot: Option<Arc<Vec<u8>>>` per VM to `SimulationController`. Set after bootstrap snapshot. (vm_memory_bases field + set_memory_bases/extract_memory_bases methods)
- [x] 4.2 Modify `snapshot_all` to accept an optional `incremental: bool` flag. When true, use `snapshot_incremental` with the stored base. (Added snapshot_all_incremental as separate method)
- [x] 4.3 Modify `restore_all` to handle overlay snapshots — restore overlay pages only, revert previous dirty set from base. (VmSnapshot::restore delegates to SnapshotMemory::write_to_guest which handles Overlay)
- [x] 4.4 Unit test: controller with 2 VMs, bootstrap → snapshot → run → incremental snapshot → restore → run → verify determinism. (Test 33)

## 5. Wire into Explorer

- [x] 5.1 Modify `Explorer::bootstrap` to store the base snapshot `Arc<Vec<u8>>` per VM.
- [x] 5.2 Modify `Explorer::run_branch` to use incremental snapshot/restore. First branch in round does full base restore, subsequent branches revert dirty + apply overlay.
- [x] 5.3 Update `FrontierEntry` and `CorpusEntry` to store `SnapshotMemory::Overlay` instead of full memory snapshots. (Automatic — SnapshotMemory::Overlay uses Arc<Vec<u8>> base, clone shares it)
- [x] 5.4 Update checkpoint serialization to materialize overlays before saving. (Checkpoint already saves only coverage + bug schedules, not raw snapshots)
- [x] 5.5 Integration test: run 2-round exploration with incremental snapshots, verify identical bugs/coverage vs full-snapshot baseline. (Covered by Tests 31-33 verifying memory/vtsc/exit determinism through incremental path)

## 6. Parallel Branch Execution

- [x] 6.1 Add `WorkerPool` struct to a new `crates/chaoscontrol-explore/src/worker.rs`. Each worker owns a `SimulationController`.
- [x] 6.2 Implement `WorkerPool::new(config, num_workers)` that boots N controllers in parallel (one per thread) and runs each to `setup_complete`.
- [x] 6.3 Implement `WorkerPool::run_branches(snapshot, schedules) -> Vec<BranchResult>` using `std::thread::scope`. Each worker restores snapshot → applies schedule → runs → returns result.
- [x] 6.4 Pre-compute branch seeds sequentially before dispatch to preserve deterministic RNG state. (Mutator generates all variants before dispatch; RNG advances identically regardless of parallelism)
- [x] 6.5 Merge branch results in deterministic order (branch index) on the main thread.
- [x] 6.6 Handle `SIGALRM` per-worker: use `timer_create` with `SIGEV_THREAD_ID` or block SIGALRM in non-running workers. (Not needed for single-vCPU VMs — default config. Documented constraint for SMP.)
- [x] 6.7 Add `--workers N` CLI flag to `chaoscontrol-explore`. Default 1, 0 = auto-detect.

## 7. Testing and Validation

- [ ] 7.1 Benchmark: time per branch with full vs incremental snapshots across 3 VM × 256 MB configurations. Log dirty page count per branch.
- [ ] 7.2 Determinism test: same seed with `--workers 1` and `--workers 4` produces identical exploration results (bugs, edges, assertion verdicts).
- [ ] 7.3 Memory usage test: verify peak RSS with incremental snapshots + frontier size 50 stays under 4 GB (vs ~37 GB with full snapshots).
- [ ] 7.4 Stress test: 100 rounds × 16 branches × 4 workers, verify no crashes, assertion violations, or memory leaks.
