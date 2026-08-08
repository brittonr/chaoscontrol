## ADDED Requirements

### Requirement: SMR workload profiles are typed and bounded
r[chaoscontrol.smr_chain.profile] ChaosControl MUST define a versioned SMR workload profile. The profile MUST bind initial-state identity, observation mode, and command, client, concurrency, virtual progress, fault, trace, replay, and evidence bounds. Missing or unbounded behavior inputs MUST deny evidence-bearing execution.

#### Scenario: Complete profile is admitted
- GIVEN a profile with canonical identity and finite values for every required bound
- WHEN profile admission runs
- THEN it produces a deterministic workload plan and profile ref.

#### Scenario: Unbounded profile is rejected
- GIVEN a profile omits a workload, virtual progress, trace, or replay bound
- WHEN profile admission runs
- THEN it rejects the profile before guest or simulator effects occur.

### Requirement: Chain transitions use canonical BLAKE3 framing
r[chaoscontrol.smr_chain.transition] ChaosControl MUST compute genesis and transition digests with separate versioned BLAKE3 domains. Genesis MUST bind the profile and canonical initial-state refs. Each transition MUST bind the profile ref, command index, prior digest, command length, and command bytes through canonical framing.

#### Scenario: Equivalent transitions match
- GIVEN two adapters provide the same profile ref, command index, prior digest, and command bytes
- WHEN the pure transition function runs
- THEN both transitions produce the same digest.

#### Scenario: Framing or position differs
- GIVEN two transitions differ in command boundary, command index, profile ref, prior digest, or bytes
- WHEN the pure transition function runs
- THEN their canonical transition inputs differ and validation cannot treat them as the same transition.

### Requirement: Replica histories are normalized and validated
r[chaoscontrol.smr_chain.history] ChaosControl MUST normalize replica observations by profile, replica, and command index. It MUST reject malformed digests, impossible indices, changed prior observations, invalid chain links, duplicate conflicts, and rollback. It MUST classify missing observations through the declared lossless or sampled mode.

#### Scenario: Lagging history remains valid
- GIVEN one replica has a valid shorter prefix of another replica's chain
- WHEN history validation runs
- THEN it reports lag without reporting divergence.

#### Scenario: Sampled history has a gap
- GIVEN a sampled observer omits an intermediate index and later reports another valid observation
- WHEN history validation runs
- THEN it reports reduced coverage without fabricating replica divergence.

#### Scenario: Replica changes an observed index
- GIVEN one replica reports different digests for the same profile and command index
- WHEN history validation runs
- THEN it reports a safety violation at that index.

### Requirement: Safety is evaluated continuously
r[chaoscontrol.smr_chain.safety] ChaosControl MUST evaluate accepted observation prefixes continuously. Different digests or canonical application-state refs at the same command index MUST fail safety regardless of later convergence or campaign completion.

#### Scenario: Replicas diverge and later match
- GIVEN two replicas report different digests at one index and later report the same higher digest
- WHEN safety evaluation processes the history
- THEN the earlier divergence remains a failure.

#### Scenario: Replicas share every observed index
- GIVEN all overlapping replica observations have equal digests and valid links
- WHEN safety evaluation processes the history
- THEN it reports no chain-divergence violation.

### Requirement: Liveness uses explicit stabilization conditions
r[chaoscontrol.smr_chain.liveness] ChaosControl MUST evaluate progress only under a named liveness profile. The profile MUST bind quorum availability, lifecycle readiness, disruptive-fault state, virtual progress horizon, and required progress.

#### Scenario: Recovered quorum makes progress
- GIVEN the profile declares a recovered quorum and disruptive faults are inactive
- WHEN committed command count advances within the virtual progress horizon
- THEN the liveness condition passes for that bounded observation.

#### Scenario: Partition remains active
- GIVEN the active fault profile permits loss of quorum
- WHEN committed command count does not advance
- THEN ChaosControl does not relabel the expected unavailability as a safety failure or unconditional liveness failure.

### Requirement: Indefinite proposal outcomes remain unknown
r[chaoscontrol.smr_chain.indefinite_outcomes] ChaosControl MUST classify proposal outcomes as acknowledged, definitely rejected, or indefinite. Retries of one logical operation MUST preserve its stable operation identity.

#### Scenario: Timeout follows a commit
- GIVEN a proposal commits but its acknowledgement is lost
- WHEN the client records an indefinite outcome and later observes the committed chain
- THEN the workload accepts that the operation can appear in history.

