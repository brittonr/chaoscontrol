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

## User Preferences
- Building a deterministic hypervisor (ChaosControl)
- Uses Rust + KVM via rust-vmm crates
- Nix flake for dev environment (must use `nix develop --command bash -c "..."`)
- NixOS host (sudoers via security.sudo.extraRules, not /etc/sudoers.d/)
- bpftrace NOPASSWD via NixOS config

## Patterns That Work
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

## Completed (2026-02-18 session)
1. ✅ Fix virtual TSC: sync_tsc_to_guest() writes virtual TSC to IA32_TSC MSR before every vcpu.run()
2. ✅ Multi-VM SimulationController (round-robin scheduling, NetworkFabric, fault dispatch)
3. ✅ Virtio MMIO transport (virtio 1.2, legacy-free) + virtio-blk, virtio-net, virtio-rng backends
4. ✅ chaoscontrol-explore crate: fork-from-snapshot, coverage-guided search, AFL-style bitmaps, frontier, mutator
5. ✅ chaoscontrol-replay crate: recording, checkpoint, replay, time-travel debugger, triage, serialize

## Remaining Work
(All items completed)

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
