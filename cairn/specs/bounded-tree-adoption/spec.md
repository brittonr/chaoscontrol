# Bounded Tree Adoption Specification

## Purpose

Defines the `bounded-tree-adoption` capability.

## Requirements

### Requirement: Completed immutable prerequisite

r[chaoscontrol.bounded_tree_adoption.prerequisite] ChaosControl MUST adopt only a completed `bounded-tree` release pinned to one immutable Radicle revision.

#### Scenario: Reviewed dependency is available
- GIVEN passing `bounded-tree` completion evidence and one immutable Radicle revision
- WHEN ChaosControl dependency admission runs
- THEN the exact revision MUST be accepted without a sibling path or fallback

#### Scenario: Shared repository is incomplete
- GIVEN a failed gate, unchecked implementation task, mutable revision, or missing Radicle source
- WHEN admission runs
- THEN adoption MUST remain blocked

### Requirement: Shared source-tree observation

r[chaoscontrol.bounded_tree_adoption.tree_observation] ChaosControl MUST preserve accepted source members and rejection behavior while using shared bounded observations.

#### Scenario: Valid source tree is observed
- GIVEN a valid initrd source tree
- WHEN old and shared-backed collection paths run
- THEN their ordered source members MUST match

#### Scenario: Unsafe source tree is observed
- GIVEN an invalid path, unsupported file, changed source, exceeded bound, or disallowed link
- WHEN both collection paths run
- THEN their stable failure classes MUST match

### Requirement: Newc archive boundary

r[chaoscontrol.bounded_tree_adoption.archive_boundary] ChaosControl MUST retain archive path mapping, Newc encoding, modes, inode assignment, padding, duplicate policy, and output limits.

#### Scenario: Valid initrd is built
- GIVEN an accepted source tree fixture
- WHEN old and shared-backed builders run
- THEN their complete Newc archive bytes MUST match

#### Scenario: Generic copy plan replaces archive policy
- GIVEN an adapter that treats a shared copy plan as Newc archive semantics
- WHEN boundary review runs
- THEN the adapter MUST be rejected

### Requirement: ChaosControl evidence boundary

r[chaoscontrol.bounded_tree_adoption.evidence_boundary] ChaosControl MUST retain kernel-bundle, boot, module, BPF, replay, readiness, and cleanup evidence meaning.

#### Scenario: Shared observation is recorded
- GIVEN a passing shared source observation
- WHEN ChaosControl writes kernel-bundle evidence
- THEN it MAY record the exact dependency and bounded observation result

#### Scenario: Shared observation proves guest behavior
- GIVEN evidence that promotes tree observation to boot correctness or deterministic replay
- WHEN evidence validation runs
- THEN validation MUST fail

### Requirement: Positive and negative parity gate

r[chaoscontrol.bounded_tree_adoption.parity] ChaosControl SHALL remove local observation mechanics only after byte-level positive parity and negative fixture parity pass.

#### Scenario: Complete parity passes
- GIVEN matching archive bytes and matching failure classes
- WHEN cutover readiness is evaluated
- THEN duplicated observation mechanics MAY be removed

#### Scenario: Any parity case differs
- GIVEN an entry, archive byte, or failure mismatch
- WHEN cutover readiness is evaluated
- THEN local observation mechanics MUST remain active

### Requirement: Coupled rollback

r[chaoscontrol.bounded_tree_adoption.rollback] ChaosControl MUST record a rollback that restores prior observation code and dependency state together.

#### Scenario: Adoption regression appears
- GIVEN a post-cutover initrd identity or failure regression
- WHEN rollback executes
- THEN the recorded pre-adoption source and dependency state MUST be restored

#### Scenario: Rollback is incomplete
- GIVEN a rollback that restores only source or dependency state
- WHEN rollback validation runs
- THEN validation MUST fail
