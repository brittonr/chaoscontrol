# replay-verdicts Specification

## Purpose

Replay verdicts define the Rust-owned, machine-readable evidence artifacts used to classify replay proof attempts without scraping raw logs, including snapshot-backed proof, replay gaps, invalid snapshot evidence, and scoped determinism wording.
## Requirements
### Requirement: Machine-readable replay verdict artifact [r[replay-verdicts.artifact]]
The system SHALL emit a machine-readable replay verdict artifact for replay proof attempts that can be validated without scraping raw logs.

#### Scenario: Verdict binds replay attempt [r[replay-verdicts.artifact.binds-attempt]]
- **GIVEN** a replay, reproduce, minimize, or snapshot smoke proof attempt runs against a bug candidate
- **WHEN** the attempt completes, fails before replay, or determines that no eligible bug exists
- **THEN** the system writes a verdict JSON artifact with schema version, command context, replay class, exit status, concise diagnostic, and timestamps or run identifiers
- **AND** the artifact references public evidence paths rather than embedding raw logs

#### Scenario: Verdict binds selected bug [r[replay-verdicts.artifact.binds-bug]]
- **GIVEN** the replay attempt selects a `bug_*.json` artifact
- **WHEN** the verdict is serialized
- **THEN** it records the bug artifact path, assertion id when available, replay parent depth, and whether a replay parent snapshot reference was present
- **AND** it records hashes for public artifacts needed to audit the verdict

### Requirement: Replay classification vocabulary [r[replay-verdicts.classification]]
The system SHALL classify replay attempts using stable replay classes that distinguish accepted snapshot-backed proof from schedule-only gaps, invalid evidence, and execution errors.

#### Scenario: Snapshot-backed reproduction is explicit [r[replay-verdicts.classification.snapshot-backed-reproduced]]
- **GIVEN** a selected bug has `replay_parent_depth > 0`, a valid replay parent snapshot reference, and standalone reproduce reports the target assertion failure
- **WHEN** the verdict is written
- **THEN** `replay_class` is `snapshot_backed_reproduced`
- **AND** the verdict records that snapshot artifact path confinement and digest validation succeeded

#### Scenario: Snapshot-backed non-reproduction is not schedule-only [r[replay-verdicts.classification.snapshot-backed-not-reproduced]]
- **GIVEN** a selected bug has `replay_parent_depth > 0` and a valid replay parent snapshot reference
- **WHEN** standalone reproduce completes without the target assertion failure
- **THEN** `replay_class` is `snapshot_backed_not_reproduced`
- **AND** the verdict preserves the result as a replay gap rather than reclassifying it as schedule-only evidence

#### Scenario: Schedule-only gaps are explicit [r[replay-verdicts.classification.schedule-only-gap]]
- **GIVEN** a run finds no eligible bug with nonzero replay parent depth or only bugs without required snapshot context
- **WHEN** evidence is retained from the run
- **THEN** the verdict uses `schedule_only_replay_gap`, `missing_snapshot_ref`, or `no_bug_found` as appropriate
- **AND** the verdict cannot be accepted as snapshot-backed replay proof

#### Scenario: Invalid snapshot evidence is explicit [r[replay-verdicts.classification.invalid-snapshot]]
- **GIVEN** a selected bug references a missing, path-escaping, unsupported-codec, or digest-mismatched snapshot artifact
- **WHEN** validation or replay attempts to classify the result
- **THEN** the verdict uses `missing_snapshot_artifact`, `invalid_snapshot_digest`, or `replay_error` as appropriate
- **AND** the verdict records the failing snapshot reference field without starting an assertion-level replay when fail-closed validation is possible

### Requirement: Verdict validation gates [r[replay-verdicts.validation]]
The system SHALL validate replay verdict artifacts through local evidence gates and fixtures before accepting dogfood replay proof.

#### Scenario: Accepted proof requires verdict [r[replay-verdicts.validation.accepted-proof]]
- **GIVEN** a dogfood receipt or smoke output claims snapshot-backed replay proof
- **WHEN** evidence validation runs
- **THEN** it requires a replay verdict artifact with `replay_class = snapshot_backed_reproduced`
- **AND** it verifies the verdict's bug, snapshot reference, digest, command status, and artifact hashes are consistent with committed evidence

