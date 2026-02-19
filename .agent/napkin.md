# Napkin

## Corrections
| Date | Source | What Went Wrong | What To Do Instead |
|------|--------|----------------|-------------------|
| 2026-02-17 | self | Tried `no-kvmclock` kernel param — not a real param | Use KVM visibility control + clocksource=tsc instead |
| 2026-02-17 | self | AMD CPUs don't have CPUID leaf 0x15 | Must INJECT leaf 0x15 into CPUID entries if missing |
| 2026-02-17 | self | Tried removing create_pit2() entirely | KVM PIT port 0x61 + IRQ routing needed; keep KVM PIT, suppress timer |
| 2026-02-17 | self | DeterministicPit never received guest PIT writes | KVM PIT intercepts port 0x40-0x43; mirror via get_pit2() |
| 2026-02-17 | self | Thought non-determinism was from PIT | bpftrace proved it's from variable VM exit counts inflating virtual TSC |
| 2026-02-17 | self | Tried full userspace PIT (no create_pit2) with worker | PIT ch0 gets too few timer IRQs — virtual TSC advance too slow for calibration |
| 2026-02-17 | self | Used string literals in bpftrace map keys | BPF verifier rejects string comparisons in map keys; use integer rw field directly |
| 2026-02-17 | self | Used args->irq for kvm_pic_set_irq | Field is called args->pin, not args->irq |
| 2026-02-17 | self | Used args->irq for kvm_set_irq | Field is called args->gsi, not args->irq |
| 2026-02-18 | self | VmSnapshot didn't save/restore VirtualTsc | Must save virtual_tsc, exit_count, io_exit_count in snapshot |
| 2026-02-18 | self | Faults didn't fire in integration tests | poll_faults() gated by setup_complete — call force_setup_complete() |
| 2026-02-18 | self | Used bzImage for kernel loading | ELF loader needs vmlinux (result-dev/vmlinux), not bzImage |
| 2026-02-18 | self | cargo fmt applied to worktrees but not main repo | Always fmt main repo first, then apply feature changes cleanly |
| 2026-02-18 | self | New files in worktrees aren't in `git diff` | Copy untracked files from worktrees separately — `git diff` only shows tracked files |
| 2026-02-18 | self | TestCluster::step() had borrow conflict: `&mut self.nodes[i]` + `self.rand()` | Move `self.rand()` call outside the `let node = &mut self.nodes[active]` borrow scope |
| 2026-02-19 | self | delegate_task workers made changes in isolated worktrees, not the main repo | Workers run in isolation — must apply changes directly via edit/write in main session |
| 2026-02-19 | self | libc::timespec fields are already i64 on x86_64 Linux | Don't cast `ts.tv_sec as i64` — clippy flags unnecessary_cast |
| 2026-02-19 | self | SMP: AP vCPU stuck in SIPI-wait, scheduler tried to run it | Check KVM_GET_MP_STATE before running each vCPU; skip non-runnable APs |
| 2026-02-19 | self | SMP: BSP spin-waits for AP with no VM exits | Tight loops don't generate KVM exits; use SIGALRM timer for preemption |
| 2026-02-19 | self | SIGALRM with SA_RESTART doesn't interrupt vcpu.run() | Must NOT set SA_RESTART; also handle Err(EINTR) in addition to VcpuExit::Intr |
| 2026-02-19 | self | ChaCha20Rng::get_seed() returns initial seed, not current state | Must save get_seed() + get_word_pos() together; restore with from_seed() + set_word_pos() |
| 2026-02-19 | self | SMP: vcpu_is_runnable() only checked RUNNABLE (0) | Must also accept HALTED (3) — AP halts during init, needs timer IRQ to continue |
| 2026-02-19 | self | SMP: scheduler.active() at top of step() overrode EINTR switch | Remove that line — let EINTR handler's active_vcpu assignment persist |
| 2026-02-19 | self | SMP: only armed timer when multiple vCPUs runnable | Must ALWAYS arm timer in SMP mode — AP becomes runnable asynchronously via SIPI |
| 2026-02-19 | self | SMP: EINTR called scheduler.advance() → non-deterministic scheduling | Use scheduler.set_active() or simple round-robin — don't consume RNG on wall-clock events |
| 2026-02-19 | self | SMP: SA_RESTART makes vcpu.run() restart after SIGALRM | Must NOT set SA_RESTART — need SIGALRM to interrupt the ioctl |
| 2026-02-19 | self | SMP: SIGALRM wall-clock timer causes non-deterministic exit counts | Replace with perf_event_open instruction counter (PMU overflow mode) |
| 2026-02-19 | self | SMP: TSC calibration varies because RDTSC sees hardware drift | Use clocksource=jiffies notsc for SMP; tsc=reliable not enough (calibration still runs) |
| 2026-02-19 | self | SMP: sync_tsc_to_guest on EINTR re-entry resets TSC non-deterministically | Add skip_tsc_sync flag; skip PIT sync + TSC write after signal returns |
| 2026-02-19 | self | PMU overflow SIGIO has 1-5 instruction skid (hardware limitation) | Accept ±4 exit variance or investigate PEBS zero-skid mode |
| 2026-02-19 | self | PMU overflow SIGIO never delivered on AMD Zen5 | SIGIO signals are 0 even with perf_event_paranoid=-1; use SIGALRM-based approach instead |
| 2026-02-19 | self | PMU instruction counts not deterministic | PIT calibration loop iterations vary 41.8M vs 42.3M between runs; exit-count scheduling is inherently more deterministic |
| 2026-02-19 | self | SIGALRM 5ms timer non-deterministic in integration tests | After 23 prior test VMs, process state differs enough to cause ±1 exit count; use 10ms |
| 2026-02-19 | self | hide_tsc via CPUID EDX bit 4 breaks APIC timer | Kernel panics "IO-APIC + timer doesn't work!"; use `notsc` cmdline instead |
| 2026-02-19 | self | PIT channel 2 frozen via count_load_time=far_future | Makes elapsed clamp to 0, counter always reads initial reload — forces CPUID 0x15 fallback |
| 2026-02-19 | self | CPUID 0x15 injection (25MHz × 120 = 3GHz) works on AMD | Leaf doesn't exist natively; combined with vendor=GenuineIntel makes kernel trust it |
| 2026-02-19 | self | Liveness switch must be invisible to scheduler | If SIGALRM calls scheduler.tick() or set_active(), it creates non-deterministic scheduling |
| 2026-02-19 | self | Spin-loop detection needs threshold ≥2 consecutive SIGALRMs | Threshold=1 triggers during PIT calibration (real exits happen between SIGALRMs) |
| 2026-02-19 | self | SMP snapshot/restore exit counts differ by ±2 | SIGALRM timer phase is non-deterministic after restore; test REPRODUCIBILITY (restore twice → same result) not EQUIVALENCE (original path == restored path) |
| 2026-02-19 | self | SIGALRM from previous SMP VM leaks into next VM's vcpu.run() | Must disarm timer in run_bounded on exit + Drop impl; stale SIGALRMs cause ±2 exit count jitter |
| 2026-02-19 | self | SMP VMs hit InternalError after snapshot/restore | VcpuSnapshot must save/restore KVM_MP_STATE — without it, KVM doesn't know AP is HALTED vs UNINITIALIZED |
| 2026-02-19 | self | VcpuSnapshot only saved regs/sregs/fpu/lapic/xcrs | Must also save mp_state; set mp_state BEFORE registers during restore (KVM rejects writes on UNINITIALIZED vCPUs) |
| 2026-02-19 | self | CPUID leaf 0xB EDX=0 for all vCPUs → "APIC ID mismatch" firmware bug | filter_cpuid makes shared template; must patch_cpuid_apic_id per-vCPU with unique APIC ID before set_cpuid2 |

