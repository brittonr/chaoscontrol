# Asynchronous Flow Fault Campaign Specification Delta

## ADDED Requirements

### Requirement: Flow campaign profile is typed and bounded

r[chaoscontrol.async_flow.profile] ChaosControl MUST define a typed Nickel campaign profile for exact cohorts, roles, placements, flows, laws, assumptions, faults, assertions, observations, bounds, replay, and non-claims.

#### Scenario: Complete profile is admitted

r[chaoscontrol.async_flow.profile.valid]
- GIVEN a profile contains every required identity, bound, assertion, observation, and non-claim
- WHEN profile admission runs
- THEN ChaosControl MUST produce one deterministic campaign plan.

#### Scenario: Profile omits a required bound

r[chaoscontrol.async_flow.profile.invalid]
- GIVEN a profile omits or widens one required execution, message, observation, artifact, snapshot, replay, or time bound
- WHEN profile admission runs
- THEN ChaosControl MUST reject it before execution.

### Requirement: Producer, proof, runtime, and observation cohorts are exact

r[chaoscontrol.async_flow.cohorts] ChaosControl MUST consume immutable Choregraph, Trellis, Lattice, and protocol-observation cohorts through versioned narrow adapters with exact source, schema, artifact, proof, assumption, fixture, and non-claim identities.

#### Scenario: All cohorts match

r[chaoscontrol.async_flow.cohorts.valid]
- GIVEN every selected cohort and adapter has its exact supported identity
- WHEN campaign admission runs
- THEN ChaosControl MAY construct the selected flow cases.

#### Scenario: Law evidence is for another reducer

r[chaoscontrol.async_flow.cohorts.wrong_law]
- GIVEN the Trellis law cohort covers another operation, domain, verifier, assumption set, or source revision
- WHEN campaign admission runs
- THEN ChaosControl MUST reject the cohort as incompatible.

### Requirement: Expected outcomes are independent

r[chaoscontrol.async_flow.oracle] Each selected flow case MUST have an independently reviewed expected-outcome fixture and pure oracle. The Lattice runtime under test MUST NOT generate its only expected result.

#### Scenario: Complete set union is evaluated

r[chaoscontrol.async_flow.oracle.valid]
- GIVEN complete admitted logical item input, exact closure, and complete observations
- WHEN the independent set-union oracle runs
- THEN it MUST compute the canonical expected union without calling the runtime under test.

#### Scenario: Runtime output is copied into expectation

r[chaoscontrol.async_flow.oracle.self]
- GIVEN the only expected value comes from the Lattice result under test
- WHEN oracle admission runs
- THEN ChaosControl MUST reject the oracle as tautological.

### Requirement: The campaign covers selected flow fault cases

r[chaoscontrol.async_flow.matrix] The campaign MUST expand bounded cases for fault-free execution, reordering, duplication, delay, loss, partition, heal, role termination, restart, uncertainty, closure, valid prefixes, and replay.

#### Scenario: Selected matrix is complete

r[chaoscontrol.async_flow.matrix.valid]
- GIVEN the admitted profile selects the first asynchronous-flow campaign
- WHEN case expansion runs
- THEN every required case MUST have one exact identity, schedule, expectation, assertion set, and bound set.

#### Scenario: Required case is missing

r[chaoscontrol.async_flow.matrix.missing]
- GIVEN one selected fault or boundary case has no exact expansion
- WHEN matrix validation runs
- THEN the campaign MUST fail before execution.

### Requirement: Flow outcomes distinguish completion from prefixes

r[chaoscontrol.async_flow.outcomes] ChaosControl MUST classify complete convergence, valid incomplete prefix, expected block, explicit uncertainty, assertion violation, unsupported, incomplete observation, and indeterminate outcomes separately.

#### Scenario: Reordered duplicate union completes

r[chaoscontrol.async_flow.outcomes.complete]
- GIVEN all admitted logical items and exact closure are observed under a reorder and duplicate schedule
- WHEN the independent oracle compares the terminal result
- THEN matching canonical union MAY be classified as complete convergence.

