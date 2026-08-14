# Role Protocol Fault Campaign Specification Delta

## ADDED Requirements

### Requirement: Campaigns bind immutable protocol cohorts

r[chaoscontrol.role_protocol.cohorts] ChaosControl MUST consume exact immutable Choregraph protocol and Lattice runtime cohorts through versioned narrow adapters. The cohorts MUST bind source revisions, schemas, artifacts, fixtures, adapter identities, and BLAKE3 source manifests.

#### Scenario: Producer and runtime cohorts match

r[chaoscontrol.role_protocol.cohorts.valid]
- GIVEN the selected Choregraph and Lattice revisions, schemas, artifacts, and adapters match the campaign profile
- WHEN cohort admission runs
- THEN the mapped protocol and runtime values MUST preserve their exact source identities.

#### Scenario: Runtime envelope schema drifts

r[chaoscontrol.role_protocol.cohorts.stale]
- GIVEN the Lattice envelope schema or identity domain differs from the frozen adapter
- WHEN cohort admission runs
- THEN the campaign MUST fail before simulation or VM activation.

### Requirement: Campaign profiles are typed and bounded

r[chaoscontrol.role_protocol.profile] ChaosControl MUST define a typed Nickel campaign profile for cohorts, protocol artifacts, role placements, cases, expected outcomes, faults, assertions, observations, named bounds, evidence scope, and non-claims. Runtime events and traces MUST remain Rust-owned.

#### Scenario: Complete protocol campaign is admitted

r[chaoscontrol.role_protocol.profile.valid]
- GIVEN a profile names exact cohorts, artifacts, roles, cases, faults, assertions, observations, finite bounds, and non-claims
- WHEN profile validation runs
- THEN it MUST produce one deterministic runtime input bound to the source profile.

#### Scenario: Campaign omits an observation bound

r[chaoscontrol.role_protocol.profile.unbounded]
- GIVEN a profile omits any required message, event, queue, time, artifact, snapshot, or replay bound
- WHEN profile admission runs
- THEN admission MUST fail before workload activation.

### Requirement: Expected outcomes are independently reviewed

r[chaoscontrol.role_protocol.oracle] Every selected case MUST bind an independently reviewed expected-outcome fixture. Campaign success MUST NOT use the Lattice runtime under test as its only oracle.

#### Scenario: Independent expectation matches runtime observation

r[chaoscontrol.role_protocol.oracle.valid]
- GIVEN a frozen fixture names initial state, selected action or fault, allowed terminal classes, forbidden changes, and required assertions
- WHEN runtime observations match that expectation
- THEN the case MAY satisfy its bounded comparison
- AND the receipt MUST bind the expectation separately from runtime artifacts.

#### Scenario: Runtime generates its own expected result

r[chaoscontrol.role_protocol.oracle.tautological]
- GIVEN a campaign derives its only expected outcome by calling the same Lattice path under test
- WHEN oracle admission runs
- THEN the case MUST be rejected as tautological.

### Requirement: The matrix covers protocol and recovery cases

r[chaoscontrol.role_protocol.matrix] Pure case expansion MUST preserve exact fault-free transfer, fault-free choice, blocked, unknown, terminal failure, heal, and replay cases without allowing one case class to silently satisfy another.

#### Scenario: Unknown dispatch is classified as completion

r[chaoscontrol.role_protocol.matrix.unknown]
- GIVEN a case requires an explicit unknown outcome after interrupted dispatch
- WHEN the observed runtime record remains uncertain
- THEN a completion case MUST NOT pass.

### Requirement: Protocol assertions detect forbidden advancement

r[chaoscontrol.role_protocol.assertions] The campaign MUST evaluate stable assertions for wrong-session, wrong-role, wrong-step, duplicate-commit, reordered-skip, stale-label, former-owner, replay-dispatch, unknown-outcome, and terminal-mutation violations.

#### Scenario: Duplicate envelope advances twice

r[chaoscontrol.role_protocol.assertions.duplicate]
- GIVEN one message identity already committed its expected transition
- WHEN a duplicate envelope reaches the same session
- THEN the duplicate-commit assertion MUST fail if any role cursor or ownership state advances again.

#### Scenario: Stale branch label is rejected

r[chaoscontrol.role_protocol.assertions.label]
- GIVEN a role cursor exposes an offer without the supplied stale label
- WHEN that label reaches the role
- THEN the stale-label assertion MUST require no branch or cursor advancement.

### Requirement: Fault stages remain explicit

r[chaoscontrol.role_protocol.faults] Campaigns MUST preserve selected, applicable, applied, observed, healed, failed, and indeterminate stages for exact loss, delay, duplication, reordering, corruption, partition, bandwidth, role-termination, restart, and heal faults supported by the selected fabric.

