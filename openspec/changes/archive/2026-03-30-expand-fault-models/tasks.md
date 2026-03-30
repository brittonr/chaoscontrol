## 1. Fault Enum & Category

- [x] 1.1 Add `FaultCategory::Cpu` variant with Display impl returning `"cpu"`
- [x] 1.2 Add `GpRegister` enum (Rax, Rbx, Rcx, Rdx, Rsi, Rdi, Rbp, Rsp, R8–R15) with serde derives to `faults.rs`
- [x] 1.3 Add 7 new `Fault` variants: `DiskSlow`, `DiskFsyncLie`, `DiskFsyncFlush`, `DiskPartialRead`, `CpuBitflip`, `CpuStall`, `ClockFreeze`, `ClockJitter`
- [x] 1.4 Implement `target()` and `category()` for each new variant
- [x] 1.5 Add serde roundtrip tests for all new variants
- [x] 1.6 Add `FaultCategory::Cpu` classification test

## 2. Block Device: DiskSlow

- [x] 2.1 Add `slow_delay_ns: u64` field to `DeterministicBlock` (default 0)
- [x] 2.2 Return delay info from `read()`/`write()` (changed return type to `Result<u64, BlockError>`)
- [x] 2.3 Include `slow_delay_ns` in `BlockSnapshot` save/restore
- [x] 2.4 Add unit tests: slow read, slow write, clear slow, snapshot roundtrip

## 3. Block Device: DiskFsyncLie

- [x] 3.1 Add `volatile: BTreeMap<usize, Vec<u8>>` and `fsync_lie: bool` to `DeterministicBlock`
- [x] 3.2 Route writes to `volatile` map when `fsync_lie` is active
- [x] 3.3 Implement 3-tier read path: volatile → dirty → base
- [x] 3.4 Add `flush_volatile()` method (moves volatile pages into dirty)
- [x] 3.5 Add `discard_volatile()` method (clears volatile map)
- [x] 3.6 Include `volatile` map and `fsync_lie` flag in `BlockSnapshot`
- [x] 3.7 Add unit tests: write-to-volatile, read-through, kill-discards, flush-commits, snapshot roundtrip

## 4. Block Device: DiskPartialRead

- [x] 4.1 Add `BlockFault::PartialRead { offset, max_bytes }` variant
- [x] 4.2 Handle in `read()`: fill only `buf[..max_bytes]`, zero the rest, consume fault
- [x] 4.3 Add unit tests: short read, one-shot consumption, full read after fault consumed

## 5. VM Register Access for CpuBitflip

- [x] 5.1 Add `get_vcpu_regs(vcpu: usize) -> Result<kvm_regs>` to `DeterministicVm`
- [x] 5.2 Add `set_vcpu_regs(vcpu: usize, regs: &kvm_regs) -> Result<()>` to `DeterministicVm`
- [x] 5.3 Add `GpRegister` → `kvm_regs` field accessor (get/set by enum variant)

## 6. Controller Dispatch

- [x] 6.1 Add `vcpu_stall_until: BTreeMap<usize, u64>` to `VmSlot`
- [x] 6.2 Add `clock_freeze: Option<(u64, u64)>` (frozen_tsc, expires_at_tick) to `VmSlot`
- [x] 6.3 Add `clock_jitter_bound: u64` to `VmSlot`
- [x] 6.4 Handle `DiskSlow` in `apply_fault()`: set `slow_delay_ns` on block device
- [x] 6.5 Handle `DiskFsyncLie` in `apply_fault()`: enable `fsync_lie` on block device
- [x] 6.6 Handle `DiskFsyncFlush` in `apply_fault()`: call `flush_volatile()` on block device
- [x] 6.7 Handle `DiskPartialRead` in `apply_fault()`: inject `BlockFault::PartialRead` into block device
- [x] 6.8 Handle `CpuBitflip` in `apply_fault()`: read regs, XOR bit, write regs
- [x] 6.9 Handle `CpuStall` in `apply_fault()`: insert into `vcpu_stall_until`
- [x] 6.10 Handle `ClockFreeze` in `apply_fault()`: set `clock_freeze` on VmSlot
- [x] 6.11 Handle `ClockJitter` in `apply_fault()`: set `clock_jitter_bound` on VmSlot
- [x] 6.12 Wire `clock_freeze` check into TSC sync path (freeze takes priority over jitter)
- [x] 6.13 Wire `clock_jitter_bound` into TSC sync path
- [x] 6.14 Wire `vcpu_stall_until` check into scheduler loop (skip stalled vCPUs)
- [x] 6.15 Include new VmSlot fields in controller snapshot/restore
- [x] 6.16 Handle `DiskFsyncLie` + `ProcessKill` interaction: call `discard_volatile()` on kill

## 7. Random Generation

- [x] 7.1 Add new fault types to `FaultEngine::generate_random_fault()` (13 → 20 types)
- [x] 7.2 Add new fault types to `ScheduleMutator::random_fault()` (15 → 22 types)
- [x] 7.3 Add test: 1000 random faults cover all new types

## 8. Tests

- [x] 8.1 Unit tests for all new `Fault` variant construction, target, category
- [x] 8.2 Property test (Hegel): random fault generation always produces valid variants
- [x] 8.3 Serde backward compat test: deserialize old checkpoint JSON without new variants
- [x] 8.4 cargo clippy + cargo fmt clean