## User Preferences
- Building a deterministic hypervisor (ChaosControl)
- Uses Rust + KVM via rust-vmm crates
- Nix flake for dev environment (must use `nix develop --command bash -c "..."`)
- NixOS host (sudoers via security.sudo.extraRules, not /etc/sudoers.d/)
- bpftrace NOPASSWD via NixOS config

## Patterns That Work
- **VirtioBackend downcasting**: `as_any()` / `as_any_mut()` on trait → `downcast_ref`/`downcast_mut` for device-specific operations (snapshot + fault injection)
- **CaptureParams struct**: group snapshot params to avoid too-many-arguments clippy lint
- **PIT ch2 virtual time sync**: set `count_load_time = host_now - virtual_elapsed_ns` before vcpu.run() to make KVM PIT return deterministic values
- **VcpuExit::Intr passthrough**: don't tick virtual_tsc on host signal interrupts — just retry vcpu.run()
- **Checkpoint resume**: Save global coverage + progress counters, skip VM snapshots (re-bootstrap on resume), carry forward coverage to avoid re-exploration
- vm_superio::Serial with EventFd + register_irqfd for interrupt-driven serial
- CapturingWriter pattern: write to stdout + capture in Arc<Mutex<Vec<u8>>>
- VirtualTsc advancing on every VM exit for deterministic time progression
- CPUID 0x15 injection for deterministic TSC calibration on AMD hosts
- hide_hypervisor=true to prevent kvm-clock
- **Hybrid PIT**: keep KVM PIT for I/O + IRQ routing, suppress timer via count_load_time=far_future,
  mirror state to DeterministicPit, deliver IRQ 0 via set_irq_line
