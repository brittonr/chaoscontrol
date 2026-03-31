## Context

The exploration loop runs `max_rounds × branch_factor` branches. Each
branch calls `restore_all` → `run` → `snapshot_all`. With 3 VMs × 256 MB
guest memory, a single branch copies ~1.5 GB of data just for snapshot
operations. Branches within a round are independent (fork from the same
snapshot, run different schedules) but execute sequentially.

Current flow per branch:
1. `restore_all`: write 256 MB × 3 VMs = 768 MB via `write_slice`
2. `controller.run(ticks)`: ~50ms of actual KVM execution
3. `snapshot_all`: read 256 MB × 3 VMs = 768 MB via `read_slice`
4. Clone `BranchResult` (includes snapshot) for frontier/corpus

The memcpy dominates wall-clock time. A typical branch runs 1000 ticks
in ~50ms but spends ~200ms on snapshot I/O.

Frontier entries store full `SimulationSnapshot` clones. With
`max_frontier=50` and 3 VMs, that's 50 × 768 MB = ~37 GB of snapshot
data in memory.

## Goals / Non-Goals

**Goals:**
- Reduce per-branch snapshot/restore time by 10-50× through dirty page
  tracking
- Reduce memory consumption from O(frontier × VMs × mem_size) to
  O(mem_size + frontier × VMs × dirty_pages)
- Enable parallel branch execution within a round for near-linear
  speedup with core count
- Maintain perfect determinism — identical seeds produce identical bugs

**Non-Goals:**
- Kernel boot optimization (already cached via controller reuse)
- Distributed exploration across machines (future work)
- Async I/O for coverage collection (64 KB bitmap is already fast)
- Changing the exploration algorithm itself

## Decisions

### 1. KVM dirty logging for change tracking

**Choice:** Use `KVM_MEM_LOG_DIRTY_PAGES` flag on the memory region
plus `KVM_GET_DIRTY_LOG` ioctl to get a hardware-tracked dirty bitmap.

**Alternatives:**
- Software write-protect via `mprotect` + SIGSEGV handler: portable but
  high overhead per fault, complex signal-safety concerns inside the
  VMM's signal handlers (SIGALRM already used for SMP preemption).
- Userfaultfd write-protect: requires newer kernel (5.7+), more complex
  than KVM's built-in mechanism, and we already depend on KVM.
- Full memory diff (XOR + scan): O(mem_size) regardless of dirty count.

**Rationale:** KVM dirty logging uses EPT/NPT hardware dirty bits.
Zero overhead during execution — the MMU sets dirty bits as a side
effect of address translation. The `KVM_GET_DIRTY_LOG` ioctl returns
a bitmap and atomically clears the dirty bits, so the next query
reflects only new writes. This is the same mechanism live migration
uses.

**API:** `kvm-ioctls` exposes `vm.get_dirty_log(slot, mem_size)` which
returns a `Vec<u64>` dirty bitmap. Each bit represents one 4 KB page.
To enable, set `KVM_MEM_LOG_DIRTY_PAGES` in `kvm_userspace_memory_region.flags`
when creating the memory slot.

### 2. Overlay snapshot format

**Choice:** `SnapshotMemory` enum with two variants:

```rust
enum SnapshotMemory {
    Full(Vec<u8>),
    Overlay {
        base: Arc<Vec<u8>>,
        dirty_pages: BTreeMap<usize, Box<[u8; 4096]>>,
    },
}
```

**Alternatives:**
- `HashMap<usize, ...>` for dirty pages: faster lookup but
  non-deterministic iteration order (we need deterministic
  materialization for checkpoint hashing).
- Flat `Vec<(usize, [u8; 4096])>` sorted by page index: lower overhead
  but O(n) lookup during restore; BTreeMap is O(log n) and still
  deterministic.
- Page-level CoW tree (like the block device): overkill — we don't
  need multi-level overlay chains. Snapshots are always base + one
  layer of dirty.

**Rationale:** BTreeMap gives deterministic iteration (needed for
`materialize()`) and O(log n) point lookups. `Box<[u8; 4096]>` avoids
heap fragmentation from many small `Vec` allocations. `Arc<Vec<u8>>`
lets all snapshots in a round share the same 256 MB base without
copying.

### 3. Incremental restore strategy

**Choice:** On `restore`, write only the overlay pages to guest memory.
Before running the first branch after a new base snapshot, do a full
restore of the base. Subsequent branches in the same round skip the
base write — they only need to:
1. Undo the previous branch's dirty pages (restore base values for
   those pages)
