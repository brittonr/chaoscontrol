# Design: Isolate Branch Watchdog Timeouts

## Context

`DeterministicVm::step` returns `VmError::HostWatchdogTimeout` when a single-vCPU `KVM_RUN` returns `VcpuExit::Intr` or `EINTR`. `SimulationController::finish_round_mutation` then latches permanent round poison, and later `restore_all`, `run`, and `snapshot_all` calls return `ControllerRoundPoisoned`.

Snapshot-backed replay already creates a fresh controller and calls `restore_all` without a bootstrap run (`chaoscontrol-explore.rs` reproduce path). Incremental restore reverts only the previous overlay's pages. It requires guest memory to equal the bootstrap base everywhere else (`SnapshotMemory::write_to_guest`).

## Decisions

### 1. Pure classification and attempt policy

`branch_recovery` owns these pure pieces:

- `host_watchdog_vcpu(&VmError)` and `explore_error_watchdog_vcpu(&ExploreError)` recognize only `HostWatchdogTimeout`.
- `decide_after_watchdog(attempts_made, parent_snapshot_available)` returns `Retry` or `Discard`.
- `RoundWatchdogLedger` records completed attempts, watchdog firings, and discarded branches. `ensure_admissible` returns `HostWatchdogTimeout` when every attempted branch was discarded.

`MAX_BRANCH_WATCHDOG_ATTEMPTS` is 3 attempts per branch. Two retries cover transient host descheduling. The bound limits the recovery work spent on a guest that never reaches a VM exit, because each attempt stops at the 100 ms watchdog.

### 2. Controller replacement

`rebuild_controller` creates a fresh `SimulationController` from the explorer's simulation configuration. It performs a full restore from the retained bootstrap snapshot with materialized memory, resets VM statuses, and installs the same memory bases. The new controller then matches the post-bootstrap memory contract. The next incremental restore of the parent snapshot follows the normal path. The explorer keeps the bootstrap snapshot with memory that shares the base `Arc`, so retaining it costs no second copy of guest memory. `dlog_dir` is cleared on replacement so the new controller does not overwrite the original determinism logs.

### 3. Retry scope

Retry is allowed only when the branch has a parent snapshot. Without one, the branch started from mutable controller state that cannot be reconstructed, so it is discarded. The input-tree probe and override branches use the same loop. Choice overrides live on the discarded controller and are applied again for each retry.

Each retry reapplies the branch's vCPU schedule variant. Completed branches keep their original variant slots when earlier branches are discarded, so Campaign child moments bind to the correct branch.

### 4. Worker pool

`WorkerPool` retains the worker simulation configuration and the recovery snapshot. Each worker rebuilds its own controller inside its thread and calls `init_thread_timers` again. Only watchdog timeouts go through recovery. Other errors and panics keep the existing placeholder behavior.

### 5. Evidence

`RoundHistory` gains `host_watchdog_timeouts` (every firing, including firings recovered by retry) and `host_watchdog_discarded_branches`, both with `#[serde(default)]`. `MetricsLine` and `format_report` show them. The report states that a nonzero discard count means the exploration trajectory depended on host timing.

## Risks

- A replacement controller starts from a clean base, while a long-lived controller carries pages that incremental revert did not reset. The overlay dirty-page gap is confirmed and predates this change. On 2026-09-25, in a Raft run with zero watchdog firings, snapshot-backed replay restored replay-parent snapshots at depths 1 and 2. At depths 3 and 4 it failed with `virtio transport restore failed: Queue(AvailableDelta { delta: 65533, capacity: 256 })`. A retried branch whose parent is at depth 3 or more can therefore differ from its uninterrupted counterpart. This change records every retry but does not fix the gap.
- Under sustained contention, recovery adds a full memory write per firing. Throughput drops, and the report shows the firing count.