- **HLT fast-forward**: on HLT, read KVM PIT reload, advance virtual TSC, inject IRQ 0
- **bpftrace for KVM debugging**: tracepoints kvm_exit, kvm_pio, kvm_pic_set_irq (pin), kvm_set_irq (gsi), kvm_inj_virq (vector)
- Self-terminating bpftrace: `interval:s:N { exit(); }` since we can't sudo kill
- sudo NOPASSWD for bpftrace only: `security.sudo.extraRules` with full nix store path

## Patterns That Don't Work
- Removing create_pit2() entirely — kernel hangs at serial driver init (too few timer IRQs)
- Fixed TSC advance per exit — variable exit counts (host INTRs, serial polling) cause ~2ms virtual time drift
- String keys in bpftrace maps — BPF verifier rejects them

## Key bpftrace Findings (2026-02-17)
- **KVM PIT suppression works**: kvm_pic_pin0 == set_irq_gsi0 (all IRQ 0 from our set_irq_line)
- **Exit count varies ±6,700 between runs** (166,777 vs 173,475) — mostly I/O exits (serial polling)
- **6,700 × 1000 TSC/exit = 6.7M TSC = 2.2ms virtual time drift** — matches observed jitter
- **16,366 port 0x42 reads** during PIT calibration — KVM PIT handles these with host time
- **Root cause of non-determinism**: variable VM exit counts × fixed TSC-per-exit = variable virtual TSC

## Determinism Status
- **FIXED (2026-02-18)**: sync_tsc_to_guest() writes virtual TSC to IA32_TSC before every vcpu.run()
- Previously: 321/324 deterministic. TSC calibration, sched_clock, audit timestamp drifted ~2ms
- Root cause was variable VM exit counts × fixed TSC advance. Fix: overwrite KVM's real-time TSC with our deterministic value before each entry
- **FIXED (2026-02-19)**: VcpuExit::Intr handled — host signals no longer cause spurious exits/ticks
- **FIXED (2026-02-19)**: PIT channel 2 count_load_time synced to virtual time — deterministic TSC calibration
- **FIXED (2026-02-19)**: DeterministicPit state saved/restored in VmSnapshot
- **FIXED (2026-02-19)**: FaultEngine state saved/restored in per-VM VmSnapshot
- **FIXED (2026-02-19)**: Virtio block device data saved/restored in VmSnapshot
- **FIXED (2026-02-19)**: coverage_active flag saved/restored in VmSnapshot
- **FIXED (2026-02-19)**: HashMap → BTreeMap in trace crate for deterministic iteration