#### Scenario: Retry changes logical identity
- GIVEN a client retries one logical operation with a different operation identity
- WHEN workload validation runs
- THEN it rejects the retry trace as an invalid idempotency input.

### Requirement: Consumer adapters expose semantic observations
r[chaoscontrol.smr_chain.adapter] A consumer adapter MUST expose bounded proposals, committed-transition observations, canonical application-state refs, lifecycle facts, observation completeness, and terminal status. It MUST NOT require consensus-internal protocol state for chain validation.

#### Scenario: Consumer uses an admitted SMR implementation
- GIVEN a Rust consumer maps committed application transitions to the adapter contract
- WHEN the workload runs
- THEN ChaosControl can evaluate chain safety without reading election, term, quorum, or protocol-message internals.

#### Scenario: Adapter fabricates expected state
- GIVEN an adapter emits expected digests without invoking the consumer's committed application path
- WHEN observer-path conformance runs
- THEN the adapter fails conformance and cannot produce accepted workload evidence.

### Requirement: Fault campaigns preserve control and effect evidence
r[chaoscontrol.smr_chain.fault_campaign] Every evidence profile MUST include a no-fault control. Fault campaigns MUST bind finite classes, weights, subsets, concurrency, activation, terminal rules, and expected observability. Effect claims MUST require applied and observed fault outcomes.

#### Scenario: Seeded swarm profile runs
- GIVEN a bounded profile declares optional features and fault classes
- WHEN seeded swarm selection runs
- THEN the receipt records the selected subset, weights, choices, and unexplored coverage.

#### Scenario: Selected fault never applies
- GIVEN the schedule selects a fault that is rejected, unsupported, or never observed
- WHEN workload evidence is classified
- THEN it does not claim that the fault affected the consumer.

### Requirement: Replay preserves semantic workload results
r[chaoscontrol.smr_chain.replay] ChaosControl MUST compare replayed operation identities, proposal outcomes, observations, safety prefixes, liveness preconditions, and terminal verdicts. The first mismatch MUST stop semantic replay acceptance.

#### Scenario: Replay matches
- GIVEN a retained workload artifact has valid parent state and complete semantic records
- WHEN standalone replay runs
- THEN the replayed semantic history and verdict match the retained result.

#### Scenario: Replay changes one observation
- GIVEN replay emits a different digest or command index at one observation
- WHEN semantic comparison runs
- THEN it reports the first mismatch and denies replay acceptance.

### Requirement: Evidence remains bounded and claim-scoped
r[chaoscontrol.smr_chain.evidence] ChaosControl MUST emit typed evidence that binds profile, build, adapter, observer, observation mode, dropped-event accounting, seed, choices, fault outcomes, observations, bounds, verdicts, replay, and non-claims. A pass MUST NOT imply universal SMR, consensus, durability, linearizability, Byzantine tolerance, security, or release correctness.

#### Scenario: Complete bounded receipt is accepted
- GIVEN a run has valid identity refs, finite bounds, observation summaries, verdicts, and matching replay evidence
- WHEN evidence validation runs
- THEN it accepts the declared bounded workload evidence class.

#### Scenario: Consumer promotes evidence scope
- GIVEN a consumer presents the receipt as proof of universal consensus correctness or production readiness
- WHEN evidence-role validation runs
- THEN it rejects the promoted claim.

### Requirement: Workload logic has a pure core
r[chaoscontrol.smr_chain.boundary] Profile admission, chain transitions, history validation, safety evaluation, liveness evaluation, indefinite-outcome handling, replay comparison, and verdict classification MUST be pure deterministic logic. Guest, VM, process, file, clock, network, persistence, and output effects MUST remain in shells.

#### Scenario: Fixture runs without infrastructure
- GIVEN a complete in-memory profile, command sequence, proposal outcomes, and replica observations
- WHEN semantic validation runs
- THEN it returns the same verdict without KVM, filesystem, environment, process, wall-clock, or network access.

### Requirement: SMR workload validation covers success and failure
r[chaoscontrol.smr_chain.validation] The change MUST include positive and negative tests for profiles, framing, histories, adapters, proposal outcomes, faults, liveness, replay, evidence, and claim boundaries.

#### Scenario: Validation corpus runs
- GIVEN compliant and deliberately faulty fixtures across pure, adapter, campaign, evidence, and KVM layers
- WHEN the selected validation rail runs
- THEN valid fixtures pass and each negative fixture fails with its expected stable class.
