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
