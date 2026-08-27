# Deterministic Snapshots Specification

## Purpose

Restore supported ChaosControl VMs from state-complete, topology-compatible snapshots without silently carrying execution-relevant live state across the restore boundary.

## Requirements

### Requirement: Exact snapshots contain every required execution state component

r[chaoscontrol.deterministic_snapshots.complete_state] An exact VM snapshot MUST contain the complete component inventory required by its declared VM topology and KVM capability profile, including vCPU architecture state, in-kernel device state, guest memory, VMM determinism state, virtio transport and queue state, and mutable deterministic backend state.

#### Scenario: Complete state is captured

r[chaoscontrol.deterministic_snapshots.validation.round_trip]
- GIVEN a supported VM whose vCPU events, MSRs, extended registers, VMM counters, virtio registers, queue cursors, interrupts, block state, network state, and entropy state have been mutated
- WHEN ChaosControl captures an exact snapshot
- THEN every required inventory component MUST be present under its stable identity
- AND canonical snapshot state MUST round-trip without field loss.

#### Scenario: Required host capability is unavailable

- GIVEN an exact snapshot profile requires a KVM state surface that the host cannot read or restore
- WHEN capture eligibility is evaluated
- THEN capture MUST fail with a typed capability diagnostic
- AND ChaosControl MUST NOT silently emit a weaker snapshot under the exact profile.

### Requirement: vCPU architecture state is complete

r[chaoscontrol.deterministic_snapshots.vcpu_state] ChaosControl MUST capture and restore KVM vCPU events, the profile's required migratable MSRs, and its selected extended-state representation in addition to general, special, debug, interrupt-controller, control, and multiprocessor state.

#### Scenario: Pending architectural state survives restore

- GIVEN a vCPU has pending event state, non-default required MSRs, and non-default extended registers at a quiescent checkpoint
- WHEN the snapshot is restored on a compatible destination
- THEN the first bounded continuation MUST observe the same architectural state and event ordering as the uninterrupted control execution.

### Requirement: Virtio transport and backend state is complete

r[chaoscontrol.deterministic_snapshots.virtio_state] Each virtio snapshot MUST bind a stable device identity to all mutable MMIO transport fields, every queue's configuration and cursor, pending interrupt state, and all mutable deterministic backend state for that device kind.

#### Scenario: Queue and backend continuation is restored

- GIVEN a virtio device has negotiated features, a partially consumed queue, a pending interrupt, and mutated backend state
- WHEN a compatible snapshot is restored
- THEN the next MMIO observations, descriptor completion, interrupt behavior, and backend output MUST match the bounded uninterrupted control execution.

### Requirement: Restore preflight rejects incompatible state before mutation

r[chaoscontrol.deterministic_snapshots.preflight] ChaosControl MUST validate snapshot schema/profile compatibility, complete component presence, byte and range invariants, vCPU topology, stable device identities, queue counts, and backend kinds in a pure preflight step before mutating the destination VM.

#### Scenario: Device topology does not match

r[chaoscontrol.deterministic_snapshots.validation.preflight]
- GIVEN a snapshot has a missing, duplicate, extra, reordered-without-identity, or wrong-kind virtio component relative to the destination
- WHEN restore preflight runs
- THEN preflight MUST reject the snapshot with a component-specific diagnostic
- AND the destination VM MUST remain unchanged and non-resumed.

#### Scenario: Snapshot bytes are malformed

- GIVEN a required architecture or device field has an invalid length, range, enum value, or component identity
- WHEN restore preflight runs
- THEN the snapshot MUST be rejected before any KVM ioctl or device mutation is attempted.

### Requirement: Imperative restore failure cannot publish success

r[chaoscontrol.deterministic_snapshots.restore_atomicity] After successful preflight, the restore shell MUST apply state in an explicit dependency order and MUST keep the VM non-runnable if any KVM or device operation fails before all restore postconditions hold.

#### Scenario: KVM restore operation fails

