## ADDED Requirements

### Requirement: Snapshot-backed workload replay evidence [r[replay-parent-snapshots.workload-evidence]]
The system MUST provide a repeatable workload evidence rail that exercises persisted replay parent snapshot artifacts outside fixture-only tests.

#### Scenario: Workload emits a parent-context bug [r[replay-parent-snapshots.workload-evidence.parent-context-bug]]
- **GIVEN** a targeted workload campaign or imported committed checkpoint selected to exercise parent-context replay
- **WHEN** the campaign emits or exports bug evidence for a branch whose replay parent depth is greater than zero
- **THEN** at least one retained evidence candidate includes `replay_parent_depth > 0`
- **AND** before acceptance, that bug includes a non-null `replay_parent_snapshot_ref` whose artifact is present under the bounded run output store

#### Scenario: Snapshot-backed reproduce is classified [r[replay-parent-snapshots.workload-evidence.reproduce-classification]]
- **GIVEN** a bug artifact with nonzero replay parent depth and a valid replay parent snapshot reference
- **WHEN** standalone reproduce or minimize runs against that bug
- **THEN** the receipt classifies the result as snapshot-backed replay success, snapshot-backed replay failure/gap, or skipped/missing-artifact before any schedule-only interpretation is made
- **AND** the receipt records the command, exit status, concise diagnostic, and artifact hashes needed to audit the classification

#### Scenario: Schedule-only and no-bug attempts remain explicit [r[replay-parent-snapshots.workload-evidence.coverage-gap]]
- **GIVEN** a targeted run finds no bugs or only bugs with `replay_parent_depth = 0`
- **WHEN** evidence is retained from that run
- **THEN** the retained receipt labels the result as a workload coverage gap rather than snapshot-backed replay proof
- **AND** raw logs, temporary minimizer logs, and voluminous checkpoints remain local unless intentionally summarized and hash-bound

#### Scenario: Interrupted campaign uses export-bugs finalization [r[replay-parent-snapshots.workload-evidence.export-finalization]]
- **GIVEN** an interrupted campaign checkpoint contains bug records that normal end-of-run finalization did not write as `bug_N.json`
- **WHEN** the operator finalizes the run for evidence review
- **THEN** the operator uses `chaoscontrol-explore export-bugs --checkpoint <checkpoint.json> --output <run-dir>` or an equivalent checked wrapper
- **AND** export fails early when any parent-context bug lacks a durable replay parent snapshot reference