#### Scenario: One required item or closure is missing

r[chaoscontrol.async_flow.outcomes.incomplete]
- GIVEN one required item, sequence, closure marker, final drain, or cleanup observation is missing
- WHEN outcome classification runs
- THEN the result MUST be incomplete or indeterminate
- AND it MUST NOT be complete convergence.

### Requirement: Protocol observations fail closed

r[chaoscontrol.async_flow.observations] Required observations MUST bind participant generation, source sequence, logical boundary, edge, operator, item, result, closure, window, attempt, outcome, loss, final drain, and cleanup through the selected protocol-observation cohort.

#### Scenario: Observation cohort is complete

r[chaoscontrol.async_flow.observations.valid]
- GIVEN every required producer sequence and terminal accounting record is present and valid
- WHEN cohort assembly runs
- THEN ChaosControl MAY pass the observations to the selected oracle.

#### Scenario: Observation sequence has a gap

r[chaoscontrol.async_flow.observations.gap]
- GIVEN one required sequence has a gap, overflow, truncation, conflict, unknown record, failed final drain, or failed cleanup
- WHEN cohort assembly runs
- THEN the cohort MUST remain incomplete or invalid.

### Requirement: Assertions cover semantic violations

r[chaoscontrol.async_flow.assertions] The campaign MUST register stable assertions for wrong item, edge, operator, duplicate application, false order, forged closure, missing closure, early protected effect, hidden retry, stale law, erased assumption, and replay dispatch.

#### Scenario: Protected effect starts early

r[chaoscontrol.async_flow.assertions.early_effect]
- GIVEN a partial result lacks full closure, window closure, and exact prefix-safety evidence
- WHEN a protected effect attempt is observed
- THEN the stable early-effect assertion MUST fail.

#### Scenario: Duplicate input is applied twice to set union state

r[chaoscontrol.async_flow.assertions.duplicate]
- GIVEN the same logical item observation appears more than once
- WHEN runtime state applies it as distinct semantic input contrary to the admitted profile
- THEN the duplicate-application assertion MUST fail.

### Requirement: Fault evidence separates stages

r[chaoscontrol.async_flow.faults] Every selected duplication, reordering, delay, loss, partition, heal, termination, or restart fault MUST retain selected, applicable, rejected, applied, application-failed, observed, healed, and indeterminate facts when relevant.

#### Scenario: Selected fault reaches the data path

r[chaoscontrol.async_flow.faults.observed]
- GIVEN a valid selected fault is applicable, applied, and consumed by the selected flow data path
- WHEN fault evidence is assembled
- THEN the receipt MUST record each established stage separately.

#### Scenario: Selected fault is never observed

r[chaoscontrol.async_flow.faults.unobserved]
- GIVEN a fault is selected or armed but no required data path observes it
- WHEN outcome evidence is assembled
- THEN ChaosControl MUST NOT claim semantic fault impact.

### Requirement: Nondeterminism assumptions stay assumptions

r[chaoscontrol.async_flow.assumptions] ChaosControl MUST evaluate each selected Choregraph nondeterminism assumption against its exact expected observational relation, cohort, schedule family, and bounds. A passing campaign MUST NOT promote the assumption to a theorem.

#### Scenario: Assumption relation holds in selected runs

r[chaoscontrol.async_flow.assumptions.valid]
- GIVEN a current assumption and complete selected campaign observations
- WHEN the assumption oracle runs
- THEN it MAY report bounded agreement for the exact run cohort
- AND it MUST retain the assumption classification.

#### Scenario: Assumption identity is missing

r[chaoscontrol.async_flow.assumptions.missing]
- GIVEN a flow requires an assumption absent from the campaign profile or runtime observations
- WHEN campaign admission or evaluation runs
- THEN ChaosControl MUST reject or classify the result as incomplete.

