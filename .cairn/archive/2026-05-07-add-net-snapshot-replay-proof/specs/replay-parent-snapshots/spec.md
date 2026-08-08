## ADDED Requirements

### Requirement: Network workload accepted replay proof [r[replay-parent-snapshots.workload-evidence.net-workload-proof]]
The system MUST retain a bounded networking workload proof before listing the net replay rail as supported-bounded.

#### Scenario: Net workload emits accepted snapshot verdict [r[replay-parent-snapshots.workload-evidence.net-workload-proof.accepted-verdict]]
- **GIVEN** the networking guest is run with `net_bug=snapshot_replay_probe`
- **WHEN** targeted checkpoint export selects a bug with `replay_parent_depth > 0`
- **THEN** the retained evidence includes a selected bug, valid replay parent snapshot artifact, and replay verdict with `replay_class = snapshot_backed_reproduced`
- **AND** the evidence identifies the net assertion ID, cmdline probe parameters, export filter, reproduce command class, and artifact hashes without committing raw logs or checkpoints

#### Scenario: Net workload support remains scoped [r[replay-parent-snapshots.workload-evidence.net-workload-proof.scoped]]
- **GIVEN** the accepted workload manifest lists the net proof
- **WHEN** replay proof coverage and readiness docs are generated
- **THEN** they MUST list only the bounded net snapshot-backed replay rail as supported
- **AND** they preserve anti-claims against mathematical determinism or universal hypervisor proof
