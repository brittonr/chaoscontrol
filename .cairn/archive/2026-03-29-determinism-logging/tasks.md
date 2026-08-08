## 1. chaoscontrol-dlog Crate Setup

- [ ] 1.1 Create `crates/chaoscontrol-dlog` with Cargo.toml (deps: bytemuck, memmap2, crossbeam-utils)
- [ ] 1.2 Define DlogHeader struct (magic, version, event_size, vm_id, config_hash) with repr(C)
- [ ] 1.3 Define DlogEvent enum with fixed-size repr(C) variants: ExitEvent, RngDraw, FaultDispatch, SdkCall, SchedulerDecision
- [ ] 1.4 Implement bytemuck Pod/Zeroable for zero-copy serialization
- [ ] 1.5 Unit tests for round-trip encode/decode of all event types

## 2. Ring Buffer and Writer

- [ ] 2.1 Implement RingBuffer<T: Pod> with configurable capacity (default 64K entries)
- [ ] 2.2 Implement DlogWriter: push events to ring buffer, flush to mmap'd file when 75% full
- [ ] 2.3 Implement Drop for DlogWriter that flushes remaining events
- [ ] 2.4 Add DlogWriter::new(path, vm_id) constructor that writes header and pre-allocates file

## 3. VMM Integration

- [ ] 3.1 Add `paranoid_log: Option<PathBuf>` to VmConfig with serde default
- [ ] 3.2 Add `--paranoid-log <dir>` to chaoscontrol-explore CLI
- [ ] 3.3 Create DlogWriter per VM in DeterministicVm::new when paranoid_log is set
- [ ] 3.4 Log ExitEvent in run_bounded() after each VM exit (exit_type, tsc, exit_count)
- [ ] 3.5 Log SdkCall in handle_sdk_hypercall() (cmd_id, payload FNV hash)
- [ ] 3.6 Log SchedulerDecision in VcpuScheduler::tick() and liveness switch

## 4. Fault and RNG Integration

- [ ] 4.1 Log RngDraw in FaultEngine::random_fault() with domain tag
- [ ] 4.2 Log FaultDispatch in SimulationController::dispatch_faults() with fault type and tick
- [ ] 4.3 Log RngDraw in NetworkFabric for loss/corruption/reorder decisions

## 5. Log Reader and Diff Tool

- [ ] 5.1 Implement DlogReader: mmap file, iterate events via slice cast
- [ ] 5.2 Implement dlog_diff(a: &Path, b: &Path) → DiffResult with first divergence index and context window
- [ ] 5.3 Add `chaoscontrol-replay diff <log-a> <log-b>` subcommand that prints divergence context

## 6. Testing

- [ ] 6.1 Unit test: two identical single-VM runs produce identical logs (byte-equal)
- [ ] 6.2 Unit test: different seeds produce logs that diverge at first RNG draw
- [ ] 6.3 Unit test: diff tool finds correct divergence index with 10-event context
- [ ] 6.4 Integration test: paranoid log round-trips through write → read → verify
- [ ] 6.5 Run `cargo clippy --workspace` clean
