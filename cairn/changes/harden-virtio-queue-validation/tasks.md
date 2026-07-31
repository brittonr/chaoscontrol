## Phase 1: Queue and request validation core

- [ ] [serial] Define raw queue inputs, validated queue configuration, legal state transitions, named `VirtioLimits`, typed violations, and assertions. r[chaoscontrol.virtio_safety.queue_configuration] r[chaoscontrol.virtio_safety.boundary]
- [ ] [serial] Implement pure checked ring-footprint, alignment, guest-memory containment, feature/status, readiness, and available-index delta validation. r[chaoscontrol.virtio_safety.queue_configuration] r[chaoscontrol.virtio_safety.ring_progress]
- [ ] [serial] Implement pure bounded descriptor-chain and block/net/entropy request planners with checked arithmetic, range, flag, direction, shape, and aggregate-budget validation. r[chaoscontrol.virtio_safety.descriptor_validation] r[chaoscontrol.virtio_safety.request_validation]
- [ ] [serial] Add positive core assertions and negative zero/truncated/oversized size, misalignment, overlap, overflow, out-of-range, excessive-delta, cycle, bad-index, unsupported-flag, wrong-direction, malformed-shape, and excessive-length assertions. r[chaoscontrol.virtio_safety.validation.core]

## Phase 2: Bounded imperative shell

- [ ] [serial] Route MMIO queue writes and readiness through the validated state machine and process queues only after legal feature/status/ready transitions. r[chaoscontrol.virtio_safety.queue_configuration]
- [ ] [serial] Replace early `pop_avail` cursor mutation with planned bounded work and one outcome-dependent cursor commit. r[chaoscontrol.virtio_safety.mutation_order]
- [ ] [parallel] Replace guest-sized block allocations with validated chunked transfers and checked disk-range handling. r[chaoscontrol.virtio_safety.resource_bounds]
- [ ] [parallel] Replace guest-sized net allocations with validated frame/request bounds and bounded buffers. r[chaoscontrol.virtio_safety.resource_bounds]
- [ ] [parallel] Replace guest-sized entropy allocations with validated writable ranges and bounded chunks that advance PRNG state only after plan acceptance. r[chaoscontrol.virtio_safety.resource_bounds] r[chaoscontrol.virtio_safety.mutation_order]
- [ ] [serial] Make metadata allocation fallible and bounded by validated queue/descriptor limits. r[chaoscontrol.virtio_safety.resource_bounds]

## Phase 3: Failure and device state

- [ ] [serial] Implement typed request-local error completions and transport/queue needs-reset behavior with bounded diagnostics and no repeated malformed processing. r[chaoscontrol.virtio_safety.failure_semantics]
- [ ] [serial] Ensure failed guest memory, backend, used-ring, and interrupt operations cannot publish successful completion and define non-reversible failure state. r[chaoscontrol.virtio_safety.failure_semantics]
- [ ] [serial] Expose validated queue/failure state to `complete-vm-snapshot-state` without duplicating snapshot payload or restore ownership. r[chaoscontrol.virtio_safety.snapshot_state]

## Phase 4: Adversarial regression evidence

- [ ] [parallel] Add compliant queue negotiation and block/net/entropy request tests through production transport and backend paths. r[chaoscontrol.virtio_safety.validation.positive]
- [ ] [parallel] Add malformed MMIO/queue/descriptor/request and injected allocation/backend failure tests proving bounded typed outcomes and no pre-validation mutation. r[chaoscontrol.virtio_safety.validation.negative]
- [ ] [serial] Add property/fuzz coverage for raw register values, memory extents, wrapping indices, descriptor graphs, addresses, flags, lengths, and request shapes with no-panic and resource-bound oracles. r[chaoscontrol.virtio_safety.validation.fuzz]
- [ ] [serial] Add a bounded malicious-guest KVM smoke test through production MMIO dispatch and verify the VMM remains alive with deterministic device failure. r[chaoscontrol.virtio_safety.validation.kvm]
- [ ] [serial] Document strict invalid-input behavior, named limits, reset semantics, supported features, and the bounded virtio-conformance claim. r[chaoscontrol.virtio_safety.failure_semantics]
- [ ] [serial] Run focused virtio tests, property/fuzz regression corpus, workspace tests, KVM smoke tests, Cairn validation, and proposal/design/tasks gates before sync or archive. r[chaoscontrol.virtio_safety.validation]
