## 1. Snapshot Contract

- [x] 1.1 Add schema version, exact profile, component inventory, topology, and stable device identity records
- [x] 1.2 Record and validate the canonical KVM MSR capability inventory
- [x] 1.3 Reject incomplete legacy CPU state and truncated serialization

## 2. CPU and KVM Continuation

- [x] 2.1 Complete pending KVM userspace exits at an immediate-exit boundary
- [x] 2.2 Capture and restore XSAVE, events, FPU, XCR, debug, LAPIC, MP, register, and MSR state
- [x] 2.3 Fail on partial KVM state reads and writes
- [x] 2.4 Add a KVM continuation test for identical guest outputs and exit order

## 3. Virtio and Backend State

- [x] 3.1 Add serializable virtio-mmio negotiation, interrupt, queue geometry, and cursor state
- [x] 3.2 Validate queue cursors against snapshot guest memory
- [x] 3.3 Add typed exact adapters for block, network, and entropy backends
- [x] 3.4 Restore devices by stable identity and reject unknown or duplicate identities
- [x] 3.5 Add positive and negative virtio transport tests

## 4. Restore Safety

- [x] 4.1 Add read-only VM and simulation preflight before mutation
- [x] 4.2 Restore deterministic VMM counters and panic-detection state exactly
- [x] 4.3 Poison a VM after a post-mutation restore failure
- [x] 4.4 Poison a controller after a post-mutation multi-VM restore failure

## 5. Documentation and Validation

- [x] 5.1 Document the exact state boundary, exclusions, compatibility policy, and non-claims
- [x] 5.2 Run formatting, full `chaoscontrol-vmm` tests, and clippy
- [x] 5.3 Validate the OpenSpec change in strict mode
