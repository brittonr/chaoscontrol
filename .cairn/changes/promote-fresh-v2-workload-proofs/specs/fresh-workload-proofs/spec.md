# Fresh Workload Proofs Specification

## Purpose

Promote current workload evidence only from fresh, strict, bounded schema-v2 KVM runs.

## ADDED Requirements

### Requirement: Fresh proofs use one typed cohort

r[chaoscontrol.fresh_workload_proofs.profile] ChaosControl MUST use a typed profile that binds source, host, guest, KVM, assertion catalog, run, snapshot, replay, artifact, bound, and non-claim facts.

#### Scenario: Complete cohort is admitted
- GIVEN every required identity and finite bound is present
- WHEN cohort admission runs
- THEN it MUST produce one deterministic runtime projection before KVM activation.

#### Scenario: Cohort is incomplete
- GIVEN one required identity, bound, or non-claim is absent
- WHEN cohort admission runs
- THEN it MUST fail before guest progress.

### Requirement: Promotion rejects inherited legacy authority

r[chaoscontrol.fresh_workload_proofs.admission] A promoted workload proof MUST come from current schema-v2 execution with an accepted strict assertion catalog. Legacy numeric assertion identity MUST remain diagnostic-only.

#### Scenario: Fresh strict carrier is evaluated
- GIVEN a current bug and replay verdict bind one accepted strict catalog and exact snapshot
- WHEN promotion admission runs
- THEN the carrier MAY proceed to full proof validation.

#### Scenario: Historical carrier is evaluated
- GIVEN a carrier has schema-v1 identity or only a numeric assertion ID
- WHEN promotion admission runs
- THEN it MUST remain blocked and MUST NOT inherit authority from a newer catalog.

### Requirement: Raft closes the first proof gap

r[chaoscontrol.fresh_workload_proofs.raft_first] The first promoted cohort MUST include one fresh Raft KVM run with exact snapshot-backed reproduction and complete receipt linkage.

#### Scenario: Raft replay reproduces
- GIVEN the admitted Raft cohort finds a strict assertion failure
- WHEN replay restores the exact parent snapshot and schedule
- THEN the verdict MAY become eligible for bounded promotion.

### Requirement: Workload coverage uses one admission rule

r[chaoscontrol.fresh_workload_proofs.coverage] Raft, Redb, network, and the downstream-shaped Rust workload MUST use the same freshness, identity, snapshot, replay, artifact, and receipt admission rules.

#### Scenario: One workload uses weaker evidence
- GIVEN a workload omits a required strict identity or replay fact
- WHEN aggregate coverage runs
- THEN that workload MUST remain blocked without weakening other rows.

### Requirement: Rust onboarding ends with a typed classification

r[chaoscontrol.fresh_workload_proofs.onboarding] ChaosControl MUST provide one bounded Rust workload flow from scaffold through build, KVM run, replay attempt, and promotion classification.

#### Scenario: New workload completes without a bug
- GIVEN a scaffold builds and its bounded run finds no selected failure
- WHEN the flow completes
- THEN it MUST report a valid no-bug diagnostic result and MUST NOT claim replay proof.

### Requirement: Proof decisions have a functional core

r[chaoscontrol.fresh_workload_proofs.functional_core] Freshness, identity, linkage, replay-class, blocker, and promotion decisions MUST be pure deterministic logic. KVM, files, processes, clocks, and publication MUST remain in shells.

#### Scenario: Identical proof facts are classified twice
- GIVEN identical loaded manifests and observations
- WHEN classification runs twice
- THEN both results MUST have identical decisions and diagnostics.

### Requirement: Fresh proof claims remain bounded

r[chaoscontrol.fresh_workload_proofs.boundary] A promoted proof MUST NOT claim workload correctness, universal determinism, host equivalence, or release eligibility beyond its admitted cohort.

#### Scenario: Report promotes a bounded replay to universal correctness
- GIVEN one workload replay reproduces
- WHEN claim validation reads a universal correctness statement
- THEN validation MUST reject the statement.

### Requirement: Fresh proof validation is adversarial

r[chaoscontrol.fresh_workload_proofs.validation] Validation MUST pair positive fresh strict proofs with negative legacy, stale, conflicting, tampered, incomplete, missing-KVM, and overclaim cases.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to archive the change
- WHEN focused and KVM validation runs
- THEN every positive and negative class MUST produce its expected typed result.
