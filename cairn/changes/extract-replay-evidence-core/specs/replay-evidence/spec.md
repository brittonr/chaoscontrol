# Replay Evidence Specification

## ADDED Requirements

### Requirement: Replay evidence has one Rust DTO authority

r[chaoscontrol.replay_evidence.shared_authority] ChaosControl MUST define replay verdict, artifact hash, snapshot reference, replay parent reference, replay class, validation status, and bounded diagnostic DTOs in one shared Rust core used by explorer emission and evidence validation.

#### Scenario: Explorer and evidence validation process one artifact

r[chaoscontrol.replay_evidence.shared_authority.scenario.shared]
- GIVEN the explorer emits an accepted replay verdict and the evidence rail validates it
- WHEN both paths construct or decode replay DTOs
- THEN they MUST use the shared definitions or explicit compatibility re-exports from the same core authority

#### Scenario: A shell introduces a second replay class model

r[chaoscontrol.replay_evidence.shared_authority.scenario.duplicate]
- GIVEN explorer or evidence shell code defines a competing verdict, hash, snapshot reference, replay class, or validation status model
- WHEN architecture validation runs
- THEN it MUST fail or require removal of the duplicate semantic authority

### Requirement: Replay classification is a functional core

r[chaoscontrol.replay_evidence.core_boundary] Replay artifact validation and classification MUST be pure deterministic logic over explicit in-memory run, bug, snapshot, exit, command, and artifact facts.

#### Scenario: Valid supplied facts produce a verdict

r[chaoscontrol.replay_evidence.core_boundary.scenario.accepted]
- GIVEN a valid run identity, admitted bug facts, explicit snapshot validation, exit observation, command summary, and checked artifact hashes
- WHEN replay classification runs
- THEN it MUST return the accepted verdict and diagnostics without reading files, clocks, processes, VMs, environment state, or Nickel sources

#### Scenario: Facts are missing or contradictory

r[chaoscontrol.replay_evidence.core_boundary.scenario.rejected]
- GIVEN a required snapshot reference is absent, an artifact hash is malformed or stale, a legacy identity is unadmitted, or exit facts contradict the requested replay class
- WHEN classification runs
- THEN it MUST reject with deterministic bounded diagnostics and MUST NOT fabricate runtime observations

### Requirement: Replay host effects remain in shells

r[chaoscontrol.replay_evidence.shell_boundary] Artifact discovery, file reads and writes, create-new publication, run-ID allocation, checkpoint and snapshot access, VM execution, process control, clocks, logging, Nickel checks, and report rendering MUST remain outside the replay evidence core.

#### Scenario: Shell publishes an admitted verdict

r[chaoscontrol.replay_evidence.shell_boundary.scenario.publish]
- GIVEN the shell has collected explicit runtime and artifact observations and the core admits a verdict
- WHEN the shell publishes that verdict
- THEN the shell MAY serialize and write the artifact under its existing bounded mutation policy
- AND publication success MUST remain a shell observation rather than a core classification input fabricated in advance

#### Scenario: Core imports host authority

r[chaoscontrol.replay_evidence.shell_boundary.scenario.forbidden]
- GIVEN the replay core imports filesystem, clock, process, network, KVM, async-runtime, logging, CLI, or Nickel runtime authority
- WHEN dependency or source validation runs
- THEN validation MUST fail with the forbidden authority class

### Requirement: Replay artifacts remain wire compatible

r[chaoscontrol.replay_evidence.compatibility] The shared-core migration MUST preserve accepted JSON field names, enum spellings, optional-field behavior, SHA-256 interoperability fields, diagnostics, and replay classifications unless a separate versioned change approves a difference.

#### Scenario: Accepted verdict bytes cross the new boundary

r[chaoscontrol.replay_evidence.compatibility.scenario.bytes]
- GIVEN an accepted snapshot-backed, schedule-only, invalid, or no-bug fixture
- WHEN legacy and migrated paths serialize the verdict
- THEN their public JSON structure and semantic classification MUST remain equal

#### Scenario: Malformed artifacts remain rejected

r[chaoscontrol.replay_evidence.compatibility.scenario.negative]
- GIVEN a malformed digest, wrong algorithm tag, path escape, missing snapshot reference, stale artifact hash, unsupported class, contradictory exit, or overclaim text
- WHEN shared validation runs
- THEN it MUST reject with the accepted failure class and MUST NOT report accepted replay evidence

### Requirement: Replay architecture checks are enforced

r[chaoscontrol.replay_evidence.architecture_guard] ChaosControl MUST run dependency, source-purity, positive shell, and negative forbidden-import checks for the replay evidence core on every declared target.

#### Scenario: Dependency direction is reversed

r[chaoscontrol.replay_evidence.architecture_guard.scenario.direction]
- GIVEN the replay core depends on explorer or evidence shell code, or a shell-owned type enters the core API without an adapter
- WHEN architecture checks run
- THEN they MUST fail with a deterministic dependency-direction diagnostic

### Requirement: Replay evidence claims remain bounded

r[chaoscontrol.replay_evidence.claim_boundary] ChaosControl MUST report shared-core success as evidence of DTO consistency, supplied-fact validation, and evaluated dependency direction only.

#### Scenario: All shared-core checks pass

r[chaoscontrol.replay_evidence.claim_boundary.scenario.non-claim]
- GIVEN core, compatibility, shell, and architecture checks pass
- WHEN evidence is summarized
- THEN it MUST NOT claim VM correctness, snapshot correctness, deterministic replay, bug reproducibility outside the supplied run, host safety, or release readiness