2. Apply the target snapshot's overlay

This means restore cost = O(prev_dirty + cur_dirty) instead of
O(mem_size).

**Implementation detail:** After `run` completes and before the next
`restore`, we have the dirty bitmap from `KVM_GET_DIRTY_LOG`. The
pages in that bitmap are exactly the ones we need to revert. We
revert them from the base, then apply the new overlay.

For the first branch in a round, we do a full base restore (no prior
dirty set to revert). This is still one full write per round, not per
branch.

### 4. Parallel execution via thread pool

**Choice:** `std::thread::scope` with N worker threads. Each worker
owns a `SimulationController` (created + bootstrapped at pool init).
Workers receive `(schedule, snapshot_base_ref)` and return
`BranchResult`.

**Alternatives:**
- `rayon`: heavier dependency, work-stealing not needed (branches are
  uniform cost), and we need each worker to own a persistent
  controller across iterations.
- `tokio` spawn_blocking: adds async runtime dependency for no benefit
  — the work is CPU-bound KVM ioctls, not I/O.
- Process-level parallelism (fork): KVM FDs don't survive fork cleanly,
  and we'd lose shared memory for the base snapshot.

**Rationale:** `std::thread::scope` is zero-dependency, gives
deterministic thread indices, and naturally handles the "each worker
owns a controller" pattern. The scoped lifetime ensures the base
`Arc` reference is valid for all workers.

### 5. Determinism under parallelism

**Choice:** Assign each branch a deterministic sub-seed derived from
the round RNG: `branch_seed = rng.next_u64()` for branch 0, 1, 2, ...
computed sequentially before dispatching to workers. Workers use their
branch seed for mutation. Results are collected into a `Vec<BranchResult>`
indexed by branch number, then processed in order.

The RNG state advances identically regardless of how many workers
execute — the seed derivation is sequential, only the execution is
parallel.

**Rationale:** The current sequential loop implicitly derives branch
seeds by consuming RNG in order. Making this explicit and pre-computing
seeds preserves the same sequence while allowing parallel execution.

### 6. Worker controller pool lifecycle

Each worker controller goes through:
1. **Boot** (once): `SimulationController::new` + `run_until_setup_complete`
2. **Receive base snapshot** (once per round): store `Arc<Vec<u8>>` ref
3. **Per branch**: restore overlay → set schedule → run → capture
   dirty → build overlay snapshot → return result
4. **Shutdown**: drop controllers when pool is dropped

Workers don't need the frontier or corpus — those stay on the main
thread. Workers only return `BranchResult` (coverage bitmap, oracle
report, overlay snapshot, exit counts).

## Risks / Trade-offs

**[Risk] KVM dirty log includes spurious pages** →
KVM marks pages dirty for internal reasons (APIC MMIO access, interrupt
delivery to pages near device MMIO). This means the dirty set is a
superset of actual guest writes. Mitigation: this only affects
snapshot size, not correctness. In practice the overhead is small
(a few extra pages).

**[Risk] SIGALRM interaction with thread pool** →
SMP preemption uses `SIGALRM` via `setitimer`. In a multi-threaded
process, signals are delivered to an arbitrary thread. Mitigation:
block `SIGALRM` in worker threads and only allow it in the thread
currently running `vcpu.run()`. Each worker manages its own timer.
Alternatively, use `timer_create` with `SIGEV_THREAD_ID` to target
the signal at the specific thread.

**[Risk] Memory region flag change requires destroying/recreating slot** →
`KVM_MEM_LOG_DIRTY_PAGES` is set at slot creation time. To toggle it,
we may need to re-register the memory region. Mitigation: enable dirty
logging at VM creation time unconditionally (negligible overhead when
not queried — the hardware sets dirty bits regardless).

**[Risk] Parallel workers multiply memory usage** →
N workers × M VMs × 256 MB base memory. With 4 workers × 3 VMs that's
3 GB of mmap'd guest memory. Mitigation: the base snapshot is shared
via `Arc`, and worker guest memory is the same total as running N
sequential branches — we just have N live at once instead of
sequentially. This is bounded by `--workers` flag.

**[Trade-off] First branch in a round still needs full base restore** →
The first branch has no prior dirty set to revert from, so it writes
all 256 MB × N VMs. Subsequent branches revert only dirty pages. For
`branch_factor=8`, this amortizes the full restore over 8 branches.