## eBPF Trace Harness (2026-02-17)
- **chaoscontrol-trace crate**: libbpf-rs 0.26 + libbpf-cargo 0.26 skeleton approach
- **BPF program**: 11 KVM tracepoints (exit, entry, pio, mmio, msr, inj_virq, pic_set_irq, set_irq, page_fault, cr, cpuid)
- **NixOS**: Must use unwrapped clang for BPF target (CLANG env var in flake.nix)
- **Struct naming**: vmlinux.h defines `struct trace_event` and `struct trace_entry` — our event struct must use a different name (`cc_trace_event`)
- **libbpf-rs 0.26 API**: `SkelBuilder::open()` takes `&mut MaybeUninit<OpenObject>`; need `Box::leak` for lifetime; traits `SkelBuilder`, `OpenSkel`, `Skel`, `MapCore` must be imported explicitly
- **kvm_exit not captured**: BPF tracepoint context struct for kvm_exit may have alignment issues with `trace_entry` from vmlinux.h — kvm_entry/pio/page_fault/cpuid/msr/cr/mmio all work
- **Event counts confirm napkin findings**: kvm_entry varies ±1000-2000, kvm_pio varies ±2000-5000 between runs; cpuid/cr/msr/mmio perfectly deterministic
- **SIGTERM handling critical**: collector must handle SIGTERM for graceful save on `kill`
- **sudo NOPASSWD needed**: for both bpftrace and chaoscontrol-trace binary

## Verus Testing (2026-02-17)
- Extracted pure functions into `src/verified/` modules in both crates
- Created Verus spec files in `verus/` directories
- Pattern: pure function in verified/, imperative shell delegates to it
- Modules covered: cpu, memory, pit, block, entropy, net, events, verifier
- Tiger Style: every verified function has debug_assert! preconditions and postconditions
- chaoscontrol-vmm verified modules: cpu (TSC advance), memory (region overlap), pit (reload/latch), block (offset clamp), entropy (seed expansion), net (MAC validation)
- chaoscontrol-trace verified modules: events (determinism_eq), verifier (divergence detection)
- All verified functions are pure (no I/O, no side effects), deterministic, and testable

## Multi-vCPU / SMP (2026-02-19)
- **Architecture**: Antithesis-style serialized execution — only one vCPU runs at a time
- **VcpuScheduler**: deterministic round-robin + randomized strategy, seeded ChaCha20
- **ACPI MADT**: minimal RSDP/RSDT/MADT at 0xF0000, EBDA pointer at 0x40E
- **Linux detects 2 CPUs**: ACPI MADT parsed, topology shows "Allowing 2 present CPUs"
- **AP boot sequence**: BSP sends INIT IPI + SIPI via LAPIC ICR (handled by KVM internally)
- **Preemption timer**: ITIMER_REAL (1ms) sends SIGALRM → Err(EINTR) from vcpu.run()
- **MP state check**: skip non-runnable APs (KVM_MP_STATE_RUNNABLE = 0)
- **Cmdline**: `nosmp noapic` for 1 vCPU, `maxcpus=N` for SMP
- **Snapshot**: VmSnapshot now stores Vec<VcpuSnapshot> + SchedulerSnapshot
- **Status**: ✅ **WORKING** — "Brought up 1 node, 2 CPUs" at ~70K exits in ~2s
- **KVM_MP_STATE_HALTED**: must treat HALTED (3) as schedulable, not just RUNNABLE (0)
- **SIGALRM preemption**: wall-clock timer (500µs) breaks tight spin loops during SMP boot
  - EINTR handler does simple round-robin switch, NOT deterministic scheduler
  - Does NOT advance exit_count or virtual_tsc — invisible to deterministic state
  - scheduler.set_active() syncs scheduler state without consuming RNG
- **PMU instruction counting**: perf_event_open(INSTRUCTIONS, exclude_host=1, overflow mode)
  - SIGIO fires after N guest instructions → replaces SIGALRM for SMP preemption
  - SIGALRM fallback when PMU unavailable (CI/containers)
  - skip_tsc_sync flag prevents non-deterministic TSC resets on signal re-entry
  - SMP cmdline: clocksource=jiffies notsc (avoids non-deterministic TSC calibration)
