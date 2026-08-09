# Deterministic Runtime Capacity Specification Delta

## ADDED Requirements

### Requirement: Runtime capacity uses an explicit checked plan

r[chaoscontrol.runtime_capacity.plan]

ChaosControl MUST compute a checked capacity plan before VM or controller activation for each selected preallocated runtime resource.

#### Scenario: A valid plan is admitted

- GIVEN explicit limits fit every compiled hard cap and checked arithmetic succeeds
- WHEN runtime-capacity admission runs
- THEN the plan MUST identify every selected slot and byte capacity
- AND allocation MUST occur before guest progress can start

#### Scenario: A plan is invalid

- GIVEN a limit is zero, above its hard cap, contradictory, or unrepresentable
- WHEN runtime-capacity admission runs
- THEN activation MUST fail before allocation-dependent runtime work or guest progress

### Requirement: Schedule reservation performs no allocation

r[chaoscontrol.runtime_capacity.journal]

An initialized schedule journal MUST own capacity for its admitted record limit, and transition reservation MUST NOT request more memory.

#### Scenario: A transition reserves within the limit

- GIVEN journal initialization allocated the admitted record capacity
- WHEN a transition reserves and commits one record
- THEN reservation and commit MUST use existing capacity
- AND the resulting trace MUST preserve current ordering and identity rules

#### Scenario: The record limit is exhausted

- GIVEN the journal already contains its admitted record count
- WHEN another transition requests reservation
- THEN reservation MUST fail before guest progress
- AND the journal state MUST remain unchanged

### Requirement: Virtio scratch buffers use bounded leases

r[chaoscontrol.runtime_capacity.virtio_pool]

Selected virtio scratch buffers MUST come from startup-allocated size classes with explicit slot limits and generation-bound leases.

#### Scenario: A buffer lease completes

- GIVEN a matching free slot exists
- WHEN a request acquires, uses, and returns the lease
- THEN the exposed bytes MUST be zeroed before use
- AND the slot MUST become available exactly once after return

#### Scenario: A lease is invalid

- GIVEN a request is oversized or a lease is stale, duplicated, or from another generation
- WHEN pool validation runs
- THEN the operation MUST fail without exposing or freeing another slot

### Requirement: Network retention uses bounded packet slots

r[chaoscontrol.runtime_capacity.network_pool]

Selected retained network packets MUST use preallocated packet slots and queue metadata under packet-count and byte-count limits.

#### Scenario: A packet enters the queue

- GIVEN a packet fits all byte, packet, and free-slot limits
- WHEN enqueue commits
- THEN the packet MUST retain FIFO order
- AND counters and slot ownership MUST change atomically

#### Scenario: Packet capacity is unavailable

- GIVEN a byte, packet, or free-slot limit is exhausted
- WHEN enqueue is attempted
- THEN enqueue MUST fail before queue counters or packet ownership change

### Requirement: Capacity observations are bounded

r[chaoscontrol.runtime_capacity.observation]

Capacity evidence MUST report only the selected plan, startup result, usage, exhaustion, release, and leak observations.

#### Scenario: A capacity report is reviewed

- GIVEN a completed or failed run emitted capacity observations
- WHEN evidence validation runs
- THEN identities and counters MUST match the selected plan and observed transitions

### Requirement: Capacity claims remain narrow

r[chaoscontrol.runtime_capacity.boundary]

Capacity evidence MUST NOT claim deterministic latency, global zero allocation, zero-copy I/O, or host memory guarantees.

#### Scenario: A capacity report overclaims

- GIVEN a report promotes selected capacity observations into a broader performance or memory claim
- WHEN claim-boundary validation runs
- THEN validation MUST fail with an overclaim diagnostic

### Requirement: Verification covers successful and failed capacity paths

r[chaoscontrol.runtime_capacity.verification]

Implementation MUST pair positive capacity tests with negative allocation, exhaustion, ownership, arithmetic, and cleanup tests.

#### Scenario: The focused matrix runs

- GIVEN valid and invalid capacity fixtures and an allocation-attempt probe
- WHEN focused verification runs
- THEN selected steady-state operations MUST use initialized capacity
- AND every declared invalid path MUST fail with its expected typed outcome
