# Repair Incremental Snapshot Lineage

## Why

The explorer stores frontier snapshots as overlays on the bootstrap memory image. `DeterministicVm::snapshot_incremental` builds each overlay from the KVM dirty log (`SnapshotMemory::from_dirty`, `snapshot.rs:175`). The dirty log records guest writes since the last `get_dirty_log` call. It does not record host writes made through the memory mapping. When a branch restores parent snapshot X, `restore_incremental` writes X's overlay pages from the host. The child snapshot Y then contains only the pages the guest wrote during the branch. Pages that X changed and the branch did not rewrite are missing from Y. `restore_incremental` also reverts only the previous overlay's pages (`last_dirty_page_indices`, `vm.rs:3023`). Pages the guest wrote during a branch stay in memory for the next restore.

A 2026-09-25 local run shows the effect. Seed 42 explored a Raft workload for 10 rounds of 8 branches, and the watchdog never fired. Snapshot-backed `reproduce` restored replay-parent snapshots at depths 1 and 2. At depths 3 and 4 it failed before execution with:

```text
virtio transport restore failed: Queue(AvailableDelta { delta: 65533, capacity: 256 })
```

The guest's available-ring index in the restored memory was three entries behind the queue cursor in the same snapshot. Every safety-assertion failure found in seven Raft campaigns sat at replay-parent depth 3 or more, and none could be replayed. The control campaign, which has no planted bug, reported 15 safety failures at depth 7. The accepted dogfood replay evidence uses replay-parent depth 1 only, so this defect does not invalidate it.

This defect breaks the `deterministic-snapshots` requirement `chaoscontrol.deterministic_snapshots.complete_state`: an exact snapshot must contain complete guest memory.

## What Changes

- Make every incremental snapshot a complete overlay relative to its base: the pages restored from the parent overlay plus the pages the guest wrote since that restore. r[chaoscontrol.deterministic_snapshots.incremental_lineage]
- Make incremental restore revert every page that differs from the base, including pages the guest wrote after the previous restore. r[chaoscontrol.deterministic_snapshots.incremental_revert]
- Plan the page sets in a pure core over supplied page-index facts. Keep KVM dirty-log reads and guest-memory writes in the shell. r[chaoscontrol.deterministic_snapshots.incremental_core]
- Validate that a depth-N lineage materializes to the same memory as a full snapshot taken at the same point, and that replay at depth 3 or more restores and continues. r[chaoscontrol.deterministic_snapshots.incremental_validation]

## Impact

- **Code**: `chaoscontrol-vmm` incremental snapshot capture and restore, snapshot page-set planning, and the explorer's controller-replacement path, which relies on the base-memory contract.
- **Evidence**: exploration findings at replay-parent depth 3 or more produced before this change are diagnostic-only. Replay and assertion-readiness gates must not accept them.
- **Performance**: overlays grow with lineage depth up to the number of pages that differ from the base. Restore reverts more pages per branch.

## Non-Goals

- No change to full snapshots, snapshot schema identity, or replay-parent persistence format beyond what complete overlays require.
- No change to the watchdog-recovery policy in `isolate-branch-watchdog-timeouts`.
- No claim that fixing lineage restores seed-to-seed explorer repeatability. `restore-explorer-seed-repeatability` owns that claim.