- GIVEN preflight succeeded but a required KVM restore operation returns an error
- WHEN the imperative restore state machine handles the failure
- THEN it MUST return a phase-specific failure
- AND it MUST NOT resume the VM or report snapshot restore success.

### Requirement: Snapshot fidelity compatibility is explicit

r[chaoscontrol.deterministic_snapshots.compatibility] Exact snapshot payloads MUST declare an internal state-schema version and completeness profile independently of external artifact references and storage paths.

#### Scenario: Incomplete legacy snapshot is presented for exact replay

r[chaoscontrol.deterministic_snapshots.compatibility.legacy]
- GIVEN a legacy payload omits one or more components required by the current exact profile
- WHEN exact restore or snapshot-backed replay requests that payload
- THEN ChaosControl MUST reject it as incomplete
- AND any inspection or migration mode MUST label it as non-proof state rather than silently promoting it.

### Requirement: Snapshot decisions have a functional core

r[chaoscontrol.deterministic_snapshots.boundary] Inventory construction, compatibility checks, topology matching, component validation, restore planning, and state-transition eligibility MUST be pure deterministic logic, while KVM ioctls, guest-memory access, device mutation, binding setup, logging, and resume remain in the imperative shell.

#### Scenario: Identical state facts produce identical plans

r[chaoscontrol.deterministic_snapshots.validation.core]
- GIVEN identical normalized topology, capability facts, snapshot metadata, and component records
- WHEN the snapshot core validates and plans restore
- THEN it MUST return the same plan or blockers without filesystem, environment, clock, process, KVM, output, or ambient mutable-state access.

### Requirement: Snapshot validation compares bounded continuation

r[chaoscontrol.deterministic_snapshots.validation] The change MUST test pure inventory validation, field-complete state round trips, preflight failure isolation, and bounded KVM continuation equivalence across every supported state owner.

#### Scenario: Restored execution matches control execution

r[chaoscontrol.deterministic_snapshots.validation.continuation]
- GIVEN a supported fixture is checkpointed at each declared architecture and device boundary
- WHEN one branch continues uninterrupted and another restores before continuing
- THEN their bounded schedule, exit, interrupt, device-completion, and output observations MUST match
- AND the result MUST remain scoped to the tested host capability profile and observation horizon.

## Portable exact-snapshot descriptor requirements

### Requirement: Public snapshot descriptors have canonical producer identity

r[chaoscontrol.snapshot_descriptor.contract] ChaosControl MUST define a versioned Rust-owned public snapshot descriptor. It MUST compute descriptor identity with domain-separated BLAKE3 framing over complete versioned canonical hash material and deterministic ordered inventories.

#### Scenario: Equivalent descriptors use different JSON formatting

- GIVEN two JSON projections decode to the same valid descriptor fields
- WHEN canonical descriptor identity is computed
- THEN both projections MUST produce the same descriptor identity

#### Scenario: Required cohort field changes

- GIVEN one behavior-relevant descriptor field changes
- WHEN identity is recomputed
- THEN the descriptor identity MUST change

### Requirement: Descriptors bind the complete exact cohort

r[chaoscontrol.snapshot_descriptor.complete_cohort] A descriptor MUST bind the exact completeness profile, state schema, architecture, KVM operation cohort, sorted MSR inventory, CPU and memory topology, stable device identities, backend kinds, deterministic state owners, guest artifacts, scheduler, time, entropy, and payload closure.

#### Scenario: Complete exact descriptor is admitted

- GIVEN every required field and component is present once under its stable identity
- WHEN descriptor validation runs
- THEN the descriptor MAY proceed to destination preflight

#### Scenario: Required backend state owner is absent

- GIVEN one mutable deterministic backend is missing from the descriptor inventory
- WHEN validation runs
- THEN ChaosControl MUST reject the descriptor as incomplete

### Requirement: Descriptor identity is independent of locators

r[chaoscontrol.snapshot_descriptor.locator_boundary] Canonical descriptors MUST contain tagged content identities, lengths, codecs, order, and closure roles. Paths, store names, Redb keys, tickets, URLs, mirrors, and provider handles MUST remain detached locator observations.

