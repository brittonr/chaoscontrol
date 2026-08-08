## ADDED Requirements

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
