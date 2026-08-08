## ADDED Requirements

### Requirement: Snapshot replay smoke check [r[replay-parent-snapshots.smoke-check]]
The system SHALL expose an explicit KVM-required smoke gate that exercises snapshot-backed replay through the real Raft workload, checkpoint export, snapshot artifact validation, and standalone reproduce path.

#### Scenario: Smoke check proves snapshot-backed reproduce [r[replay-parent-snapshots.smoke-check.reproduce]]
- **GIVEN** a host capable of running KVM-backed ChaosControl simulations
- **WHEN** the snapshot replay smoke check runs
- **THEN** it executes the targeted Raft `snapshot_replay_probe` workload with bounded parameters
- **AND** it finalizes checkpoint-held bugs with `chaoscontrol-explore export-bugs`
- **AND** it selects at least one bug with `replay_parent_depth > 0` and a non-null `replay_parent_snapshot_ref`
- **AND** it verifies the referenced snapshot artifact is present, path-confined, and SHA-256 digest-matching
- **AND** it runs standalone reproduce against the selected bug and requires a `BUG REPRODUCED` verdict

#### Scenario: Smoke check keeps raw logs ephemeral [r[replay-parent-snapshots.smoke-check.ephemeral-logs]]
- **GIVEN** the smoke check captures run or reproduce logs for diagnostics
- **WHEN** the check succeeds or fails
- **THEN** raw logs remain in the temporary build or scratch directory
- **AND** the committed repository only carries the script/check wiring and any intentionally curated concise evidence
