# Virtio Guest Input Safety Specification

## Purpose

Validate and bound all guest-controlled virtio MMIO queue and descriptor inputs before host arithmetic, allocation, cursor movement, backend mutation, or completion.

## Requirements

### Requirement: Only validated queue configurations become ready

r[chaoscontrol.virtio_safety.queue_configuration] ChaosControl MUST validate the full-width MMIO value, selected queue, nonzero power-of-two size within the offered maximum, required address alignments, checked ring footprints, guest-memory containment, negotiated features, device status, and legal state transition before a queue becomes ready or is processed.

#### Scenario: Compliant queue becomes ready

r[chaoscontrol.virtio_safety.validation.positive]
- GIVEN a driver negotiates supported features and supplies a valid queue size and fully contained aligned ring regions in the required status order
- WHEN it marks the queue ready
- THEN the transport MUST activate exactly that configuration without truncation or clamping.

#### Scenario: Zero or oversized queue is marked ready

r[chaoscontrol.virtio_safety.validation.negative]
- GIVEN a driver supplies zero, a non-power-of-two value, a value wider than the queue field, or a size above the offered maximum
- WHEN it attempts to mark the queue ready
- THEN activation MUST be rejected with a typed transport violation
- AND queue processing MUST NOT execute modulo by that value or treat a clamped/truncated value as accepted.

### Requirement: Ring progress and addresses are checked

r[chaoscontrol.virtio_safety.ring_progress] Descriptor, available-ring, and used-ring footprints and element addresses MUST be computed with checked arithmetic from validated configuration, and available-index delta MUST NOT exceed queue capacity.

#### Scenario: Ring arithmetic overflows or escapes guest memory

- GIVEN a ring base and queue geometry would overflow an address calculation or extend outside mapped guest memory
- WHEN queue validation or ring access is planned
- THEN the operation MUST be rejected before memory access
- AND no queue cursor, used index, or backend state MAY change.

#### Scenario: Guest advertises excessive available entries

- GIVEN wrapping-index arithmetic shows more unconsumed entries than the queue can contain
- WHEN available work is planned
- THEN the queue MUST enter defined malformed-input handling
- AND the VMM MUST NOT iterate over overwritten or unbounded entries.

### Requirement: Descriptor chains are fully validated

r[chaoscontrol.virtio_safety.descriptor_validation] Before backend processing, ChaosControl MUST validate descriptor head and next indices, a descriptor-count bound no greater than queue capacity, cycle freedom, negotiated/supported flags, checked address-plus-length arithmetic, complete guest-memory containment, and checked aggregate length.

#### Scenario: Descriptor chain is malformed

r[chaoscontrol.virtio_safety.validation.core]
- GIVEN a chain has an invalid index, cycle, unsupported indirect flag, unknown flag, overflowing range, out-of-memory range, or aggregate length above policy
- WHEN descriptor planning runs
- THEN it MUST return a typed validation error in bounded work
- AND it MUST NOT allocate from, read from, or write to the invalid range.

### Requirement: Device request shape is validated

r[chaoscontrol.virtio_safety.request_validation] Block, network, and entropy backends MUST receive only complete request plans whose descriptor order, direction, header/status shape, operation type, storage range, frame bounds, and writable entropy buffers satisfy device-specific invariants.

#### Scenario: Block request has wrong descriptor direction

- GIVEN a block request supplies a read-only status buffer or reverses an operation's required data direction
- WHEN request planning runs
- THEN the request MUST be rejected before disk or guest-memory mutation
- AND a device-specific error completion MAY be written only through an independently validated status buffer.

### Requirement: Host resources are bounded independently of guest lengths

r[chaoscontrol.virtio_safety.resource_bounds] Virtio processing MUST use named queue, chain, aggregate-request, device-transfer, frame, and scratch-buffer limits, and MUST NOT size an infallible host allocation solely from a guest-controlled descriptor length.

#### Scenario: Descriptor requests excessive transfer memory

