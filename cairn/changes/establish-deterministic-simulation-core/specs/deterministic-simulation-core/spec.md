# Deterministic Simulation Core Specification

## Purpose

Defines product-neutral deterministic time, entropy, scheduling, event, choice, and snapshot mechanisms for ChaosControl and Aspen adapters.

## Requirements

### Requirement: A shared seam precedes repository creation

r[shared.deterministic_sim.aspen_boundary] ChaosControl and Aspen MUST compare their clock, entropy, scheduler, event, choice, snapshot, error, and policy semantics before publication. The shared repository MUST contain only a stable product-neutral subset that both consumers can wrap.

#### Scenario: Consumer semantics conflict

- GIVEN the comparison finds incompatible ordering or snapshot requirements with no neutral contract
- WHEN the establishment decision runs
- THEN repository creation MUST stop with the exact conflicting requirements
- AND neither product implementation MAY be renamed as the shared standard.

### Requirement: Deterministic simulation has a shared repository

r[shared.deterministic_sim.repository] After seam acceptance, the project MUST publish a `deterministic-sim` repository under `AGPL-3.0-or-later`. Consumers MUST pin immutable reviewed revisions without sibling path fallbacks.

#### Scenario: A consumer adopts the core

- GIVEN the shared repository passes package and behavior checks
- WHEN ChaosControl or Aspen adds the dependency
- THEN the consumer MUST pin one immutable reviewed revision
- AND it MUST retain a product-specific adapter for policy and effects.

### Requirement: Virtual time uses checked explicit transitions

r[shared.deterministic_sim.clock] A virtual clock MUST bind an algorithm version, current tick, and named advance policy. Advancing time MUST use checked arithmetic and MUST return a typed failure on invalid deltas or overflow.

#### Scenario: Tick advancement overflows

- GIVEN a valid clock state near its numeric limit
- WHEN the supplied advance cannot be represented
- THEN the transition MUST return an overflow failure
- AND it MUST NOT saturate, wrap, or mutate the prior state.

### Requirement: Entropy streams are versioned and separable

r[shared.deterministic_sim.entropy] Deterministic entropy MUST use versioned ChaCha20 streams bound to explicit seed material, domain, stream label, and byte position. Snapshot and restore MUST reproduce the next bytes exactly.

#### Scenario: Two domains use equal seed material

- GIVEN equal seed material with different domain or stream labels
- WHEN both streams generate bytes
- THEN their stream identities and output sequences MUST differ
- AND each snapshot MUST resume its own sequence exactly.

#### Scenario: Seed material is all zero

- GIVEN explicit zero seed material
- WHEN a stream is created
- THEN zero MUST remain part of the versioned seed input
- AND the implementation MUST NOT substitute an undocumented constant.

### Requirement: Scheduling uses supplied replay-stable facts

r[shared.deterministic_sim.scheduler] The scheduler MUST consume an ordered runnable set, explicit policy, supplied stable progress facts, and deterministic choice state. It MUST NOT read host wall time, signal arrival, thread timing, or ambient process state.

#### Scenario: The runnable set is empty

- GIVEN scheduler policy does not permit completion and no runnable identities exist
- WHEN a decision transition runs
- THEN it MUST return a typed no-runnable failure
- AND it MUST NOT fabricate a task or consume choice state.

#### Scenario: Equal progress facts are replayed

- GIVEN equal scheduler state, runnable identities, progress facts, and choice state
- WHEN the transition runs twice
- THEN both decisions and next states MUST match exactly.

### Requirement: Scheduled events have stable generic order

r[shared.deterministic_sim.events] A scheduled event MUST bind event identity, logical tick, deterministic order key, and payload. Equal ticks MUST use a documented stable tie rule. Event mechanics MUST not assign fault, packet, task, or workload meaning.

#### Scenario: Events share one tick

- GIVEN multiple pending events have the same logical tick
- WHEN the queue selects the next event
- THEN it MUST use the versioned stable order key
- AND insertion timing outside supplied state MUST NOT affect selection.

### Requirement: Recorded choices reject invalid overrides

r[shared.deterministic_sim.choices] A recorded choice MUST bind domain identity, option count, selected index, and override provenance. An override MUST be admitted only when its domain and option count match and its index is in range.

#### Scenario: An override index is out of range

- GIVEN a choice domain has a bounded option count
- WHEN replay supplies an index outside that domain
- THEN choice preflight MUST return a typed override failure
- AND it MUST NOT advance the choice stream.

### Requirement: Snapshots contain all shared replay state

r[shared.deterministic_sim.snapshot] Shared snapshots MUST include algorithm versions, clock state, entropy streams, scheduler state, pending events, recorded choices, counters, and declared limits. Pure preflight MUST reject missing, malformed, or incompatible state before reconstruction.

#### Scenario: A snapshot omits one entropy stream

- GIVEN consumer topology requires a named stream
- WHEN snapshot preflight sees that the stream is absent
- THEN it MUST return an incomplete-state failure
- AND no partial core state MAY be restored.

### Requirement: Product authority remains in adapters

r[shared.deterministic_sim.chaoscontrol_boundary] ChaosControl MUST retain guest progress measurement, KVM control, VMM scheduling policy, fault application, device effects, artifact persistence, and replay evidence. Aspen MUST retain Molten runtime and distributed-system policy.

#### Scenario: A shared scheduler selects a task

- GIVEN a consumer supplies valid runnable and progress facts
- WHEN the core returns a task identity
- THEN the consumer adapter MUST decide and perform the actual execution effect
- AND the core result MUST NOT claim that execution occurred.

### Requirement: Version compatibility is explicit

r[shared.deterministic_sim.compatibility] Algorithm, snapshot, and adapter versions MUST have an explicit compatibility table. Unsupported combinations MUST fail before state mutation or replay claims.

#### Scenario: A snapshot names an unsupported entropy version

- GIVEN a snapshot uses an entropy algorithm version outside the compatibility table
- WHEN admission runs
- THEN it MUST return a typed compatibility failure
- AND no stream state MAY activate.

### Requirement: Migration preserves deterministic observations

r[shared.deterministic_sim.migration] Consumer migration MUST compare entropy bytes, clock ticks, schedule choices, event order, recorded choices, and snapshot continuation before deleting local mechanisms.

#### Scenario: Snapshot continuation differs

- GIVEN a maintained snapshot-resume fixture
- WHEN local and shared adapters produce different next observations
- THEN migration MUST stop
- AND local code MUST remain until an explicit versioned behavior change is accepted.

### Requirement: Checks include hostile states

r[shared.deterministic_sim.validation] Shared and consumer suites MUST include positive repeat and resume cases plus overflow, empty, stale, invalid, incomplete, incompatible, and exhausted-budget cases.

#### Scenario: Full deterministic core checks run

- GIVEN shared unit fixtures and consumer integration fixtures
- WHEN all focused checks run
- THEN equal accepted inputs MUST produce equal transitions and snapshots
- AND invalid inputs MUST fail without panic, hidden ambient input, partial restore, or silent fallback.
