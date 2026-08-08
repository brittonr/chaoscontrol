## ADDED Requirements

### Requirement: Rust workload snapshot replay proof [r[rust-workload-harness.snapshot-replay-proof]]

ChaosControl MUST provide an opt-in Rust workload snapshot replay proof rail that converts the downstream-shaped Rust workload from bounded VM campaign evidence into accepted snapshot-backed replay evidence only when a persisted replay parent snapshot, exported bug, and reproduced replay verdict are all present.

#### Scenario: Probe is opt-in [r[rust-workload-harness.snapshot-replay-proof.opt-in]]

- GIVEN the Rust workload guest is run without the snapshot probe cmdline flag
- WHEN the workload executes local or bounded VM campaign behavior
- THEN the snapshot replay probe does not intentionally fail assertions

#### Scenario: Accepted replay verdict is required [r[rust-workload-harness.snapshot-replay-proof.accepted-verdict]]

- GIVEN the Rust workload snapshot probe is enabled and a parent-context bug is exported
- WHEN the replay proof rail runs standalone reproduce with `--verdict-output`
- THEN the proof is accepted only if the verdict has `replay_class = snapshot_backed_reproduced`, `reproduced = true`, `replay_parent_depth > 0`, and a valid digest-verified snapshot artifact

#### Scenario: Coverage manifest includes Rust workload [r[rust-workload-harness.snapshot-replay-proof.coverage-manifest]]

- GIVEN accepted Rust workload replay proof evidence exists
- WHEN replay proof coverage and readiness reports are generated
- THEN the Rust workload appears as a distinct accepted workload proof without weakening the existing Raft, redb, and net proof requirements
