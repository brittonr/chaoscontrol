## 1. Dlog record enrichment

- [x] 1.1 Add DlogTag variants: RegisterDump (20), MemoryHash (21), TickMarker (22)
- [x] 1.2 Pack RSP[31:0] + RFLAGS[31:0] into `extra` field for exit-type records (IoIn, IoOut, MmioRead, MmioWrite, Hlt, Hypercall) in vm.rs dlog emit sites
- [x] 1.3 Add `dlog_register_interval: u64` to VmConfig (default 0 = disabled); emit RegisterDump record every N exits with RIP/RSP/RAX/RBX/RCX/RDX/RSI/RDI/RFLAGS packed into data+extra
- [x] 1.4 Update DlogRecord Display impl for RegisterDump, MemoryHash, TickMarker tags
- [x] 1.5 Update DlogTag::from_u8 and name() for new variants
- [x] 1.6 Unit tests: round-trip encode/decode for new tags, display formatting, register interval emission

## 2. Memory page hashing

- [x] 2.1 Add `crc32fast` dependency to chaoscontrol-vmm Cargo.toml
- [x] 2.2 Add `dlog_memory_hash: bool` to VmConfig (default false)
- [x] 2.3 Implement `dlog_emit_memory_hashes(&mut self)` on DeterministicVm: hash coverage page (0xE0000), hypercall page (0xFE000), stack area (0x8000–0x9000), first 1 MB in 4 KB pages, CoW block dirty pages; emit one MemoryHash record per page with CRC32 in data and page-frame number in addr fields
- [x] 2.4 Call `dlog_emit_memory_hashes()` from the snapshot path (after dlog_snapshot_taken)
- [x] 2.5 Extend `dlog_diff` to report page-frame number and both CRC32 values when MemoryHash records diverge
- [x] 2.6 Unit tests: CRC32 hash of known data, MemoryHash record round-trip, diff detecting page divergence

## 3. Cross-VM tick markers

- [x] 3.1 Add `dlog_tick_marker(&mut self, tick: u64)` method on DeterministicVm
- [x] 3.2 Call `vm.dlog_tick_marker(self.tick)` from SimulationController before each VM's quantum in step_round()
- [x] 3.3 Unit test: TickMarker records carry correct tick values, appear in dlog output

## 4. Dlog CLI subcommand

- [x] 4.1 Add `Dlog` variant to the replay CLI's top-level clap enum with sub-subcommands: Dump, Diff, Stats
- [x] 4.2 Implement `dlog dump --file <path> [--from N] [--count N]` using dlog_dump()
- [x] 4.3 Implement `dlog diff --file-a <path> --file-b <path> [--strict]` using dlog_diff()
- [x] 4.4 Implement `dlog stats --file <path>`: iterate all records, count per-tag, print summary table
- [x] 4.5 Add `dlog_stats(path) -> BTreeMap<u8, u64>` helper to dlog.rs

## 5. DeterministicVm memory/register public API

- [x] 5.1 Add `read_guest_memory(&self, addr: u64, size: usize) -> Result<Vec<u8>, VmError>` to DeterministicVm
- [x] 5.2 Add `write_guest_memory(&self, addr: u64, data: &[u8]) -> Result<(), VmError>` to DeterministicVm
- [x] 5.3 Add `read_vcpu_registers(&self, vcpu: usize) -> Result<RegisterState, VmError>` converting kvm_regs + kvm_sregs into the replay crate's RegisterState
- [x] 5.4 Add `set_vcpu_registers(&mut self, vcpu: usize, regs: &RegisterState) -> Result<(), VmError>` applying RegisterState fields back to kvm_regs + kvm_sregs
- [x] 5.5 Unit tests: read/write round-trip on guest memory, register read returns populated struct

## 6. SimulationRunner trait extension

- [x] 6.1 Add `read_memory`, `write_memory`, `read_registers`, `set_registers` methods to SimulationRunner trait
- [x] 6.2 Implement the four methods on RealSimulationRunner delegating to controller.vm(index)
- [x] 6.3 Implement the four methods on MockRunner (return fixed data / record calls)
- [x] 6.4 Move RegisterState to chaoscontrol-vmm (or a shared location) so both vmm and replay can use it without circular deps

## 7. Debugger destructive analysis

- [x] 7.1 Wire `read_memory` in Debugger to call `runner.read_memory()` (replace stub)
- [x] 7.2 Wire `read_registers` in Debugger to call `runner.read_registers()` (replace stub); add `vcpu` parameter
- [x] 7.3 Add `poke_memory(&mut self, vm_index, addr, data)` to Debugger that calls `runner.write_memory()`
- [x] 7.4 Add `set_register(&mut self, vm_index, vcpu, reg, value)` to Debugger
- [x] 7.5 Define `Register` enum and `RegisterModification` struct in the replay crate
- [x] 7.6 Extend `counterfactual()` signature to accept `Vec<RegisterModification>` alongside memory mods
- [x] 7.7 Implement `replay_with_modification` to actually apply memory writes and register overrides before running ticks (replace the log::warn stub)

## 8. Integration and cleanup

- [x] 8.1 Wire `--dlog-dir <path>` through the explore CLI (already exists) and replay CLI
- [x] 8.2 Wire `--dlog-register-interval N` and `--dlog-memory-hash` through VmConfig in explore/replay CLIs
- [x] 8.3 cargo fmt, cargo clippy --workspace, cargo test --workspace
- [x] 8.4 Update napkin with new patterns and pitfalls discovered during implementation
