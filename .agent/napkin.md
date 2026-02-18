# Napkin

## Corrections
| Date | Source | What Went Wrong | What To Do Instead |
|------|--------|----------------|-------------------|
| 2026-02-17 | self | step() method took `capture` param but callers didn't pass it | Removed param; CapturingWriter already captures all serial output |
| 2026-02-17 | self | restore() passed io::stdout() but Serial expects CapturingWriter | Pass serial_writer.clone() to from_state |
| 2026-02-17 | self | Duplicated GDT/memory/CPUID code in vm.rs | Extracted to memory.rs and cpu.rs modules |

## User Preferences
- Building a deterministic hypervisor (ChaosControl)
- Uses Rust + KVM via rust-vmm crates
- Nix flake for dev environment (must use `nix develop --command bash -c "..."`)
- Project structure: workspace with crates/ directory

## Patterns That Work
- vm_superio::Serial with EventFd + register_irqfd for interrupt-driven serial
- CapturingWriter pattern: write to stdout + capture in Arc<Mutex<Vec<u8>>>
- VirtualTsc advancing on every VM exit for deterministic time progression
- Comprehensive CPUID filtering across leaves 0x1, 0x7, 0x15, 0x16, 0x80000001, 0x80000007
- GuestMemoryManager wrapping GuestMemoryMmap for clean snapshot/restore
- `cargo clippy --fix --allow-dirty` for auto-fixing lint issues

## Patterns That Don't Work
- Manual range checks (use RangeInclusive::contains instead per clippy)
- Redundant closures for enum variant constructors (clippy catches these)

## Domain Notes
- KVM requires: set_tss_address BEFORE create_irq_chip, create_irq_chip BEFORE create_vcpu
- Guest memory layout follows Firecracker conventions (HIMEM_START=0x100000)
- Design doc at docs/deterministic-vmm-design.md has full architecture
- Milestones: boot✅ → determinism controls✅ → device backends✅ → virtio transport → multi-VM
- Currently at 105 unit tests + 11 doc-tests, zero clippy warnings
- Next: virtio MMIO transport, wire devices into run loop, multi-VM simulation
