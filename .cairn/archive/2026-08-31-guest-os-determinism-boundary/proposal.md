# Guest OS Determinism Boundary

## Why

ChaosControl determinizes the surfaces it declares: TSC, CPUID, virtio entropy, block, and network devices. It does not control reads that guest userspace makes outside those surfaces. `clock_gettime`, `getrandom`, `/dev/urandom`, process and thread scheduling, signal delivery, and memory layout can still vary. A workload that reads these surfaces without going through the SDK can break bit-exact replay, and arbitrary unmodified binaries have no replay guarantee at all.

## What Changes

- Bootstrap the guest kernel with a run-derived Linux seed node, then prove CRNG continuation from an admitted quiescent snapshot.
- Fix the admitted guest clock on deterministic jiffies driven by the VMM timer plan.
- Fix memory layout by disabling supported Linux randomization inputs and bind that policy to the run configuration.
- Make signal delivery order derive from the deterministic schedule.
- Add a bit-exact determinism validation fixture that reads every admitted surface and proves identical output across repeated identical runs.

## Impact

- **VMM**: boot-time entropy injection and clock pinning profile.
- **Guest packaging**: determinism validation tool in the initrd.
- **Evidence**: a dedicated drift gate for bit-exact guest reproducibility.
- **Testing**: positive bit-exact runs and negative entropy, clock, and layout drift cases.

## Non-Goals

- No full syscall interception layer in this change.
- No guarantee for host-side or cross-machine runs.
- No claim of invariants beyond the admitted surface list.
