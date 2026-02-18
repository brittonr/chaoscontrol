# Napkin

## Corrections
| Date | Source | What Went Wrong | What To Do Instead |
|------|--------|----------------|-------------------|
| 2026-02-17 | self | step() method took `capture` param but callers didn't pass it | Removed param; CapturingWriter already captures all serial output |
| 2026-02-17 | self | restore() passed io::stdout() but Serial expects CapturingWriter | Pass serial_writer.clone() to from_state |
| 2026-02-17 | self | Duplicated GDT/memory/CPUID code in vm.rs | Extracted to memory.rs and cpu.rs modules |
| 2026-02-17 | self | Tried `no-kvmclock` kernel param — not a real param | Use KVM visibility control + clocksource=tsc instead |
| 2026-02-17 | self | Set fixed_family=6 (Intel) but forgot vendor string is separate from family | Must also set CPUID leaf 0x0 vendor string to "GenuineIntel" |
| 2026-02-17 | self | AMD CPUs don't have CPUID leaf 0x15, so filter_entry never matched | Must INJECT leaf 0x15 into CPUID entries if missing, not just filter existing |
| 2026-02-17 | self | Faked Intel family but forgot to bump max leaf (EAX of leaf 0x0) | Must set leaf 0x0 EAX >= 0x15 so kernel knows leaf 0x15 exists |
| 2026-02-17 | self | Tried removing create_pit2() entirely | KVM PIT provides essential IRQ routing + port 0x61 handling; must keep it but suppress its timer |
| 2026-02-17 | self | DeterministicPit never received guest PIT writes | KVM PIT intercepts port 0x40-0x43; must read back KVM PIT state via get_pit2() and mirror it |
| 2026-02-17 | self | PIT test used truncating division for TSC-per-period | Must use div_ceil() to avoid off-by-one in reverse PIT tick computation |

## User Preferences
- Building a deterministic hypervisor (ChaosControl)
- Uses Rust + KVM via rust-vmm crates
- Nix flake for dev environment (must use `nix develop --command bash -c "..."`)
- Project structure: workspace with crates/ directory

## Patterns That Work
- vm_superio::Serial with EventFd + register_irqfd for interrupt-driven serial
- CapturingWriter pattern: write to stdout + capture in Arc<Mutex<Vec<u8>>>
- VirtualTsc advancing on every VM exit for deterministic time progression
- Comprehensive CPUID filtering across leaves 0x0, 0x1, 0x7, 0xB, 0x15, 0x16, 0x80000001, 0x80000007
- Faking Intel vendor + injecting CPUID 0x15 for deterministic TSC calibration on AMD hosts
- hide_hypervisor=true to prevent kvm-clock (host wall time dependent)
- **Hybrid PIT**: keep KVM PIT for I/O handling, suppress its timer via count_load_time=far_future,
  mirror state to DeterministicPit, deliver IRQ 0 ourselves via set_irq_line
- **HLT fast-forward**: on HLT exit, read KVM PIT reload value, advance virtual TSC by one period,
  inject IRQ 0, continue — this eliminates host-time-dependent HLT wake timing
- get_pit2() + set_pit2() to read/suppress KVM PIT state on every exit

## Patterns That Don't Work
- Manual range checks (use RangeInclusive::contains instead per clippy)
- Redundant closures for enum variant constructors (clippy catches these)
- `no-kvmclock` is not a valid kernel parameter
- Setting fixed_family alone doesn't change vendor string (CPUID leaf 0x0 is separate)
- Filtering CPUID entries that don't exist on the host CPU — must inject them
- Removing create_pit2() entirely — breaks kernel boot at serial driver init
- DeterministicPit as sole PIT (no KVM PIT) — KVM needs PIT for port 0x61 and IRQ routing

## Domain Notes
- KVM requires: set_tss_address BEFORE create_irq_chip, create_irq_chip BEFORE create_vcpu
- Guest memory layout follows Firecracker conventions (HIMEM_START=0x100000)
- Design doc at docs/deterministic-vmm-design.md has full architecture
- Currently at 116 unit tests + 12 doc-tests, zero clippy warnings

## Determinism Status (2026-02-17)
- **DETERMINISTIC**: TSC frequency (exactly 3GHz via CPUID 0x15), BogoMIPS, clocksource,
  heartbeat output, application-level behavior, 321 of 324 kernel lines identical
- **NOT YET DETERMINISTIC**: 3 kernel-internal lines:
  - `tsc: Detected X MHz processor` (PIT calibration via RDTSC vs real PIT, harmless)
  - `Memory: available` (±28KB, down from ±100KB — much improved)
  - `sched_clock: Marking stable` (internal clock stabilization values)
- **ROOT CAUSE**: PIT calibration uses RDTSC (real time) vs PIT countdown (host time).
  The timer interrupt jitter is now minimized by our hybrid PIT approach.
- **APPROACH**: Hybrid — KVM PIT for I/O/routing, suppress timer, mirror to DeterministicPit,
  inject IRQ 0 at virtual-time points, HLT fast-forward

## Next Steps
1. Add bpftrace/eBPF integration for KVM tracepoint monitoring (user has root)
2. Implement virtio MMIO transport layer
3. Build multi-VM simulation controller