- **Determinism status**:
  - Single-vCPU: ✅ PERFECTLY DETERMINISTIC (70000 × 3 runs identical)
  - SMP 2-vCPU: ✅ PERFECTLY DETERMINISTIC (69905 × 5 runs identical @ 5ms, 69930 × 5 @ 50ms)
  - PMU instruction counting abandoned — SIGIO never delivered on AMD Zen5, counts non-deterministic
  - Exit-count scheduling + SIGALRM liveness is the winning approach
- **Integration tests**: 24/24 pass
- **CPUID per-vCPU**: filter_cpuid() creates shared template → patch_cpuid_apic_id() per-vCPU for leaf 0x1 EBX[31:24] + leaf 0xB/0x1F EDX
- **Key architecture**:
  1. Exit-count scheduler (quantum=100 round-robin, seeded RNG for randomized)
  2. SIGALRM (10ms) fires during spin loops, detected by `sigalrm_without_exit >= 2`
  3. Liveness switch changes `active_vcpu` only (invisible to scheduler — no tick/set_active)
  4. `skip_tsc_sync = true` after SIGALRM prevents PIT/TSC disruption
  5. `maybe_switch_vcpu()` guards: only ticks scheduler when `active_vcpu == scheduler.active()`
  6. PIT channel 2 frozen (count_load_time = i64::MAX / 2), CPUID 0x15 provides 3GHz
  7. Serial verified with `strip_nondeterministic()` (strips timestamps, Memory line, TSC MHz)

## Completed (2026-02-18 session)
1. ✅ Fix virtual TSC: sync_tsc_to_guest() writes virtual TSC to IA32_TSC MSR before every vcpu.run()
2. ✅ Multi-VM SimulationController (round-robin scheduling, NetworkFabric, fault dispatch)
3. ✅ Virtio MMIO transport (virtio 1.2, legacy-free) + virtio-blk, virtio-net, virtio-rng backends
4. ✅ chaoscontrol-explore crate: fork-from-snapshot, coverage-guided search, AFL-style bitmaps, frontier, mutator
5. ✅ chaoscontrol-replay crate: recording, checkpoint, replay, time-travel debugger, triage, serialize

## Remaining Work
(All items completed)

## Network Simulation Fidelity (2026-02-19)
- **NetworkJitter fault**: per-VM random latency variation (0 to jitter_ns extra delay per packet)
- **NetworkBandwidth fault**: per-VM throughput cap with serialization delay queuing model
  - Token-bucket style: tracks `next_free_tick` per VM, back-to-back packets queue naturally
  - `delay_ticks = packet_bits * 1000 / effective_bps` (1 tick = 1ms)
  - Bottleneck: `min(sender_bps, receiver_bps)` when both are set
- **PacketDuplicate fault**: per-VM duplication rate (PPM), duplicate arrives with 0-2 ticks offset
- All three are bidirectional (max of sender/receiver rate), consistent with existing loss/corruption model
- `NetworkHeal` resets jitter, bandwidth, next_free_tick, and duplication (same as loss/corruption/reorder)
- Updated: `FaultEngine::random_fault()` (11 types), `Mutator::random_fault()` (13 types), checkpoint serialization
- New `send()` pipeline: partition → loss → bandwidth → corruption → latency+jitter → reorder → duplication
- 31 new tests (616 total, 0 failures)
- Existing `latency` field stores ticks but comments said "nanoseconds" — naming inconsistency preserved for now

## Network Observability (2026-02-19)
- **NetworkStats struct**: cumulative packet-level counters in NetworkFabric
  - packets_sent, packets_delivered, packets_dropped_partition, packets_dropped_loss
  - packets_corrupted, packets_duplicated, packets_bandwidth_delayed (+ total ticks)
  - packets_jittered (+ total ticks), packets_reordered
- **Wired into**: SimulationResult, ExplorationReport, format_report()
- **Display impl**: one-liner for log output
- **NOT reset on NetworkHeal**: stats are cumulative across entire simulation
- **Exploration report**: new "Network Fabric Statistics" section shows non-zero counters + averages

