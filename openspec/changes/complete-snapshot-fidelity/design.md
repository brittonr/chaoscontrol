## Context

The snapshot path already captures guest memory, in-kernel interrupt and timer state, selected registers, and deterministic shell state. It does not capture all KVM CPU state or virtio transport state. Device restore also depends on vector order. See `proposal.md` and `specs/snapshot-fidelity/spec.md` for the required behavior.

KVM userspace exits have a completion phase. Reading registers immediately after an I/O exit can capture the instruction before KVM completes that exit. Device queue cursors also depend on indices stored in snapshot guest memory.

## Goals / Non-Goals

**Goals:**

- Define one explicit exact snapshot profile.
- Make omitted execution state a capture or preflight error.
- Keep validation logic separate from mutation shells.
- Preserve deterministic continuation across serialization.
- Make post-mutation restore failure terminal and observable.

**Non-Goals:**

- Portable restore across different VM topologies or KVM capability inventories.
- Migration of legacy snapshots that never recorded required state.
- Proof of KVM or guest correctness.
- Snapshot of host file descriptors, timers, output sinks, or log writers.

## Decisions

### Use a versioned completeness profile and inventory

Each snapshot records schema version `2`, profile `exact-x86-kvm-v1`, a sorted component inventory, and topology. Restore compares this declaration with the live VM before mutation.

An implicit schema was rejected because it cannot distinguish a valid old snapshot from one that silently omitted state.

### Complete pending KVM exits with immediate exit

Capture and restore first set `immediate_exit`, enter each vCPU once, and require an interrupt result. This completes prior userspace exit emulation without retiring another guest instruction.

Capturing the raw `kvm_run` mapping was rejected because it is a host ABI buffer, not a stable snapshot contract.

### Capture the host KVM MSR capability inventory

The VMM reads, sorts, and deduplicates `KVM_GET_MSR_INDEX_LIST`. Every vCPU snapshot must contain that exact inventory, and partial reads or writes fail.

A fixed hand-written MSR list was rejected because guest-visible supported MSRs can exceed that list and vary by host kernel.

### Use typed CPU and device state adapters

CPU POD structures use fixed-size byte serialization with size checks. Virtio transport state has explicit serializable records. Block, network, and entropy backends use a typed enum. Capture rejects an unknown backend adapter.

An untyped backend byte blob was rejected because it allows type confusion and hidden defaults.

### Bind devices by stable transport identity

The stable identity is MMIO base address, IRQ, and virtio device type. Snapshot topology and restore lookup use this identity. Queue count remains a topology constraint.

Vector position was rejected because collection order is not device identity.

### Validate queue cursors against snapshot memory

Preflight materializes snapshot memory in a temporary guest-memory mapping. Transport validation checks queue geometry and cursor consistency against that mapping. Restore validates again after snapshot memory has been applied.

Checking live guest memory during preflight was rejected because live memory can validly differ from snapshot memory.

### Split preflight from mutation and poison on late failure

The public restore shell runs all structural checks first. It then calls a validated mutation path. Any mutation-path error latches a permanent VM poison. Multi-VM shells also latch controller poison.

Rollback was rejected because KVM state writes are not transactional and rollback can fail after partial mutation.

### Reject incomplete legacy state

Legacy fields can use serde defaults only to produce a clear completeness diagnostic. Metadata and full CPU state remain mandatory for restore. No default execution state is synthesized.

Silent defaulting was rejected because it creates non-equivalent continuation.

## Risks / Trade-offs

- **[Preflight copies snapshot memory]** → This adds memory and time cost, but it permits read-only queue cross-validation before live VM mutation.
- **[KVM capability inventories vary]** → Restore fails with an explicit topology mismatch instead of claiming portability.
- **[Snapshot APIs become mutable]** → The API reflects the required KVM exit-completion effect.
- **[A late kernel failure poisons the VM]** → Callers must discard it, which is safer than continuing from uncertain mixed state.
- **[Exact adapters cover only known backends]** → New backends must add capture, validation, restore, and positive and negative tests.

## Migration Plan

1. Emit only schema `2` exact-profile snapshots from the updated VMM.
2. Reject incomplete legacy snapshots at decode or preflight.
3. Update callers for mutable snapshot methods.
4. Keep snapshot artifacts versioned so rollback can use the prior binary with prior artifacts.
5. Do not rewrite or relabel legacy artifacts as exact snapshots.
