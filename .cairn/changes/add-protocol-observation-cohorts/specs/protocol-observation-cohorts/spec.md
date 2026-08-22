# Protocol Observation Cohorts Specification

## Purpose

Provide bounded canonical protocol-observation transport and cohort assembly without transferring protocol semantics into ChaosControl.

## ADDED Requirements

### Requirement: Protocol-observation profiles are typed and bounded
r[chaoscontrol.protocol_observation.profile] A protocol-observation campaign MUST use a typed Nickel profile. The profile MUST bind protocol, projection schema, producers, participants, logical-boundary schema, oracle adapter, novelty selection, marker policy, record bounds, cohort bounds, oracle-work bounds, and non-claims. Unknown fields, missing identities, incompatible schemas, or unbounded values MUST deny admission.

#### Scenario: Complete profile is admitted
- GIVEN a profile with exact identities, compatible schemas, declared participants, and finite bounds
- WHEN profile admission runs
- THEN it returns a canonical profile ref and bounded observation plan.

#### Scenario: Oracle adapter identity is missing
- GIVEN an evidence-bearing profile without an immutable consumer oracle-adapter identity
- WHEN profile admission runs
- THEN it rejects the profile before guest execution.

### Requirement: Protocol observations use canonical opaque envelopes
r[chaoscontrol.protocol_observation.envelope] Each protocol observation MUST bind the admitted profile, protocol, producer, participant, guest or process, generation, source sequence, transition class, logical-boundary ref, projection schema, projection ref, scheduler position, and completeness facts. Inline projection bytes MUST be bounded and canonical. Record identity MUST use domain-separated BLAKE3. Process-local hashes MUST NOT serve as durable record identity.

#### Scenario: Equivalent observation repeats
- GIVEN identical admitted envelope fields and canonical projection bytes
- WHEN record identity is computed in separate processes
- THEN both records have the same BLAKE3 identity.

#### Scenario: Projection payload exceeds its bound
- GIVEN a record with inline projection bytes beyond the admitted limit
- WHEN envelope admission runs
- THEN it returns the typed bound failure and stores no truncated passing record.

#### Scenario: Source sequence repeats with different content
- GIVEN one producer and generation emit the same source sequence with different projection refs
- WHEN envelope admission runs
- THEN it reports a conflict.

### Requirement: Cohorts use consumer-defined logical boundaries
r[chaoscontrol.protocol_observation.cohort] The pure core MUST assemble records by exact protocol cohort and consumer-defined logical-boundary refs. It MUST validate required participants, generations, source sequences, duplicate rules, loss counters, record bounds, final-drain facts, and projection identities. It MUST return complete, incomplete, conflicting, or unsupported. It MUST NOT infer protocol order from timestamps or cross-producer arrival order.

#### Scenario: Required participants reach one logical boundary
- GIVEN every required participant emits one admitted projection for the same logical-boundary ref with complete sequence accounting
- WHEN cohort assembly runs
- THEN it returns a complete cohort with the exact participant and record refs.

#### Scenario: One required participant is missing
- GIVEN a cohort profile requires a participant that has no admitted record at the logical boundary
- WHEN cohort assembly runs
- THEN it returns incomplete and identifies the missing participant.

#### Scenario: Timestamps imply an order
- GIVEN observations from different producers have host or guest timestamps but no consumer-defined ordering relation
- WHEN cohort assembly runs
- THEN it does not create a semantic total order from those timestamps.

### Requirement: Protocol oracle semantics remain consumer-owned
r[chaoscontrol.protocol_observation.oracle_boundary] ChaosControl MUST pass only admitted cohort facts to a separately identified consumer-owned pure oracle adapter. The adapter MUST return typed results and its exact oracle identity. ChaosControl MUST NOT invent, widen, or reinterpret protocol semantics. The runtime under test MUST NOT be the only source for an expected success result.

