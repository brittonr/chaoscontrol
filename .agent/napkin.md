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
1. Fix kvm_exit BPF tracepoint (trace_entry struct alignment with vmlinux.h)
2. Add kvm_exit + kvm_inj_virq + kvm_set_irq capture (currently 0 events for these)
3. End-to-end integration test with real kernel (boot multi-VM, inject faults, verify exploration loop)
4. Wire fault engine effects into real VMs at runtime (controller has structure, needs integration test)
5. SDK coverage instrumentation (guest-side branch tracking to shared memory page)

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