## Entropy & Seeding Determinism Tests (2026-02-19)
- **Gap found**: Test 18 verified network config survives snapshot/restore but NOT that the RNG state was identical
- **5 new unit tests**: clone-preserves-RNG, seed-changes-all-domains, stats-deterministic, domain-separator-isolation, snapshot-restore-RNG-decisions
- **3 new integration tests (20-22)**:
  - Test 20: snapshot/restore + 80 packet sends → identical delivery ticks, data, loss counts
  - Test 21: seed propagation — same seed=same traffic, different seed=different traffic
  - Test 22: all 10 NetworkStats counters match between identical full runs
- **Seed propagation chain**: `config.seed` → FaultEngine (`seed`), per-VM CPU (`seed + i`), NetworkFabric (`seed + 0x4E455446414E` domain separator)
- **Key insight**: NetworkFabric is `Clone` → snapshot = clone → RNG state preserved by ChaCha20's `get_seed()` (stored in the struct fields, not external state)
- 630 unit tests, 22/22 integration tests pass

## Completed (2026-02-19 SMP end-to-end)
26. ✅ SMP snapshot/restore integration test (Test 25) — two restores produce identical execution
27. ✅ num_vcpus + SchedulingStrategy wired through VmConfig → SimulationConfig → ExplorerConfig → CLI
28. ✅ CLI: --vcpus and --scheduling flags for chaoscontrol-explore
29. ✅ VcpuSnapshot saves/restores KVM_MP_STATE (critical for SMP restore)
30. ✅ SIGALRM timer properly disarmed on run_bounded exit + VM Drop
31. ✅ VcpuExit::InternalError handled gracefully in SMP (switch to next runnable vCPU)
32. ✅ SMP Raft exploration: 3 VMs × 2 vCPUs, 3 rounds × 4 branches completed
33. ✅ 25/25 integration tests pass, 634 unit tests pass

## Completed (2026-02-18 loose ends)
18. ✅ DiskTornWrite + DiskCorruption fault handlers wired to DeterministicBlock
19. ✅ Explore `resume` subcommand with JSON checkpoint save/load
20. ✅ cargo fmt pass across workspace
21. ✅ 523 tests passing, 0 failures

## CLI Binaries (2026-02-18)
- **chaoscontrol-explore**: `run` subcommand (ExplorerConfig from CLI args, progress via env_logger), `resume` placeholder
- **chaoscontrol-replay**: 5 subcommands: `replay`, `triage`, `info`, `events`, `debug`
- **Pattern**: clap derive + env_logger, matching chaoscontrol-trace style
- **Delegates made worktree changes**: Must copy from /tmp/pi-worktrees/ back to main repo
- **Virtio MMIO already wired**: MmioRead/MmioWrite in step(), process_queues() + IRQ raise, kernel cmdline has virtio_mmio.device= params

## SDK Guest Program (2026-02-18)
- **chaoscontrol-guest crate**: minimal SDK-instrumented guest binary, runs as PID 1 in VM
- **musl static linking**: `x86_64-unknown-linux-musl` target, binary is ~560KB, fully static
- **CARGO_TARGET_DIR**: nix sets `CARGO_TARGET_DIR=$HOME/.cargo-target`; build scripts must use it
- **devtmpfs**: guest mounts devtmpfs on /dev for `/dev/mem` + `/dev/port` (SDK std transport)
- **`file` output**: musl binary says "static-pie linked" not "statically linked"; use `ldd` to verify
- **flake.nix**: added `pkgs.pkgsCross.musl64.stdenv.cc` + musl target to rust-overlay
- **`.cargo/config.toml`**: sets `linker = "x86_64-unknown-linux-musl-gcc"` for musl target
- **7/7 SDK guest tests pass**: boot, setup_complete, oracle assertions, always verdicts, coverage bitmap, lifecycle events, determinism
- **100 coverage edges**: guest records ~100 non-zero bitmap entries across 50 iterations × 4 choices