#### Scenario: Consumer oracle reports a safety failure
- GIVEN a complete admitted cohort and a consumer oracle that detects conflicting protocol state
- WHEN the oracle adapter runs
- THEN ChaosControl records the typed result, oracle ref, cohort ref, and bounded diagnostic refs without changing the result meaning.

#### Scenario: Runtime self-report is the only oracle
- GIVEN a workload proposes its own pass field without an independent evaluation path
- WHEN oracle-adapter admission runs
- THEN the evidence-bearing campaign rejects that oracle configuration.

#### Scenario: Cohort is incomplete
- GIVEN a consumer oracle requires a complete cohort but cohort assembly returns incomplete
- WHEN orchestration evaluates the campaign
- THEN it records incomplete and does not invoke or promote a passing protocol result.

### Requirement: Protocol novelty identity is stable
r[chaoscontrol.protocol_observation.novelty] The pure core MUST compute a domain-separated BLAKE3 novelty identity from profile-selected canonical projection fields or refs. The explorer MAY map that identity into bounded coverage guidance. Evidence MUST retain the full novelty identity and selection-profile ref.

#### Scenario: Same protocol state repeats
- GIVEN separate branches reach the same selected canonical protocol state
- WHEN novelty identity is computed
- THEN both branches produce the same full novelty identity.

#### Scenario: Coverage slot collides
- GIVEN two full novelty identities map to one compact coverage slot
- WHEN evidence exports
- THEN it retains both full identities and does not treat the slot as proof of state equality.

### Requirement: Protocol observations can bind declared markers and snapshots
r[chaoscontrol.protocol_observation.snapshot_binding] After the declared-event branching contract is admitted, a protocol observation MAY bind a declared marker identity. Marker-linked evidence MUST bind the logical boundary, projection, cohort, and restorable parent snapshot refs. A marker or snapshot MUST NOT make an incomplete cohort complete or claim one cross-participant wall-clock instant.

#### Scenario: Marker-linked protocol state replays
- GIVEN an admitted marker, protocol observation, complete cohort, and restorable parent snapshot
- WHEN snapshot-backed replay runs
- THEN replay validates every linked identity before it accepts the marker context.

#### Scenario: Marker cohort is incomplete
- GIVEN a marker fires before all required participant observations arrive
- WHEN marker evidence is assembled
- THEN it records the incomplete cohort and cannot claim complete protocol-state coverage.

### Requirement: Protocol-observation evidence fails closed
r[chaoscontrol.protocol_observation.evidence] Receipts MUST bind profile, producer, participant, schema, record, cohort, completeness, oracle adapter, oracle result, novelty, marker, snapshot, scheduler, fault, replay, bound, and non-claim refs where applicable. Missing, conflicting, unsupported, stale, overflowed, or unbounded facts MUST remain distinct from pass.

#### Scenario: Complete protocol campaign exports
- GIVEN a bounded campaign has admitted observations, complete cohorts, consumer oracle results, and replay facts
- WHEN evidence exports
- THEN the receipt binds every applicable identity, result, gap, and non-claim.

#### Scenario: Bounded result is promoted to universal proof
- GIVEN one passing protocol-aware campaign receipt
- WHEN a consumer labels it universal protocol correctness, production readiness, or release eligibility
- THEN claim validation rejects the promotion.

### Requirement: Protocol-observation validation is adversarial
r[chaoscontrol.protocol_observation.validation] Validation MUST include positive and negative profile, envelope, cohort, oracle, novelty, marker, snapshot, replay, bound, evidence, and claim-boundary fixtures.

#### Scenario: False runtime oracle fixture runs
- GIVEN a fixture self-reports success while independent projection facts violate its declared property
- WHEN focused validation runs
- THEN the consumer oracle reports the expected failure.

#### Scenario: Closeout validation runs
- GIVEN maintainers intend to publish the protocol-observation contract
- WHEN focused tests, lifecycle gates, and selected KVM or Nix checks run
- THEN every positive and negative fixture produces its expected stable result.