#### Scenario: Negative fixtures reject overclaims [r[replay-verdicts.validation.negative-fixtures]]
- **GIVEN** fixtures for schedule-only replay, missing snapshot refs, invalid snapshot digests, and non-reproducing snapshot-backed bugs
- **WHEN** the evidence validation gate runs
- **THEN** each fixture is rejected as accepted snapshot-backed proof with a diagnostic naming the replay class or invalid field

### Requirement: Scoped determinism wording [r[replay-verdicts.scoped-determinism]]
The system SHALL document replay verdicts as scoped replay evidence and SHALL NOT present one workload verdict as proof of global deterministic hypervisor behavior.

#### Scenario: Snapshot replay proof wording is scoped [r[replay-verdicts.scoped-determinism.wording]]
- **GIVEN** documentation, receipts, or smoke summaries describe `snapshot_backed_reproduced`
- **WHEN** they explain the result
- **THEN** they state that the verdict proves the selected snapshot-backed workload replay rail
- **AND** they avoid claiming global deterministic hypervisor proof across arbitrary workloads, devices, and timing paths

### Requirement: Shared replay/evidence core DTO boundary [r[replay-verdicts.shared-core.dto-boundary]]
The system SHALL define a shared Rust-owned replay/evidence core for replay verdict DTOs, artifact hash DTOs, snapshot reference DTOs, replay parent snapshot reference DTOs, replay class values, and validation status values that are currently consumed by both exploration and evidence-readiness code.

#### Scenario: Explorer and evidence gate use one DTO authority [r[replay-verdicts.shared-core.dto-boundary.shared-authority]]
- **GIVEN** `chaoscontrol-explore` emits a replay verdict artifact and `chaoscontrol-evidence` validates that artifact
- **WHEN** the shared core migration is complete
- **THEN** both crates use the same Rust-owned DTO definitions or compatibility re-exports for verdict, artifact hash, snapshot ref, replay parent snapshot ref, replay class, and validation status fields
- **AND** public JSON field names remain stable unless a separate spec admits a breaking change

### Requirement: Shared replay/evidence validation fixtures [r[replay-verdicts.shared-core.fixtures]]
The shared core SHALL include positive and negative fixture coverage for replay evidence DTO compatibility and fail-closed validation.

#### Scenario: Current positive verdicts remain accepted [r[replay-verdicts.shared-core.positive-fixtures]]
- **GIVEN** current explorer verdict output, evidence readiness accepted proof records, snapshot-backed reproduced verdicts, schedule-only gaps, and no-bug classifications
- **WHEN** shared core validation runs
- **THEN** valid records are accepted with the same public replay class and artifact reference semantics as before migration

#### Scenario: Invalid verdicts fail closed [r[replay-verdicts.shared-core.negative-fixtures]]
- **GIVEN** a verdict or evidence record has a malformed artifact hash, missing snapshot reference, invalid snapshot digest, path-escaping artifact ref, unsupported replay class, stale artifact hash, non-reproducing snapshot-backed bug claimed as accepted proof, or global-determinism overclaim
- **WHEN** shared core validation runs
- **THEN** validation fails with a deterministic diagnostic naming the invalid field or unsupported claim

### Requirement: Shared core remains bounded [r[replay-verdicts.shared-core.pure-core]]
The shared replay/evidence core SHALL be pure deterministic logic over in-memory DTOs and SHALL NOT perform VM execution, filesystem traversal, clock access, process execution, KVM interaction, or receipt writing.

#### Scenario: Shell owns effects [r[replay-verdicts.shared-core.pure-core.shell-boundary]]
- **GIVEN** a CLI, explorer, replay, or evidence-readiness command loads bug files, snapshot files, dogfood manifests, or Nickel-rendered contracts
- **WHEN** it invokes shared core validation
- **THEN** all file, VM, clock, process, and contract-rendering effects remain outside the shared core
- **AND** the shared core returns only DTOs, classifications, diagnostics, or compatibility decisions

