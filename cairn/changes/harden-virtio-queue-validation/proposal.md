## Why

The virtio MMIO transport accepts a zero queue size, truncates the guest's register value to `u16`, clamps oversized values, and marks a queue ready without validating size or addresses. Queue processing then computes ring positions with modulo `self.size`, so a guest can make a zero-sized ready queue reach a division-by-zero panic. Descriptor and ring addresses use unchecked arithmetic, and guest-controlled descriptor lengths directly size `Vec` allocations in block, net, and entropy backends, allowing malformed guests to trigger overflow behavior, large allocation pressure, or process abort rather than a bounded device error.

All queue geometry, descriptor chains, request shape, address ranges, and resource budgets must be validated before queue cursor movement, backend mutation, entropy advancement, or allocation.

## What Changes

- Add a pure virtio queue configuration/state-transition core with checked width conversion, size, power-of-two, alignment, footprint, guest-memory, feature, status, and readiness validation.
- Validate available-ring deltas and complete descriptor chains with checked arithmetic, cycle/count bounds, supported flags, direction, range, and aggregate-length limits.
- Define named per-device request and scratch-buffer budgets; replace guest-sized allocations with bounded chunked processing or checked bounded buffers.
- Plan a complete request before advancing queue cursors or mutating block, network, entropy, interrupt, or used-ring state.
- Return typed request/queue/transport failures and deterministic virtio status/completion behavior; malformed input cannot panic, spin indefinitely, or be reported as successful I/O.
- Add positive Linux-compatible queue/request tests and negative malformed-MMIO, zero/oversized queue, overflow, out-of-bounds, cycle, unsupported-flag, wrong-direction, excessive-length, allocation-failure, and fuzz/property tests.

## Impact

- **Files**: `virtio_mmio.rs`, block/net/entropy virtio backends, VM MMIO dispatch, device status/interrupt behavior, queue snapshot adapters, and virtio tests/fuzz fixtures.
- **Compatibility**: invalid queue values that were truncated or clamped will be rejected; compliant modern virtio drivers retain their expected configuration and request behavior.
- **Security**: guest-controlled values no longer directly select unchecked arithmetic or unbounded allocations in the host VMM.
- **Reliability**: malformed requests consume a bounded transition and produce a defined completion or device-needs-reset state rather than panic or unbounded retry.
- **Scope boundary**: this change defines validated live queue state. `complete-vm-snapshot-state` owns persistence and restore of that state; replay DTO and artifact-path work remain outside this package.
- **Claims**: tests establish bounded behavior for implemented virtio MMIO block, net, and entropy paths, not full conformance for unsupported virtio transports or device classes.
