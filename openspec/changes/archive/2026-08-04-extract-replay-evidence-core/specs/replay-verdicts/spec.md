## ADDED Requirements

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
