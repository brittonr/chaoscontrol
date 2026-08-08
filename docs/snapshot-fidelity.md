# Snapshot fidelity and compatibility

ChaosControl snapshots use the `exact-x86-kvm-v1` completeness profile.
The current state schema version is `2`.

## Exact state boundary

The profile includes these state groups:

- guest memory;
- KVM IRQ chip, PIT, and clock state;
- general, special, debug, LAPIC, MP, XCR, XSAVE, event, and required MSR state for each vCPU;
- deterministic time, entropy, timer, panic detection, counters, coverage, fault, and scheduler state;
- serial controller state;
- virtio-mmio transport registers and queue progress;
- block, network, and entropy backend state.

Each snapshot contains an inventory of these components. It also contains the vCPU count and stable virtio identities. A virtio identity contains the MMIO base address, IRQ, and device type. Restore does not use vector position as device identity.

Host handles are not in the deterministic state boundary. These handles include file descriptors, timer handles, performance counter handles, log writers, and output sinks. Restore creates new handles or configures them from restored deterministic state.

## Capture rules

Before capture, the VMM completes pending KVM userspace exits with `immediate_exit` set. This step completes prior I/O or MMIO emulation without running another guest instruction.

Capture fails if a device operation is in progress, a device is failed, schedule evidence is pending, the VM is poisoned, or a backend has no exact snapshot adapter. Capture also fails if KVM cannot return every required vCPU state group.

## Restore rules

Restore first runs a read-only preflight. Preflight checks the schema, profile, inventory, topology, memory shape, vCPU state, scheduler state, serial bounds, fault state, and every virtio transport and backend adapter.

A preflight failure does not change VM state. A failure after restore starts permanently poisons the VM. A simulation restore failure after mutation starts also poisons the controller. The caller must discard a poisoned VM or controller.

## Compatibility policy

Schema `2` restores only into the same exact profile and matching VM topology. The snapshot records the sorted `KVM_GET_MSR_INDEX_LIST` inventory. The restore host must expose that same inventory and support all required KVM state operations.

Legacy snapshots that omit XSAVE, vCPU events, required MSRs, metadata, or the complete inventory are incompatible. Decode or preflight rejects them. ChaosControl does not silently fill missing execution state with defaults.

A newer reader can accept an older snapshot only through an explicit migration that produces a complete current inventory. No such migration exists for incomplete legacy CPU or device state.

## Claim boundary

The profile records and restores the state that ChaosControl owns for deterministic continuation. It does not prove KVM correctness, guest correctness, cross-kernel ABI equivalence, or portability between different VM topologies.
