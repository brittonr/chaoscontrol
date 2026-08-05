## Why

`VmSnapshot` is described as complete, but its vCPU image omits KVM vCPU events, MSRs, and extended processor state beyond the legacy FPU structure. Its virtio image records only a device type and optional block backing snapshot, while restore leaves MMIO transport registers, queue configuration and cursors, interrupt state, negotiated features, virtio-net state, and the virtio-rng generator at their post-snapshot values. A restored VM can therefore resume from guest memory and general registers that do not match the CPU and device state that will service its next instruction or I/O request.

Snapshot-backed replay and fork exploration must either restore every execution-relevant state component or reject the snapshot before mutating the destination VM.

## What Changes

- Define an explicit, versioned inventory of execution-relevant KVM, VMM, transport, queue, and backend state for each configured VM topology.
- Capture and restore required vCPU event, MSR, and extended-state surfaces in addition to the existing register set.
- Capture and restore complete virtio MMIO transport state and deterministic block, network, and entropy backend state.
- Preflight schema, capability, topology, component-presence, and range invariants in a pure validator before any restore mutation.
- Reject incomplete legacy snapshots for exact replay; they may remain inspectable only under an explicit non-proof compatibility path.
- Add positive round-trip and continuation-equivalence tests plus negative missing-state, topology-mismatch, unsupported-capability, and malformed-state tests.

## Impact

- **Files**: `crates/chaoscontrol-vmm/src/snapshot.rs`, VM snapshot/restore orchestration, virtio transport and backend snapshot adapters, snapshot codec tests, and determinism documentation.
- **Compatibility**: snapshots that lack required state or target an incompatible topology will no longer be silently restored as exact snapshots.
- **Reliability**: restore becomes all-or-reject at preflight boundaries; failures name the missing or incompatible component.
- **Scope boundary**: this change owns in-memory VM state fidelity and restore compatibility. It does not own replay-evidence DTO extraction, snapshot-reference path confinement, artifact lookup, or external receipt policy covered by `openspec/changes/extract-replay-evidence-core/`.
- **Claims**: passing the new tests demonstrates bounded state and continuation parity for supported KVM capabilities and fixtures, not universal VM migration across arbitrary kernels, CPUs, or device models.
