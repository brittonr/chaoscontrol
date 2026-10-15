# Design: Repair Incremental Snapshot Lineage

## Context

The guest memory contract for an incremental restore is: memory equals the base everywhere except the pages in `last_dirty_page_indices`. Three facts break that contract today.

1. `get_dirty_log` returns guest writes only. Host writes during restore are not logged.
2. `snapshot_incremental` reads the dirty log and captures only those pages.
3. `restore_incremental` reverts only the pages written by the previous restore.

The explorer runs sequences like: restore X, run, snapshot Y, restore X', run, snapshot Y'. Each Y misses X's pages that the branch did not rewrite. Each restore leaves the previous branch's guest writes in place. The long-lived explorer controller therefore runs on memory that neither the snapshot nor a fresh controller can reproduce.

## Decisions

### 1. Track the page set that differs from the base

The VM keeps one page set, `pages_differing_from_base`. It is the union of:

- the pages written by the last incremental restore (the restored overlay's pages), and
- the guest-dirty pages accumulated from every dirty-log read since that restore.

A full restore clears the set only when the restored memory equals the base. Otherwise the set becomes every page where the restored image differs from the base.

### 2. Capture uses the set, not only the dirty log

`snapshot_incremental` reads the dirty log, merges it into the set, and captures every page in the set. Y is then complete relative to the base regardless of lineage depth.

### 3. Restore reverts the set

`restore_incremental` first reads and merges the dirty log, reverts every page in the set to the base, writes the new overlay, and resets the set to the new overlay's pages. The dirty log is drained before the write so host writes cannot be confused with later guest writes.

### 4. Pure page-set planning

A pure function receives the current set, a dirty bitmap, and the target overlay page indices. It returns the revert list, the write list, and the next set. It checks page-index bounds against the guest size before any memory access. The shell performs the ioctls and memory writes.

### 5. Old deep evidence is diagnostic-only

Replay and readiness gates already require a valid parent snapshot. After this change they classify an older replay-parent snapshot at depth 3 or more as `blocked` when restore preflight rejects it. They do not attempt to repair old snapshots.

## Risks

- Overlay size and restore cost grow with lineage depth. Validation records overlay page counts per depth for the Raft and redb workloads.
- The explorer's controller-replacement path (`isolate-branch-watchdog-timeouts`) writes the base and then restores incrementally. After this change it matches the long-lived controller exactly. Validation must compare the two directly.
