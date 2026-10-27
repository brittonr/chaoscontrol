# Assertion Hit Checkpoints Specification

## Purpose

Preserve assertion-hit states as bounded, replayable exploration checkpoints so rare workload states can be branched from without rediscovering them by chance.

## ADDED Requirements

### Requirement: Assertion hits are observed at an exact execution boundary

r[chaoscontrol.assertion_checkpoints.observation] The VMM MUST expose a bounded assertion-hit observation for an accepted catalog-bound assertion at the deterministic boundary after its hypercall or port-I/O exit has been fully handled and before the guest executes another instruction. The observation MUST carry the exact assertion fingerprint and catalog token, kind, verdict, guest/process owner, virtual tick, VM and vCPU, and the absence of intervening guest progress. Unbound, conflicting, malformed, or out-of-catalog assertions MUST NOT produce a capture candidate.

#### Scenario: A rare hit is observable

- GIVEN an admitted structured assertion catalog and a running guest that evaluates a `sometimes` assertion to true
- WHEN the assertion exit is handled
- THEN the VMM MUST expose one hit observation with the exact identity, tick, owner, and boundary facts
- AND no guest instruction MUST have executed between the assertion effect and the observation.

#### Scenario: An unbound assertion is evaluated

- GIVEN a guest emits an assertion whose identity is absent from the admitted catalog
- WHEN the exit is handled
- THEN no checkpoint candidate MUST be produced for that event
- AND the event MUST retain its existing catalog-failure classification.

### Requirement: Capture selection is pure, seeded, and bounded

r[chaoscontrol.assertion_checkpoints.policy] The explorer MUST select capture candidates with a pure deterministic policy over admissible hit facts, prior hit counts, frontier state, the campaign seed, and named limits. The policy MUST define per-run and per-campaign capture limits, deterministic ordering, novelty and rarity inputs, duplicate suppression, and a stable reason for every skip. It MUST NOT read files, clocks, environment, processes, or network state.

#### Scenario: The same hit facts produce the same decision

- GIVEN the same seed, limits, prior hit counts, frontier state, and ordered hit observations
- WHEN selection runs twice
- THEN it MUST return the same captures, skips, scores, and reasons in the same order.

#### Scenario: The campaign capture budget is exhausted

- GIVEN the per-campaign capture limit has been reached
- WHEN another admissible hit arrives
- THEN the policy MUST skip it with the exhausted-budget reason
- AND it MUST NOT capture a snapshot or mutate the frontier.

### Requirement: Captured checkpoints bind exact state and provenance

r[chaoscontrol.assertion_checkpoints.capture] A selected hit MUST be captured at its observation boundary and bound to the exact assertion identity, catalog token, tick, run identity, seed, fault schedule, topology, and guest artifact identities. The snapshot reference MUST retain the existing snapshot store, codec, and digest meaning. Capture MUST NOT reconstruct an earlier state or use a later checkpoint as a substitute.

#### Scenario: A selected hit is captured

- GIVEN the policy selects one hit for capture
- WHEN the shell captures the boundary state
- THEN the retained reference MUST bind the exact hit and execution facts required to restore it.

#### Scenario: Snapshot storage fails

- GIVEN a selected hit cannot be encoded, stored, or validated within the declared bounds
- WHEN capture finishes
- THEN the checkpoint MUST be recorded as not captured with its typed reason
- AND no partial or unverified reference MUST enter the frontier.

### Requirement: Checkpoints enter the frontier with validated provenance

r[chaoscontrol.assertion_checkpoints.frontier] A captured checkpoint MUST become a frontier proposal with its assertion provenance, exact parent context, score inputs, and capture reference. The frontier MUST deduplicate equivalent hits, reject stale or mismatched parents, and apply the same restore and replay validation as other parent sources. Restoring a checkpoint MUST reject identity, catalog, tick, schedule, topology, or artifact drift.

#### Scenario: A checkpoint is restored for a branch

- GIVEN a captured checkpoint and matching frontier proposal
- WHEN the explorer restores it for expansion
- THEN the restored state MUST match the retained assertion context and execution facts
- AND a branch from that parent MUST carry the checkpoint provenance.

#### Scenario: A checkpoint is stale

- GIVEN a checkpoint reference whose catalog, tick, schedule, or guest artifact identity no longer matches the campaign facts
- WHEN the frontier validates it
- THEN the checkpoint MUST be rejected before restore or expansion
- AND no branch MUST be published from it.

### Requirement: Checkpoint evidence is bounded and explicit

r[chaoscontrol.assertion_checkpoints.evidence] The explorer MUST emit a bounded receipt binding the selection policy identity, limits, seed, ordered hit observations, captures, skips, duplicate decisions, failed captures, frontier insertions, and referenced snapshot identities. The receipt MUST exclude raw guest memory, raw logs, and any payload beyond declared bounds.

#### Scenario: Evidence is written for a campaign

- GIVEN a campaign produced selected, skipped, and failed checkpoint outcomes
- WHEN the checkpoint receipt is written
- THEN it MUST account for each outcome without raw memory or transcript dumps
- AND its stored size MUST remain within the declared bound.

### Requirement: Checkpoints do not change assertion or finding meaning

r[chaoscontrol.assertion_checkpoints.nonclaims] A captured or skipped checkpoint MUST NOT change an assertion verdict, create a bug from a `sometimes` or `reachable` assertion, promote a liveness gap, or by itself promote replay, correctness, coverage-completeness, causality, or release evidence. A checkpoint MUST describe only that the named state was reached and retained.

#### Scenario: A hit is captured but no branch finds a bug

- GIVEN one captured checkpoint and a completed campaign with no safety failure
- WHEN the campaign report is produced
- THEN the checkpoint MUST remain an exploration fact only
- AND it MUST NOT be reported as a bug, liveness failure, or correctness proof.

#### Scenario: A report overclaims amplification

- GIVEN a report claims that captured checkpoints prove complete rare-state coverage
- WHEN non-claim validation runs
- THEN the report MUST fail validation.

### Requirement: The checkpoint rail has positive and negative fixtures

r[chaoscontrol.assertion_checkpoints.fixtures] The rail MUST include positive fixtures for a rare hit, duplicate suppression, deterministic rerun, and successful branch amplification, plus negative fixtures for an unbound assertion, identity drift, stale parent, exhausted capture budget, storage failure, and overclaim. Focused KVM acceptance MUST demonstrate the post-hit boundary and a branch restored from it.

#### Scenario: The rail is proposed for product use

- GIVEN observation, policy, capture, frontier, evidence, and fixture work is complete
- WHEN focused Cargo, octet, Cairn, and Nix checks run
- THEN every positive fixture MUST pass
- AND every negative fixture MUST fail at its declared boundary.
