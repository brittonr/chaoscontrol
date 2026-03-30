## Context

ChaosControl's fault injection operates at three layers:

1. **Fault definition** (`chaoscontrol-fault/src/faults.rs`): `Fault` enum + `FaultCategory` + serde
2. **Fault dispatch** (`chaoscontrol-vmm/src/controller.rs`): `apply_fault()` maps enum variants to VMM operations
3. **Random generation** (`chaoscontrol-fault/src/engine.rs` + `chaoscontrol-explore/src/mutator.rs`): both have a `match fault_type` block that selects from N types

Adding new faults means touching all three layers plus the block device
(`devices/block.rs`) and the VM's TSC/register accessors.

The block device is CoW-based (`base: Arc<Vec<u8>>` + `dirty: BTreeMap`).
Faults are queued via `inject_fault(BlockFault)` and consumed on matching
I/O. The virtual TSC is managed by `VirtualTsc` and written to the guest
via `sync_tsc_to_guest()` before every `vcpu.run()`.

## Goals / Non-Goals

**Goals:**
- Add 7 new fault variants across 3 categories (disk, cpu, clock)
- Each variant is deterministic, snapshot-safe, and serializable
- Random generators and mutators include all new types
- Backward-compatible checkpoint deserialization

**Non-Goals:**
- Filesystem-aware faults (journal corruption, inode manipulation) — too guest-specific
- Memory bitflips (would need KVM dirty log + guest page table walking) — separate effort
- Per-sector slow I/O profiles (latency curve modeling) — DiskSlow uses a flat delay

## Decisions

### 1. DiskSlow: persistent delay state on DeterministicBlock

The block device gets a `slow_delay_ns: u64` field (default 0). When
nonzero, `read()` and `write()` return a `BlockResult` that carries the
delay alongside the data. The controller converts `delay_ns` to TSC ticks
(`delay_ns * tsc_freq / 1_000_000_000`) and calls `virtual_tsc_mut().advance(delta)`.

**Alternative**: Model delay as ticks the VM is paused. Rejected because
the VM should still run between I/O calls — only the I/O itself is slow.

### 2. DiskFsyncLie: volatile overlay on top of CoW

Add a second `dirty` map (`volatile: BTreeMap<usize, Vec<u8>>`) to
`DeterministicBlock`. When `fsync_lie` is active:
- `write()` goes to `volatile` instead of `dirty`
- `read()` checks `volatile` first, then `dirty`, then `base`
- `DiskFsyncFlush` moves all `volatile` pages into `dirty`
- On `ProcessKill`, the controller calls `block.discard_volatile()` which
  clears the volatile map

The volatile map is included in `BlockSnapshot` so snapshot/restore
preserves the split. A new `DiskFsyncFlush` fault variant triggers the
commit.

**Alternative**: Track per-write sequence IDs and roll back on kill.
Rejected — page-level volatile buffer is simpler and matches the ext4
writeback model (pages are either in page cache or on disk, no partial).

### 3. DiskPartialRead: new BlockFault variant

Add `BlockFault::PartialRead { offset, max_bytes }`. On a matching read,
only fill `buf[..max_bytes]` and leave the rest zeroed. Return success
(not error) — this models a degraded device that returns short, not one
that fails. One-shot like existing BlockFault variants.

### 4. CpuBitflip: KVM get_regs/set_regs at tick boundary

The controller's `apply_fault()` calls `vm.get_vcpu_regs(vcpu)`, XORs
the target register's bit, then `vm.set_vcpu_regs(vcpu, regs)`. The
`GpRegister` enum (Rax..R15) maps to the `kvm_regs` struct field.
Bit >= 64 is a no-op. The DeterministicVm needs new methods to expose
per-vCPU register access.

**Alternative**: Write directly to a specific MSR. Rejected — GPRs are
the registers that hold application state (pointers, counters). MSR
corruption would mostly crash the kernel rather than corrupt application
logic.

### 5. CpuStall: per-vCPU stall counter in VmSlot

Add `vcpu_stall_until: BTreeMap<usize, u64>` to `VmSlot`. When a vCPU
is in the stall map and `tick < stall_until`, the scheduler skips it.
Cleanup: remove entries when `tick >= stall_until`. For single-vCPU VMs,
stalling vCPU 0 is equivalent to ProcessPause. Included in snapshot via
the controller's snapshot of VmSlot state.

### 6. ClockFreeze: frozen TSC value in VmSlot

Add `clock_freeze: Option<(u64, u64)>` (frozen_tsc, expires_at_tick) to
`VmSlot`. When active, `sync_tsc_to_guest()` writes `frozen_tsc` instead
of `virtual_tsc()`. On expiry, cleared automatically. Included in
controller snapshot.

### 7. ClockJitter: per-VM jitter bound in VmSlot

Add `clock_jitter_bound: u64` to `VmSlot` (default 0). When nonzero,
`sync_tsc_to_guest()` adds `rng.gen_range(-bound..=bound)` to the ideal
TSC. The jitter RNG is the VM's existing deterministic RNG (seeded,
snapshot-safe). Jitter is cosmetic — the underlying `VirtualTsc` advances
normally.

### 8. Serde compatibility

Use `#[serde(tag = "type")]` (already the pattern). New variants are
additive — old JSON without them deserializes fine. New JSON with unknown
variants on old code will error, which is the expected behavior (newer
format, older binary). No `#[serde(default)]` hacks needed.

### 9. FaultCategory::Cpu

New category. The `FaultCategory` enum gains a `Cpu` variant with
`Display` impl returning `"cpu"`. No changes to existing categories.

## Risks / Trade-offs

- **DiskFsyncLie complexity**: The volatile overlay adds a third read
  layer. Risk: subtle read ordering bugs where volatile data is stale
  relative to dirty pages. Mitigation: volatile writes always supersede
  dirty (most recent write wins at page granularity).

- **CpuBitflip may crash the kernel immediately**: Flipping a bit in RSP
  or RBP will likely cause an immediate fault/panic. This is by design
  (tests crash handling), but it means most bitflip faults won't produce
  subtle data corruption. Acceptable tradeoff — the point is testing
  resilience, not simulating realistic bit-error rates.

- **ClockJitter + ClockFreeze interaction**: If both are active on the
  same VM, freeze takes priority (jitter is meaningless when TSC is
  frozen). Document this precedence.

- **Random generation weight**: 7 new fault types out of 20 total may
  over-represent exotic faults vs common network/process faults. Mitigation:
  consider weighting in a future change, but uniform random is fine for
  exploration.