#### Scenario: Partition blocks a transfer and later heals

r[chaoscontrol.role_protocol.faults.partition]
- GIVEN one admitted partition targets the transfer path and a later heal restores it
- WHEN the campaign runs
- THEN the faulted phase MUST remain a transport or blocked outcome
- AND later completion MAY be reported only through the selected recovery rules.

#### Scenario: Fault targets another step

r[chaoscontrol.role_protocol.faults.wrong_step]
- GIVEN a selected fault targets a different session step, link, direction, or tick range
- WHEN the current case runs
- THEN the campaign MUST NOT report that fault as observed for the current action.

### Requirement: Outcome classes separate safety and observation

r[chaoscontrol.role_protocol.outcomes] Pure classification MUST distinguish expected completion, expected block, explicit unknown outcome, expected terminal failure, assertion violation, protocol mismatch, transport outcome, guest or runtime failure, partial observation, unsupported, and indeterminate.

#### Scenario: Packet is absent without complete state evidence

r[chaoscontrol.role_protocol.outcomes.absent]
- GIVEN no message delivery is observed and required protocol-state or loss accounting is incomplete
- WHEN outcome classification runs
- THEN the case MUST be partial or indeterminate
- AND it MUST NOT report a safe expected block or completion.

### Requirement: Observation accounting detects incomplete evidence

r[chaoscontrol.role_protocol.observation] Evidence-eligible campaigns MUST bind producer generations, source-local sequences, event classes, bounds, loss counters, terminal accounting, final drain, detach, and cleanup. Required gaps, loss, overflow, truncation, malformed events, unknown events, parse failure, missing terminal accounting, or failed cleanup MUST prevent complete classification.

#### Scenario: Required producer loses one event

r[chaoscontrol.role_protocol.observation.loss]
- GIVEN a required producer reports a sequence gap or loss
- WHEN accounting and outcome classification run
- THEN observation MUST be partial, failed, or unsupported
- AND complete protocol evidence MUST not pass.

### Requirement: Replay reproduces one exact bounded outcome

r[chaoscontrol.role_protocol.replay] Snapshot-backed replay MAY establish reproduction only for the exact protocol cohort, campaign, schedule, guest, snapshot, assertion catalog, and observation scope. Replay MUST NOT dispatch fresh protocol effects.

#### Scenario: Replay reproduces a duplicate rejection

r[chaoscontrol.role_protocol.replay.valid]
- GIVEN an exact retained snapshot and schedule reproduce one duplicate-message rejection
- WHEN replay validation runs
- THEN the receipt MAY report reproduction for that bounded outcome
- AND no fresh transport effect MUST occur.

### Requirement: Campaign evidence is canonical and narrow

r[chaoscontrol.role_protocol.evidence] ChaosControl MUST emit domain-separated BLAKE3 identities for cohorts, profile, oracle, matrix, run, observations, assertions, snapshots, replay, and receipt. The receipt MUST bind terminal class, blockers, and non-claims.

#### Scenario: One runtime cohort identity changes

r[chaoscontrol.role_protocol.evidence.stale]
- GIVEN a Lattice source, schema, adapter, or fixture identity changes
- WHEN evidence validation runs
- THEN the prior campaign receipt MUST be stale.

### Requirement: Ownership boundaries remain explicit

r[chaoscontrol.role_protocol.boundary] ChaosControl MUST retain simulation, fault, assertion, replay, observation, and campaign evidence meaning. Trellis and Choregraph MUST retain protocol semantics and projection. Lattice MUST retain runtime, persistence, adapter, recovery, and authority meaning.

#### Scenario: KVM campaign is promoted to universal correctness

r[chaoscontrol.role_protocol.boundary.overclaim]
- GIVEN one exact KVM campaign and replay pass
- WHEN evidence is presented as universal deadlock freedom, exactly-once delivery, physical-network correctness, production availability, or release eligibility
- THEN the promotion MUST be rejected.

### Requirement: Protocol campaigns have maintained validation

r[chaoscontrol.role_protocol.validation] Maintained checks MUST pair positive transfer, choice, block, unknown, heal, observation, and replay cases with negative stale, tautological, mismatched, duplicated, reordered, leaking, unsupported, incomplete, cleanup, missing-prerequisite, and overclaim cases. Cheap checks MUST remain separate from KVM behavior evidence.

#### Scenario: Closeout checks run

r[chaoscontrol.role_protocol.validation.closeout]
- GIVEN maintainers intend to sync and archive the change
- WHEN pure, Nickel, adapter, oracle, assertion, simulator, network, fault, replay, Cairn, KVM, and relevant Nix checks run
- THEN required positive and negative cases MUST produce their expected results
- AND dry-run, compile-only, or missing-KVM evidence MUST NOT satisfy behavior completion.
