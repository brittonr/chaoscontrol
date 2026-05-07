# replay-parent-snapshots Specification

## Purpose
TBD - created by archiving change persist-replay-parent-snapshots. Update Purpose after archive.
## Requirements
### Requirement: Replay parent snapshot references [r[replay-parent-snapshots.references]]
The system SHALL persist public bug and receipt evidence for parent-context replay using a stable `replay_parent_snapshot_ref` envelope rather than embedding large snapshot payloads directly in bug JSON.

#### Scenario: Bug evidence names required parent context [r[replay-parent-snapshots.references.bug-ref]]
- **GIVEN** a bug discovered from a branch whose replay parent depth is greater than zero
- **WHEN** the bug evidence is serialized for a dogfood or exploration run
- **THEN** the bug record includes a `replay_parent_snapshot_ref` with store kind, digest, codec, schema version, and artifact locator
- **AND** the bug record still includes schedule and assertion metadata needed to replay the branch

#### Scenario: Schedule-only bugs remain compatible [r[replay-parent-snapshots.references.schedule-only]]
- **GIVEN** a bug that can be replayed from a fresh bootstrap snapshot using only its schedule
- **WHEN** the bug evidence is serialized
- **THEN** the snapshot reference may be absent
- **AND** existing consumers can distinguish schedule-only replay from parent-context replay using replay context metadata

### Requirement: Rust-owned snapshot artifact store [r[replay-parent-snapshots.store]]
The system SHALL provide a Rust-owned `SnapshotStore` boundary for persisted replay snapshots with operations to put, get, test for presence, and retire or garbage-collect snapshot artifacts by digest.

#### Scenario: File-backed store writes content-addressed snapshots [r[replay-parent-snapshots.store.file-backed]]
- **GIVEN** a parent `SimulationSnapshot` captured during exploration
- **WHEN** the file-backed store persists it under a run output directory
- **THEN** the store writes a content-addressed snapshot artifact whose digest matches the public reference
- **AND** the artifact can be copied with the run directory and loaded without a database server

#### Scenario: Optional redb store remains host-side [r[replay-parent-snapshots.store.redb-host-side]]
- **GIVEN** the optional redb-backed snapshot store is enabled
- **WHEN** snapshots are persisted or indexed through redb
- **THEN** the store is clearly documented and named as a host evidence store
- **AND** the public bug and receipt evidence still exposes hash-addressed JSON references rather than requiring redb inspection

#### Scenario: Snapshot bytes remain Rust-derived [r[replay-parent-snapshots.store.rust-derived]]
- **GIVEN** a persisted snapshot artifact
- **WHEN** evidence ownership is classified
- **THEN** snapshot bytes are classified as Rust-derived runtime artifacts
- **AND** Nickel validates only the reference envelope, digest, codec, schema version, and receipt linkage

### Requirement: Replay loads required parent snapshots [r[replay-parent-snapshots.replay-load]]
The standalone replay and minimization paths SHALL load a required parent snapshot from the snapshot store before executing a saved schedule whose bug record has nonzero `replay_parent_depth`.

#### Scenario: Parent snapshot replay succeeds [r[replay-parent-snapshots.replay-load.success]]
- **GIVEN** a bug record with nonzero `replay_parent_depth` and a valid `replay_parent_snapshot_ref`
- **WHEN** standalone replay runs against the bug record and output directory
- **THEN** replay restores the referenced parent snapshot before applying the saved schedule
- **AND** the reproduction verdict is based on that restored context rather than a fresh bootstrap schedule-only attempt

#### Scenario: Missing required snapshot fails early [r[replay-parent-snapshots.replay-load.missing]]
- **GIVEN** a bug record that requires parent context but the referenced snapshot artifact is missing
- **WHEN** standalone replay validates inputs
- **THEN** replay exits before starting VMs with a diagnostic that names the missing snapshot reference
- **AND** the result is not reported as an assertion-level non-reproduction

### Requirement: Snapshot reference validation gates [r[replay-parent-snapshots.validation]]
The system SHALL validate replay parent snapshot references through local contract/checker gates that accept valid artifacts and reject incomplete or tampered evidence.

#### Scenario: Wrong digest is rejected [r[replay-parent-snapshots.validation.wrong-digest]]
- **GIVEN** a bug or receipt fixture whose snapshot reference digest does not match the referenced snapshot artifact bytes
- **WHEN** evidence contract validation runs
- **THEN** validation fails with an error identifying the digest mismatch

#### Scenario: Unsupported codec is rejected [r[replay-parent-snapshots.validation.unsupported-codec]]
- **GIVEN** a snapshot reference with an unsupported codec or incompatible schema version
- **WHEN** replay or contract validation attempts to load the artifact
- **THEN** validation fails before replay starts with an actionable compatibility diagnostic

#### Scenario: Raw logs and secrets remain excluded [r[replay-parent-snapshots.validation.exclusions]]
- **GIVEN** raw `run.log` or `reproduce.log` files, secrets, or guest workload database contents exist locally
- **WHEN** snapshot artifact validation runs for committed evidence
- **THEN** those surfaces are not required as committed acceptance evidence
- **AND** validation rejects snapshot references that point outside the bounded run artifact store

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

#### Scenario: Second workload replay proof [r[replay-parent-snapshots.workload-evidence.second-workload-proof]]
- **GIVEN** the Raft snapshot replay rail already has accepted machine-readable verdict evidence
- **WHEN** the project broadens replay evidence toward an Antithesis-like internal rail
- **THEN** at least one retained non-Raft workload evidence directory includes a selected bug with `replay_parent_depth > 0`, a valid digest-checked `replay_parent_snapshot_ref`, and a `snapshot_backed_reproduced` replay verdict
- **AND** the evidence identifies the workload, assertion ID, cmdline probe parameters, export filter, reproduce command class, and artifact hashes without committing raw logs or checkpoints

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

### Requirement: Replay readiness status report [r[replay-parent-snapshots.readiness-status]]
ChaosControl MUST publish an operator-facing replay readiness status report generated from committed proof evidence rather than hand-authored claims.

#### Scenario: Report separates readiness surfaces [r[replay-parent-snapshots.readiness-status.surfaces]]
- **GIVEN** the accepted workload proof manifest contains committed proof entries
- **WHEN** the readiness report is generated
- **THEN** it lists supported snapshot-backed replay surfaces from those proofs
- **AND** it separately lists experimental or unproven surfaces without promoting them to supported status

#### Scenario: Stale report fails validation [r[replay-parent-snapshots.readiness-status.stale]]
- **GIVEN** the manifest or report content changes
- **WHEN** the readiness report check runs
- **THEN** it MUST fail unless the committed Markdown report exactly matches the generated content

#### Scenario: Scope anti-claims are preserved [r[replay-parent-snapshots.readiness-status.anti-claims]]
- **GIVEN** the report describes accepted workload proof coverage
- **WHEN** an operator reads the generated status
- **THEN** it MUST state that the evidence is not a mathematical proof and not a universal deterministic hypervisor claim

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