- GIVEN an otherwise readable descriptor length exceeds the named device or aggregate request limit
- WHEN resource planning runs
- THEN the request MUST fail with a resource-limit outcome before allocation or backend work
- AND host allocation MUST remain within the declared metadata and scratch budgets.

#### Scenario: Bounded allocation fails

- GIVEN a validated bounded metadata or scratch allocation cannot be reserved
- WHEN the shell handles reservation failure
- THEN it MUST return a typed resource failure without panic, abort, cursor commit, or successful completion.

### Requirement: Validation precedes mutation and commits once

r[chaoscontrol.virtio_safety.mutation_order] Queue processing MUST validate the queue, ring progress, complete descriptor chain, request shape, ranges, and resource budget before committing `last_avail_idx`, advancing entropy, mutating block/network state, writing a successful used entry, or raising a completion interrupt.

#### Scenario: Invalid entropy destination is supplied

- GIVEN an entropy request contains an invalid or excessive writable range
- WHEN the request is planned
- THEN validation MUST fail before deterministic entropy state advances
- AND retrying from the same pre-request state MUST produce the same next entropy stream.

### Requirement: Malformed input has deterministic failure semantics

r[chaoscontrol.virtio_safety.failure_semantics] A request-local error MUST produce a device-specific error completion only when its completion path is independently valid; queue or transport corruption MUST stop queue processing in a defined failed or device-needs-reset state. No malformed guest input MAY cause host panic, unbounded allocation, unbounded loop, repeated silent retry, or successful I/O reporting.

#### Scenario: Used-ring write fails after backend side effect

- GIVEN an imperative backend action cannot be safely reversed and the subsequent validated completion write unexpectedly fails
- WHEN the shell handles the failure
- THEN it MUST mark the device in a typed non-success state requiring reset or run termination
- AND it MUST NOT report the request as successfully completed.

### Requirement: Validated queue state is snapshot-ready

r[chaoscontrol.virtio_safety.snapshot_state] The virtio owner MUST expose validated configuration, cursor, failure, and pending-completion state required by the complete VM snapshot owner to preserve deterministic continuation.

#### Scenario: Queue state is captured

- GIVEN a valid queue has partially consumed work or a defined failure state
- WHEN the complete snapshot adapter captures it
- THEN all live validated queue state needed for the next bounded transition MUST be available without reconstructing it from unvalidated guest memory.

### Requirement: Virtio validation has a functional core

r[chaoscontrol.virtio_safety.boundary] Queue-state transitions, ring-footprint and progress calculations, descriptor-chain validation, request planning, resource-budget checks, and failure classification MUST be pure deterministic logic, while MMIO dispatch, guest-memory reads/writes, allocation, backend mutation, interrupt signaling, logging, and persistence remain in imperative shells.

#### Scenario: Validation is replayed in memory

- GIVEN identical raw register facts, memory extents, ring indices, descriptors, device policy, and limits
- WHEN the validation core runs
- THEN it MUST return identical plans or violations without filesystem, environment, clock, process, KVM, guest-memory I/O, allocation side effects, output, or ambient mutable-state access.

### Requirement: Virtio safety validation is adversarial

r[chaoscontrol.virtio_safety.validation] The change MUST test compliant production paths and malformed register, queue, ring, descriptor, request, allocation, backend, completion, and KVM guest paths with no-panic, bounded-resource, and no-prevalidation-mutation oracles.

#### Scenario: Generated adversarial corpus runs

r[chaoscontrol.virtio_safety.validation.fuzz]
- GIVEN generated full-width register values, memory extents, wrapping indices, descriptor graphs, addresses, flags, lengths, and block/net/entropy request shapes
- WHEN pure and bounded-shell properties execute the corpus
- THEN every case MUST return a bounded plan or typed violation without panic or allocation beyond policy.

#### Scenario: Malicious guest uses production MMIO path

r[chaoscontrol.virtio_safety.validation.kvm]
- GIVEN a bounded guest emits malformed queue configuration and descriptors through production virtio MMIO
- WHEN the KVM smoke fixture runs
- THEN the VMM MUST remain alive
- AND each malformed operation MUST end in the specified request error or device reset state rather than host crash or resource exhaustion.
