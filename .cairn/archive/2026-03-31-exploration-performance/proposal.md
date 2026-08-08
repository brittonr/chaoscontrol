## Why

Exploration throughput is bottlenecked by full-memory snapshot operations.
Each branch does a 256 MB × N memcpy on restore and another on snapshot —
with 3 VMs that's 1.5 GB of copies per branch. Branches also run
sequentially despite being independent. A 200-round × 8-branch campaign
spends most of its wall-clock time on memcpy and idle cores.

## What Changes

- Use KVM dirty page tracking (`KVM_MEM_LOG_DIRTY_PAGES` +
  `KVM_GET_DIRTY_LOG`) to identify which 4 KB pages the guest wrote
  during a branch. Snapshot and restore only dirty pages instead of the
  full address space.
- Store snapshots as a shared immutable base (`Arc<Vec<u8>>`) plus a
  sparse dirty-page overlay (`BTreeMap<usize, [u8; 4096]>`). Cloning a
  snapshot copies the overlay, not the base.
- Run branches within a round in parallel across threads. Each worker
  owns its own `SimulationController` (separate KVM VM FDs), restores
  from a shared snapshot, executes its schedule, and returns results.
- Pool worker controllers so kernel boot happens once per worker, not
  once per branch.

## Capabilities

### New Capabilities
- `incremental-snapshots`: Dirty-page-tracked snapshot and restore using
  KVM dirty logging. Replaces full-memory copy with sparse overlays.
- `parallel-exploration`: Multi-threaded branch execution within an
  exploration round. Worker pool with per-worker VM controllers.

### Modified Capabilities

## Impact

- `chaoscontrol-vmm`: memory.rs (dirty log API), snapshot.rs (overlay
  format), vm.rs (enable dirty tracking, incremental capture/restore),
  controller.rs (snapshot types carry Arc base).
- `chaoscontrol-explore`: explorer.rs (parallel branch loop, worker
  pool), frontier.rs and corpus.rs (store overlay snapshots instead of
  full copies).
- Memory usage drops from O(frontier × VMs × 256 MB) to
  O(256 MB + frontier × VMs × dirty_pages).
- New dependency: `rayon` or manual `std::thread` pool.
