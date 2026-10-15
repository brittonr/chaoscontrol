## Tasks

- [x] [serial] Confirm the overlay capture and revert gaps and record the 2026-09-25 depth-three replay failure. r[chaoscontrol.deterministic_snapshots.incremental_lineage]
- [ ] [depends:snapshot-lineage-baseline] Add a failing KVM regression that compares a depth-three incremental lineage with a full snapshot. r[chaoscontrol.deterministic_snapshots.incremental_validation]
- [ ] [depends:snapshot-lineage-regression] Add pure page-set planning with bounds checks and positive and negative unit tests. r[chaoscontrol.deterministic_snapshots.incremental_core]
- [ ] [depends:snapshot-lineage-core] Route incremental capture and restore through the page set and drain the dirty log around host writes. r[chaoscontrol.deterministic_snapshots.incremental_lineage] r[chaoscontrol.deterministic_snapshots.incremental_revert]
- [ ] [depends:snapshot-lineage-shell] Compare fresh and long-lived controller branch results from the same parent, including the watchdog replacement path. r[chaoscontrol.deterministic_snapshots.incremental_validation]
- [ ] [depends:snapshot-lineage-shell] Rerun the Raft control and planted-bug campaigns, replay every safety finding at depth three or more, and record overlay sizes per depth. r[chaoscontrol.deterministic_snapshots.incremental_validation]
- [ ] [depends:snapshot-lineage-campaign] Mark pre-change deep findings diagnostic-only in replay and readiness docs, then run workspace tests, KVM smoke tests, and Cairn validation. r[chaoscontrol.deterministic_snapshots.incremental_validation]
