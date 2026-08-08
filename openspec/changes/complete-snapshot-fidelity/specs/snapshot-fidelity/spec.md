## Purpose

Defines the complete state, validation, compatibility, and failure rules for exact deterministic VM continuation from a snapshot.

## ADDED Requirements

### Requirement: Snapshot declares exact completeness
A restorable snapshot SHALL contain a state schema version, completeness profile, component inventory, vCPU count, KVM MSR capability inventory, and stable device topology.

#### Scenario: Complete snapshot capture
- **WHEN** snapshot capture succeeds
- **THEN** the snapshot declares the exact profile and every required component

#### Scenario: Missing component declaration
- **WHEN** restore receives a snapshot with a missing or duplicate required component
- **THEN** restore rejects the snapshot before VM mutation

### Requirement: Capture uses a quiescent execution boundary
The system SHALL complete pending KVM userspace exits without running another guest instruction before CPU state capture. The system SHALL reject device capture when an operation is in progress or a device is failed.

#### Scenario: Pending I/O exit
- **WHEN** capture starts after a KVM I/O exit
- **THEN** the prior exit is completed with an immediate-exit boundary before register capture

#### Scenario: In-flight device operation
- **WHEN** a virtio queue has pending completion state
- **THEN** capture fails instead of producing a partial snapshot

### Requirement: Snapshot preserves complete vCPU state
The system SHALL capture and restore general, special, debug, LAPIC, MP, XCR, XSAVE, event, FPU, and KVM-exposed MSR state for each vCPU. Partial KVM reads or writes SHALL fail.

#### Scenario: Exact CPU continuation
- **WHEN** a serialized vCPU snapshot is restored
- **THEN** the subsequent guest outputs and KVM exit sequence match uninterrupted execution

#### Scenario: Host capability mismatch
- **WHEN** the restore host exposes a different KVM MSR inventory
- **THEN** preflight rejects the snapshot

### Requirement: Snapshot preserves complete virtio state
The system SHALL capture and restore virtio-mmio feature negotiation, selectors, status, interrupt status, configuration generation, queue geometry, readiness, and queue cursors. It SHALL capture each supported backend through a typed exact-state adapter.

#### Scenario: Transport round trip
- **WHEN** a configured virtio transport is captured, reset, and restored
- **THEN** its negotiation state, queue geometry, and cursors equal the captured state

#### Scenario: Queue cursor conflict
- **WHEN** a queue cursor conflicts with the queue indices in snapshot guest memory
- **THEN** preflight rejects the snapshot without changing the live transport

#### Scenario: Unsupported backend
- **WHEN** a configured backend has no exact snapshot adapter
- **THEN** capture fails

### Requirement: Restore binds devices by stable identity
Restore SHALL bind a device snapshot by MMIO base address, IRQ, and device type. Restore SHALL NOT use vector position as device identity.

#### Scenario: Reordered runtime collection
- **WHEN** runtime device collection order differs but all stable identities match
- **THEN** restore applies each snapshot to its matching device

#### Scenario: Duplicate or unknown identity
- **WHEN** snapshot topology contains a duplicate or unknown device identity
- **THEN** preflight rejects the snapshot

### Requirement: Restore validates before mutation
Restore SHALL validate schema, profile, inventory, topology, memory shape, vCPU state, serial bounds, scheduler state, fault state, and device state before it changes VM state.

#### Scenario: Corrupt snapshot
- **WHEN** a snapshot is malformed, truncated, structurally inconsistent, or incompatible
- **THEN** restore fails before changing the VM

#### Scenario: Multi-VM preflight
- **WHEN** one VM snapshot in a simulation fails preflight
- **THEN** no VM or simulation state is restored

### Requirement: Post-mutation failures fail closed
If restore fails after mutation starts, the system SHALL permanently poison the affected VM. A multi-VM restore SHALL also poison the controller.

#### Scenario: Kernel restore failure
- **WHEN** a KVM state write fails after restore mutation starts
- **THEN** subsequent execution is rejected as poisoned

#### Scenario: Controller restore failure
- **WHEN** any VM restore fails after simulation mutation starts
- **THEN** subsequent controller execution is rejected as poisoned

### Requirement: Compatibility is explicit
The system SHALL reject incomplete legacy snapshots. It SHALL accept an older snapshot only through an explicit migration that produces the complete current inventory.

#### Scenario: Legacy CPU state omission
- **WHEN** a legacy snapshot omits XSAVE, vCPU events, required MSRs, metadata, or inventory
- **THEN** decode or preflight returns an explicit incompatibility error

#### Scenario: Truncated serialization
- **WHEN** serialized snapshot data is truncated
- **THEN** decode returns an error and does not produce a restorable snapshot
