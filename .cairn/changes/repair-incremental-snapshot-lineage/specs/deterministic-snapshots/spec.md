# Deterministic Snapshots Specification

## Purpose

Keep incremental snapshots exact at every lineage depth.

## ADDED Requirements

### Requirement: Incremental snapshots are complete relative to their base

r[chaoscontrol.deterministic_snapshots.incremental_lineage] An incremental snapshot MUST contain every guest page whose content differs from its base image at capture time. Pages written by an earlier incremental restore and pages written by the guest MUST both be included.

#### Scenario: Depth-three lineage is captured

- GIVEN a VM restored from a depth-two overlay whose pages the next branch does not rewrite
- WHEN the branch ends and an incremental snapshot is captured
- THEN materializing that snapshot MUST produce the same guest memory as a full snapshot captured at the same point.

### Requirement: Incremental restore resets every non-base page

r[chaoscontrol.deterministic_snapshots.incremental_revert] Before writing a new overlay, incremental restore MUST revert every page that differs from the base, including pages the guest wrote after the previous restore.

#### Scenario: Sibling branch starts clean

- GIVEN a branch that wrote pages outside its parent overlay
- WHEN a sibling branch restores the same parent
- THEN guest memory after restore MUST equal the parent's materialized memory.

### Requirement: Page-set planning is pure

r[chaoscontrol.deterministic_snapshots.incremental_core] Revert, write, and next-set planning MUST be a pure function over supplied page sets, dirty bitmaps, overlay indices, and guest size. Out-of-range pages MUST fail before any memory access.

#### Scenario: Overlay page is out of range

- GIVEN an overlay page index beyond the guest memory size
- WHEN restore planning runs
- THEN it MUST return a typed failure and no guest page MAY change.

### Requirement: Lineage fidelity is validated

r[chaoscontrol.deterministic_snapshots.incremental_validation] Validation MUST compare incremental lineages against full snapshots at depths one through at least four. It MUST replay an exploration finding at replay-parent depth three or more, and it MUST show that a fresh controller and a long-lived controller produce equal branch results from the same parent snapshot.

#### Scenario: Deep replay restores

- GIVEN an exploration finding whose replay-parent depth is three or more
- WHEN snapshot-backed replay runs
- THEN restore preflight MUST accept the parent snapshot and execution MUST continue.

#### Scenario: Control campaign has no lineage artifacts

- GIVEN the Raft control workload without a planted bug and without memory-corrupting faults
- WHEN a bounded exploration reaches depth four or more
- THEN no safety assertion MAY fail.