## BPF Tracepoint Fix (2026-02-18)
- **Root cause**: kvm_exit struct had implicit padding between u32 exit_reason and u64 guest_rip. Kernel format (verified via /sys/kernel/tracing/events/kvm/kvm_exit/format) has guest_rip at offset 16, isa at 24, etc. — compiler was inserting correct padding but the struct didn't have explicit __u32 _pad fields
- **Fix**: Added explicit __u32 _pad0/1/2 fields to tp_kvm_exit to make alignment visible and prevent compiler differences
- **kvm_inj_virq**: Changed `bool` → `__u8` for soft/reinjected (bool fragile in BPF)
- **kvm_set_irq**: Now captures irq_source_id in arg2 (was dropped)
- **Pattern**: Always verify BPF tracepoint context structs against `/sys/kernel/tracing/events/<subsys>/<event>/format`

## Completed (2026-02-18 continued)
6. ✅ SDK coverage instrumentation — AFL-style 64KB bitmap at 0xE0000, SanCov hooks, no_std + std transport
7. ✅ End-to-end integration test — 12 tests: boot, determinism, snapshot/restore, coverage bitmap, multi-VM, fault injection (ProcessKill, NetworkPartition, ClockSkew), controller determinism
8. ✅ Fault engine wired to real VMs — ProcessKill, ClockSkew, NetworkPartition confirmed working via integration tests
9. ✅ VmSnapshot now saves/restores VirtualTsc + exit_count + io_exit_count (was missing, caused snapshot/restore vTSC mismatch)
10. ✅ FaultEngine::force_setup_complete() for integration tests where guest doesn't use SDK

## Raft Test Expansion (2026-02-18)
- **Restructured**: Extracted pure Raft logic into `src/lib.rs` (no SDK deps), `main.rs` is thin SDK wrapper
- **Cargo.toml**: Added `[lib]` + `[[bin]]` sections so lib tests work without VM
- **Pattern**: SDK calls (coverage/random/assert) are injected via parameters (jitter: usize) instead of called directly
- **TestCluster**: Deterministic cluster runner using LCG for randomness, drives full simulation loop
- **78 tests** across 15 categories: node construction, follower/candidate transitions, RequestVote, AppendEntries, commit quorum, heartbeats, safety checks, full scenarios, determinism, coverage gaps
- **100% line coverage** (244/244 lines) verified via cargo-tarpaulin
- **Borrow fix**: Leader propose logic must be outside `let node = &mut self.nodes[active]` scope
- **Coverage gaps found**: leader log content mismatch (line 444), `leaders()` method (601-605), `run_checked` panic paths (587) — all covered by Category O tests

## Pi Skill (2026-02-18)
- Created `chaoscontrol` skill in agentkit at `_global/skills/chaoscontrol/SKILL.md`
- Symlinked to `~/.pi/agent/skills/chaoscontrol`
- Covers: workspace layout, build commands, CLI tools, key APIs (SDK, VMM, controller, fault, explore, replay), architecture notes, guest program patterns, common pitfalls, testing patterns
- Updated agentkit README.md to include it

## Completed (2026-02-18 cleanup)
11. ✅ Packet-level faults: PacketLoss, PacketCorruption, PacketReorder implemented in NetworkFabric
12. ✅ ProcessPause auto-resume: VmStatus::Resuming variant, schedule_resume() method
13. ✅ MemoryPressure stored in VmSlot.memory_limit_bytes
14. ✅ Explorer tick tracking: BranchResult.total_ticks used in BugReport.tick
15. ✅ README roadmap updated — all 17 items checked
16. ✅ Zero TODOs remaining in codebase
17. ✅ 503 tests passing, 0 failures

## Dogfooding: Raft Guest (2026-02-18)
- **chaoscontrol-raft-guest crate**: 3-node in-process Raft with SDK assertions
- Safety: election safety (≤1 leader/term), log matching, leader completeness
- Liveness: leader elected, values committed, 3+ committed
- 240 coverage edges, fully deterministic, all safety assertions pass
- **End-to-end exploration works**: 2 rounds × 4 branches completed, no bugs (correct)

