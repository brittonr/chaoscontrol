## 1. Record Format and Types

- [x] 1.1 Define `DlogTag` repr(u8) enum with all exit/event variants
- [x] 1.2 Define `DlogRecord` repr(C) 64-byte struct with seq, tsc, exit_count, rip, tag, vcpu, port_or_addr, data, extra fields
- [x] 1.3 Add `DlogRecord::new()` constructor and `Display` impl for human-readable output
- [x] 1.4 Unit tests: record size is 64 bytes, round-trip encode/decode, Display formatting

## 2. Writer

- [x] 2.1 Implement `DlogWriter` with `BufWriter<File>` (64KB buffer) and monotonic seq counter
- [x] 2.2 `DlogWriter::emit(&mut self, record: DlogRecord)` writes raw bytes
- [x] 2.3 `DlogWriter::flush()` and `Drop` impl for crash safety
- [x] 2.4 Unit tests: write N records, read back from file, verify contents

## 3. Reader and Diff

- [x] 3.1 Implement `DlogReader` that opens a file and iterates `DlogRecord`s via buffered reads (skip memmap2 — use `BufReader` + `read_exact` for simplicity)
- [x] 3.2 `dlog_diff(a: &Path, b: &Path, strict: bool) -> DiffResult` — sequential compare, returns first divergence with 5-record context window
- [x] 3.3 `dlog_dump(path: &Path, from: u64, count: u64)` — text dump to stdout
- [x] 3.4 Unit tests: diff identical files → Ok, diff divergent files → correct offset, dump formatting

## 4. VMM Integration

- [x] 4.1 Add `dlog: Option<DlogWriter>` field to `DeterministicVm`
- [x] 4.2 Add `dlog_path: Option<PathBuf>` to `VmConfig`
- [x] 4.3 `#[inline] fn dlog_emit()` helper on `DeterministicVm`
- [x] 4.4 Emit `DlogTag::IoIn` / `IoOut` in the IoIn/IoOut match arms of `step()`
- [x] 4.5 Emit `DlogTag::MmioRead` / `MmioWrite` in the MMIO match arms
- [x] 4.6 Emit `DlogTag::Hlt`, `Shutdown`, `Intr`, `Debug`, `IrqWindowOpen`, `InternalError`
- [x] 4.7 Emit `DlogTag::Hypercall` / `SdkHypercall` in the hypercall/SDK paths
- [x] 4.8 Emit `DlogTag::SchedulerSwitch` in `maybe_switch_vcpu()`
- [x] 4.9 Emit `DlogTag::SnapshotTaken` / `SnapshotRestored` in snapshot/restore methods
- [x] 4.10 Emit `DlogTag::FaultApplied` / `InterruptInjected` / `NmiInjected` in fault dispatch
- [x] 4.11 Flush dlog on snapshot and on `Drop`

## 5. Controller and Explorer Wiring

- [x] 5.1 Add `dlog_dir: Option<PathBuf>` to `SimulationConfig`
- [x] 5.2 `SimulationController::new()` creates `<dir>/vm_N.dlog` per VM when dlog_dir is set
- [x] 5.3 Add `--dlog <dir>` flag to `chaoscontrol-explore` CLI
- [x] 5.4 Wire through `ExplorerConfig` → `SimulationConfig` → `VmConfig`

## 6. Replay CLI Subcommands

- [x] 6.1 `chaoscontrol-replay dlog-diff <a.dlog> <b.dlog> [--strict]` subcommand
- [x] 6.2 `chaoscontrol-replay dlog-dump <file.dlog> [--from N] [--count M]` subcommand

## 7. Integration Test

- [ ] 7.1 Integration test: run same seed twice with dlog enabled, diff the two logs, assert identical (requires /dev/kvm + kernel images)
- [ ] 7.2 Integration test: run two different seeds with dlog, diff, assert divergence found (requires /dev/kvm + kernel images)
