## Why

The current VM snapshot path omits CPU and virtio state that can change future guest execution. Restore can therefore produce a plausible VM that does not continue from the captured execution point.

## What Changes

- Add a versioned exact-completeness profile and component inventory.
- Capture full KVM vCPU state, including XSAVE, events, and the host MSR capability inventory.
- Complete pending KVM userspace exits before capture and restore.
- Capture virtio-mmio negotiation, interrupt, queue geometry, and queue cursor state.
- Capture block, network, and entropy backend state through typed adapters.
- Match devices by stable MMIO identity instead of vector position.
- Run a read-only restore preflight before VM mutation.
- Poison a VM or controller when restore fails after mutation starts.
- **BREAKING**: Snapshot APIs require mutable VM and controller access because KVM exit completion is part of capture.
- **BREAKING**: Incomplete legacy snapshots are rejected instead of receiving default execution state.

## Capabilities

### New Capabilities

- `snapshot-fidelity`: Exact VM snapshot capture, validation, compatibility, restore, and deterministic continuation requirements.

### Modified Capabilities

None.

## Impact

The change affects `chaoscontrol-vmm` snapshot schemas, CPU capture, virtio transport and backend adapters, VM and controller APIs, restore failure handling, KVM tests, and snapshot compatibility documentation. No new runtime dependency is required.
