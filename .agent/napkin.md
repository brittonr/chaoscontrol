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
- **DETERMINISTIC**: 321 of 324 kernel lines, all application output
- **NOT DETERMINISTIC**: tsc calibration, Memory ±28KB, sched_clock, audit timestamp
- **ROOT CAUSE**: variable VM exit counts (host interrupts, serial polling) × fixed TSC advance

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

## Next Steps
1. Fix virtual TSC: advance based on guest execution time, not exit count
2. Fix kvm_exit BPF tracepoint (trace_entry struct alignment with vmlinux.h)
3. Add kvm_exit + kvm_inj_virq + kvm_set_irq capture (currently 0 events for these)
4. Implement virtio MMIO transport
5. Build multi-VM simulation controller
