## Why

The current fault model covers network, disk I/O, process lifecycle, clock,
and interrupt injection — but misses several failure modes that real
distributed systems hit in production. Filesystem-level faults (fsync lies,
slow I/O), CPU-level faults (register bitflips, SIGBUS), and richer clock
manipulation (monotonic clock freezes, backward jumps with clamping) would
let ChaosControl find bugs that the existing fault categories cannot reach.

## What Changes

- **New disk faults**: `DiskSlow` (adds per-operation latency to reads/writes),
  `DiskFsyncLie` (silently drops writes that haven't been fsynced — models
  the ext4 data=writeback + power-loss scenario), `DiskPartialRead` (returns
  fewer bytes than requested, simulating short reads from degraded storage)
- **New CPU faults**: `CpuBitflip` (flips a random bit in a general-purpose
  register at a tick boundary — models single-event upsets / cosmic ray
  bitflips), `CpuStall` (pauses a single vCPU for N ticks while other vCPUs
  continue — models a core entering C-state or thermal throttling in SMP)
- **New clock faults**: `ClockFreeze` (holds the virtual TSC at a fixed value
  for N ticks — models a stuck clock source), `ClockJitter` (adds random
  per-exit TSC noise within a bound — models unstable oscillator)
- Update `generate_random_fault()` in FaultEngine and `random_fault()` in
  ScheduleMutator to include the new types
- Update `apply_fault()` in SimulationController to dispatch the new types
- Serialize/deserialize the new variants for checkpoint and bug report
  compatibility

## Capabilities

### New Capabilities
- `disk-advanced-faults`: DiskSlow, DiskFsyncLie, DiskPartialRead fault variants with block device integration
- `cpu-faults`: CpuBitflip and CpuStall fault variants with KVM register/vCPU manipulation
- `clock-advanced-faults`: ClockFreeze and ClockJitter fault variants with virtual TSC manipulation

### Modified Capabilities

## Impact

- `chaoscontrol-fault`: New `Fault` enum variants, `FaultCategory` unchanged (reuses Disk, Clock; adds Cpu)
- `chaoscontrol-vmm`: Block device gains slow-io and fsync-lie state; controller dispatches new faults; VM exposes register read/write for bitflip
- `chaoscontrol-explore`: Mutator gains 7 new fault types (15 → 22)
- `chaoscontrol-fault` engine: random fault generator gains 7 new types (13 → 20)
- Serde: new variants need `#[serde(rename_all = "snake_case")]` or tag matching for backward-compatible checkpoint loading