## Dogfooding Findings (2026-02-18)
| Finding | Impact | Fix |
|---------|--------|-----|
| Kernel never HLTs after workload — busy serial polls | Idle detection based on HLT doesn't work | **Fixed**: `exits_since_last_sdk` counter |
| Kernel idle loop = serial I/O polling, not HLT | All 50 post-workload exits are IoIn | Counter must track total exits, not HLT exits |
| `run_bounded` had no idle detection | VM spins forever after workload | **Fixed**: SDK_IDLE_THRESHOLD=500 in run_bounded |
| Explore creates new SimulationController per branch | 5s kernel boot per branch | **Fixed**: controller cached in Explorer, reused via restore_all |
| Coverage bitmap shows 0 edges in exploration | Guest run-to-completion during boot, exploration ticks idle | **Fixed**: guest changed to infinite loop; controller.run() is relative ticks |
| `controller.run(max_ticks)` was absolute, not relative | After restore to tick=5, `run(5)` exits immediately | **Fixed**: changed to `run(num_ticks)` = relative duration |
| Guest "workload complete" model incompatible with exploration | Guest runs 200 ticks then idles forever | **Fixed**: infinite loop, no completion, VMM controls horizon |

## Coverage Instrumentation (2026-02-18)
- **Coverage bitmap**: 64KB at GPA 0xE0000 (BIOS reserved area, within E820 gap)
- **Protocol constants**: COVERAGE_BITMAP_ADDR, COVERAGE_BITMAP_SIZE, COVERAGE_PORT (0x0511)
- **SDK coverage module**: `no_std` direct pointer + `std` mmap /dev/mem
- **SanCov hooks**: `__sanitizer_cov_trace_pc_guard` + `__sanitizer_cov_trace_pc_guard_init`
- **AFL edge hashing**: `prev_location XOR cur_location`, saturating 8-bit counters
- **VMM integration**: clear_coverage_bitmap() before each branch, read_coverage_bitmap() after
- **Explore wiring**: ExplorerConfig.coverage_gpa defaults to COVERAGE_BITMAP_ADDR, CoverageCollector reads via collect_from_guest()

## Integration Test Results (2026-02-18)
- **12/12 tests pass** with real kernel (vmlinux ELF, not bzImage)
- **Determinism**: Bounded runs (100K exits) produce identical exit counts + vTSC; serial content 99%+ match (1 line differs: PIT-calibrated TSC MHz varies due to KVM PIT using host time)
- **Snapshot/restore**: vTSC correctly saved/restored, serial content 100% match after restore
- **Fault injection**: ProcessKill, ClockSkew, NetworkPartition all confirmed working
- **Key fix**: Must call force_setup_complete() when guest doesn't use SDK, faults are gated by setup_complete flag
- **Kernel loading**: Must use vmlinux (ELF), not bzImage — ELF loader rejects bzImage magic

## SDK + Fault Injection (2026-02-18)
- **chaoscontrol-protocol**: Wire format crate, `no_std`, zero deps. Defines HypercallPage (4096 bytes, `repr(C, align(4096))`), command IDs, payload encode/decode
- **chaoscontrol-sdk**: Guest SDK crate, `no_std` default + `std` feature. Antithesis-style API: assert::{always,sometimes,reachable,unreachable}, lifecycle::{setup_complete,send_event}, random::{get_random,random_choice}
- **chaoscontrol-fault**: Host-side engine crate. FaultEngine (dispatch + scheduling + random generation), PropertyOracle (cross-run assertion tracking), FaultSchedule (time-ordered fault delivery)
- **Transport**: Shared memory page at `0x000F_E000` (E820 reserved gap) + `outb(0x0510)` trigger port
- **VMM integration**: SDK port (0x510) handled in step() IoIn/IoOut. handle_sdk_hypercall() reads page from guest memory, dispatches to FaultEngine, writes result back
- **Assertion ID**: FNV-1a hash of location string, computed at const time via location_id()
- **Faults gated by setup_complete**: No faults fire until guest calls lifecycle::setup_complete()
- **BTreeMap not HashMap** in oracle for determinism
- **Oracle borrow fix**: Must compute run_id BEFORE mutable borrow of self.assertions (Rust borrow checker)
