## Phase 1: State inventory and compatibility core

- [x] [serial] Define a pure, versioned VM-state inventory over normalized topology and captured KVM capability facts, including required vCPU, in-kernel, VMM, virtio transport, queue, and backend components. r[chaoscontrol.deterministic_snapshots.complete_state] r[chaoscontrol.deterministic_snapshots.boundary]
- [x] [serial] Define pure preflight validation for schema/profile compatibility, complete component sets, stable identities, counts, kinds, byte lengths, and ranges. r[chaoscontrol.deterministic_snapshots.preflight]
- [x] [serial] Add positive complete-inventory assertions and negative missing, duplicate, extra, malformed, unsupported-capability, and topology-mismatch assertions. r[chaoscontrol.deterministic_snapshots.validation.core]

## Phase 2: Complete capture and restore

- [x] [serial] Capture and restore KVM vCPU events, the declared required migratable MSR set, and the selected extended-state representation with phase-specific errors. r[chaoscontrol.deterministic_snapshots.vcpu_state]
- [x] [parallel] Add complete snapshot adapters for virtio MMIO registers and every queue's configuration and cursor. r[chaoscontrol.deterministic_snapshots.virtio_state]
- [x] [parallel] Add complete deterministic backend adapters for block state, network state, and virtio-rng state. r[chaoscontrol.deterministic_snapshots.virtio_state]
- [x] [serial] Replace positional device restore with stable-identity matching and reject unknown, missing, duplicate, extra, or wrong-kind devices before mutation. r[chaoscontrol.deterministic_snapshots.preflight]
- [x] [serial] Apply the documented restore state machine and keep a VM non-runnable after any imperative restore failure. r[chaoscontrol.deterministic_snapshots.restore_atomicity]

## Phase 3: Payload compatibility

- [x] [serial] Add an internal snapshot state-schema version and completeness profile without taking ownership of external replay references or artifact paths. r[chaoscontrol.deterministic_snapshots.compatibility]
- [x] [serial] Reject incomplete legacy payloads for exact restore and snapshot-backed replay while preserving explicit inspection or diagnostic migration behavior. r[chaoscontrol.deterministic_snapshots.compatibility.legacy]

## Phase 4: Regression evidence

- [x] [parallel] Add field-complete vCPU, transport, queue, block, network, and entropy round-trip tests. r[chaoscontrol.deterministic_snapshots.validation.round_trip]
- [x] [parallel] Add negative restore tests proving preflight rejects incompatible state without mutating the destination. r[chaoscontrol.deterministic_snapshots.validation.preflight]
- [x] [serial] Add bounded KVM continuation-equivalence tests at pending-event, extended-register, queue-cursor, pending-interrupt, network, block, and entropy boundaries. r[chaoscontrol.deterministic_snapshots.validation.continuation]
- [x] [serial] Document supported capability profiles, legacy behavior, phase failures, and the bounded portability non-claim. r[chaoscontrol.deterministic_snapshots.compatibility]
- [x] [serial] Run focused snapshot/device tests, workspace tests, the snapshot replay smoke gate, Cairn validation, and proposal/design/tasks gates before sync or archive. r[chaoscontrol.deterministic_snapshots.validation]