#### Scenario: Snapshot moves to another store

- GIVEN identical verified payload bytes and descriptor facts receive a new locator
- WHEN descriptor identity is recomputed
- THEN the descriptor identity MUST remain unchanged

#### Scenario: Locator is supplied without content identity

- GIVEN a confined path exists but no valid payload identity and length are known
- WHEN descriptor admission runs
- THEN ChaosControl MUST reject the reference as incomplete

### Requirement: Payload closure supports monolithic and chunked artifacts

r[chaoscontrol.snapshot_descriptor.closure] The descriptor MUST bind either one exact monolithic payload or one ordered complete chunk manifest. Every member MUST have a tagged digest algorithm, digest, length, role, and order where applicable. Unknown algorithms, gaps, overlaps, truncation, or digest mismatch MUST fail closed.

#### Scenario: Chunked closure verifies

- GIVEN every ordered chunk matches its declared identity and total logical payload facts
- WHEN closure validation runs
- THEN ChaosControl MUST admit the closure without requiring a host path in canonical identity

#### Scenario: One chunk is reordered

- GIVEN all chunk bytes exist but their order differs from the descriptor
- WHEN closure validation runs
- THEN ChaosControl MUST reject the payload

### Requirement: Destination preflight remains pure and exact

r[chaoscontrol.snapshot_descriptor.preflight] Preflight MUST compare a valid descriptor with supplied destination architecture, KVM, MSR, topology, device, backend, and resource observations without I/O. Unsupported or mismatching facts MUST fail before VM mutation.

#### Scenario: Destination MSR inventory differs

- GIVEN a valid descriptor and a destination with a different required MSR inventory
- WHEN preflight runs
- THEN it MUST reject restore before any KVM or device mutation

### Requirement: Restore receipts preserve mutation and poison state

r[chaoscontrol.snapshot_descriptor.restore_receipt] Detached restore receipts MUST bind descriptor, destination cohort, preflight, materialization, mutation-start, ordered phase results, poison state, completion, and bounded continuation observations.

#### Scenario: Restore fails after mutation starts

- GIVEN preflight passed and one required restore phase fails after destination mutation
- WHEN the receipt is emitted
- THEN it MUST report the destination as poisoned
- AND it MUST NOT report restore success

### Requirement: Public projections remain Rust-owned and reviewable

r[chaoscontrol.snapshot_descriptor.projection] Rust DTOs MUST own emitted descriptor and restore-receipt facts. JSON schemas and Nickel review contracts MUST be generated or checked from that owner with fixture parity and content-bound freshness.

#### Scenario: Rust shape changes without contract update

- GIVEN a descriptor field changes while its schema, contract, or fixtures remain stale
- WHEN projection freshness runs
- THEN validation MUST fail before publication

### Requirement: Consumer fixtures preserve ChaosControl ownership

r[chaoscontrol.snapshot_descriptor.consumer_contract] ChaosControl MUST publish a refs-only external-consumer fixture for identity, closure, and exact compatibility. The fixture MUST NOT encode Molten branch, merge, authority, promotion, or release semantics.

#### Scenario: Consumer treats descriptor as restore authority

- GIVEN a consumer fixture claims descriptor validity grants runtime activation
- WHEN conformance validation runs
- THEN ChaosControl MUST reject the claim as an authority overreach

### Requirement: Descriptor verification covers hostile compatibility inputs

r[chaoscontrol.snapshot_descriptor.verification] ChaosControl MUST test valid descriptors, stable identities, complete closures, exact preflight, restore observations, incomplete inventories, locator substitution, payload tamper, cohort mismatch, poison, and portability overclaims.

#### Scenario: Focused descriptor rail runs

- GIVEN positive and negative fixtures use the reviewed exact snapshot cohort
- WHEN descriptor verification runs
- THEN it MUST report the exact supported profile and all bounded non-claims