### Requirement: Cheap and KVM rails stay distinct

r[chaoscontrol.async_flow.kvm] ChaosControl MUST provide a cheap pure and in-process rail plus a separate selected KVM rail. Missing KVM, guest, kernel, initrd, or artifact prerequisites MUST produce unsupported, not pass.

#### Scenario: Cheap rail passes

r[chaoscontrol.async_flow.kvm.cheap]
- GIVEN all pure profiles, adapters, oracles, classifiers, assertions, faults, and fixtures pass
- WHEN the cheap rail completes
- THEN it MAY report bounded logic and simulation evidence
- AND it MUST NOT claim KVM execution.

#### Scenario: KVM is unavailable

r[chaoscontrol.async_flow.kvm.unavailable]
- GIVEN the selected host cannot provide the required KVM cohort
- WHEN the KVM rail starts
- THEN the result MUST be unsupported
- AND it MUST NOT inherit the cheap-rail pass.

### Requirement: Snapshot replay binds one exact outcome

r[chaoscontrol.async_flow.replay] The KVM rail MUST retain and validate a parent snapshot for at least one selected flow outcome. Replay MUST reject wrong artifacts, schedules, parents, observations, or dispatch during replay.

#### Scenario: Exact outcome reproduces

r[chaoscontrol.async_flow.replay.valid]
- GIVEN one selected outcome has an admitted parent snapshot and exact replay inputs
- WHEN snapshot-backed replay runs
- THEN it MAY report reproduction of that exact bounded outcome.

#### Scenario: Replay dispatches a flow item or effect

r[chaoscontrol.async_flow.replay.dispatch]
- GIVEN replay starts transport, retries, flow dispatch, or a protected effect
- WHEN replay validation runs
- THEN the replay result MUST fail.

### Requirement: Campaign evidence is complete and bounded

r[chaoscontrol.async_flow.evidence] ChaosControl MUST bind exact cohorts, profile, matrix, oracles, faults, observations, assertions, assumptions, outcomes, snapshots, replay, bounds, blockers, and non-claims through domain-separated BLAKE3 identities.

#### Scenario: Complete receipt is emitted

r[chaoscontrol.async_flow.evidence.valid]
- GIVEN the selected campaign finishes with complete required artifacts and observations
- WHEN evidence assembly runs
- THEN it MUST emit one deterministic bounded receipt with the exact terminal classification.

#### Scenario: Evidence hides an incomplete input

r[chaoscontrol.async_flow.evidence.hidden_gap]
- GIVEN a required item, observation, fault stage, closure, drain, cleanup, or replay fact is absent
- WHEN evidence assembly runs
- THEN the receipt MUST expose the blocker
- AND it MUST NOT report complete convergence.

### Requirement: Campaign authority remains bounded

r[chaoscontrol.async_flow.boundary] Passing campaign evidence MUST NOT claim compiler correctness, proof-system soundness, runtime correctness, all-schedule convergence, transport delivery, exactly-once effects, universal determinism, physical network behavior, production readiness, or release eligibility.

#### Scenario: One replay is presented as universal convergence

r[chaoscontrol.async_flow.boundary.overclaim]
- GIVEN one exact snapshot-backed outcome reproduces
- WHEN evidence is presented as proof for all schedules or deployments
- THEN ChaosControl MUST reject the claim as outside campaign scope.

### Requirement: Flow campaign validation is maintained

r[chaoscontrol.async_flow.validation] Maintained validation MUST cover positive and negative profiles, cohorts, matrices, oracles, outcomes, observations, assertions, faults, assumptions, KVM, replay, evidence, architecture, formatting, Clippy, Octet, Cairn, and Nix cases.

#### Scenario: False completion becomes accepted

r[chaoscontrol.async_flow.validation.regression]
- GIVEN a change turns incomplete observation, a self-oracle, an early effect, stale evidence, or unsupported KVM into pass
- WHEN maintained validation runs
- THEN the validation rail MUST fail before sync or archive.
