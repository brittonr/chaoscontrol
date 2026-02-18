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
- GuestMemoryManager wrapping GuestMemoryMmap for clean snapshot/restore
- `cargo clippy --fix --allow-dirty` for auto-fixing lint issues
- Faking Intel vendor + injecting CPUID 0x15 for deterministic TSC calibration on AMD hosts
- reset_time_state() right before first vcpu.run() — TSC=0, KVM clock=0, PIT snapshot/restore
- `clocksource=tsc tsc=reliable lpj=6000000` cmdline for deterministic clocks
- hide_hypervisor=true to prevent kvm-clock (host wall time dependent)

## Patterns That Don't Work
- Manual range checks (use RangeInclusive::contains instead per clippy)
- Redundant closures for enum variant constructors (clippy catches these)
- `no-kvmclock` is not a valid kernel parameter
- Setting fixed_family alone doesn't change vendor string (CPUID leaf 0x0 is separate)
- Filtering CPUID entries that don't exist on the host CPU — must inject them

## Domain Notes
- KVM requires: set_tss_address BEFORE create_irq_chip, create_irq_chip BEFORE create_vcpu
- Guest memory layout follows Firecracker conventions (HIMEM_START=0x100000)
- Design doc at docs/deterministic-vmm-design.md has full architecture
- Milestones: boot✅ → determinism controls✅ → device backends✅ → virtio transport → multi-VM
- Currently at 105 unit tests + 11 doc-tests, zero clippy warnings

## Determinism Status (2026-02-17)
- **DETERMINISTIC**: TSC frequency (exactly 3GHz via CPUID 0x15), BogoMIPS, clocksource, 
  heartbeat output, kernel boot messages (310+ of 315 lines identical)
- **NOT YET DETERMINISTIC**: 4 kernel lines affected by PIT interrupt delivery timing:
  - `tsc: Detected X MHz processor` (PIT measurement, harmless — overridden by CPUID 0x15)
  - `Memory: available` (varies by ~100KB, timing-dependent allocations)
  - `sched_clock: Marking stable` (internal clock stabilization values)
  - `audit(timestamp)` / `Uptime` (PIT-derived time values)
- **ROOT CAUSE**: PIT timer runs in KVM kernel, delivers interrupts based on host wall time
- **FIX**: Need to trap PIT/LAPIC timer interrupts and deliver at deterministic exit counts

## Next Steps
1. Trap PIT timer to deliver interrupts at deterministic guest execution points
2. Implement virtio MMIO transport layer
3. Build multi-VM simulation controller
