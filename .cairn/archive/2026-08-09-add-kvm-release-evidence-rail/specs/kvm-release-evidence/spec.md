# KVM Release Evidence Specification

## Purpose

Require a fresh, complete, bounded KVM behavior receipt for release eligibility.

## ADDED Requirements

### Requirement: KVM release evidence uses a typed matrix

r[chaoscontrol.kvm_release_rail.matrix] ChaosControl MUST define a typed Nickel matrix that binds required rows, worker capabilities, commands, limits, retained artifacts, and non-claims.

#### Scenario: Matrix is complete
- GIVEN every row has one capability predicate, command, finite bound, and terminal policy
- WHEN matrix validation runs
- THEN it MUST produce one deterministic runtime projection.

#### Scenario: Matrix omits a bound
- GIVEN one required row lacks a finite execution or artifact bound
- WHEN matrix validation runs
- THEN validation MUST fail.

### Requirement: Required KVM rows are explicit

r[chaoscontrol.kvm_release_rail.required_rows] The base release matrix MUST cover deterministic SMP, serialized snapshot replay, production virtio malformed-input survival, one admitted drift profile, and one fresh workload replay. PMU claims MUST add a matching required row.

#### Scenario: Base rows pass
- GIVEN every base row runs on an admitted worker and passes
- WHEN matrix classification runs
- THEN the matrix MAY proceed to receipt validation.

### Requirement: Required unsupported rows block release

r[chaoscontrol.kvm_release_rail.worker] Missing capability, skipped execution, timeout, absence, or unsupported status MUST NOT count as a passed required row.

#### Scenario: Worker lacks one required capability
- GIVEN a required row cannot run on the selected worker
- WHEN the runner records `unsupported`
- THEN the release verdict MUST remain blocked with the missing capability.

### Requirement: KVM receipts bind the cohort

r[chaoscontrol.kvm_release_rail.receipt] A matrix receipt MUST bind source and dirty state, runner, kernel, KVM and host facts, binaries, guests, profile, command identities, limits, row outcomes, and retained artifact identities.

#### Scenario: Artifact identity drifts
- GIVEN one retained row artifact differs from its receipt identity
- WHEN receipt validation runs
- THEN the whole matrix MUST fail closed.

### Requirement: KVM verdicts have a functional core

r[chaoscontrol.kvm_release_rail.functional_core] Matrix shape, capability, freshness, row, artifact, and terminal decisions MUST be pure deterministic logic. Host queries, KVM execution, clocks, files, and publication MUST remain in shells.

#### Scenario: Identical matrix facts are replayed
- GIVEN identical matrix and observation facts
- WHEN the core evaluates them twice
- THEN both verdicts MUST be identical.

### Requirement: Portable and KVM CI remain separate

r[chaoscontrol.kvm_release_rail.ci] Portable CI MUST NOT claim KVM behavior. KVM CI MUST publish a distinct bounded receipt from an admitted worker.

#### Scenario: Portable CI passes without KVM receipt
- GIVEN all portable checks pass but no fresh KVM receipt exists
- WHEN release eligibility runs
- THEN KVM release evidence MUST remain unsatisfied.

### Requirement: KVM claims remain bounded

r[chaoscontrol.kvm_release_rail.boundary] Passing the matrix MUST NOT claim worker integrity, all-host equivalence, universal determinism, workload correctness, or production availability.

#### Scenario: Receipt is used as universal host proof
- GIVEN one admitted worker passes
- WHEN a report claims all supported hosts are equivalent
- THEN claim validation MUST reject the report.

### Requirement: KVM release validation is adversarial

r[chaoscontrol.kvm_release_rail.validation] Validation MUST pair a complete matrix with missing, stale, skipped, unsupported, timed-out, tampered, dirty-source, and overclaim cases.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to enable the release gate
- WHEN pure, KVM, CI, receipt, and lifecycle validation runs
- THEN every positive and negative class MUST produce its expected result.
