# Napkin

## Corrections
| Date | Source | What Went Wrong | What To Do Instead |
|------|--------|----------------|-------------------|
| 2026-02-17 | self | step() method takes `capture` param but callers don't pass it | Removed param; CapturingWriter already captures all serial output |
| 2026-02-17 | self | restore() passes io::stdout() but Serial expects CapturingWriter | Pass serial_writer.clone() to from_state |

## User Preferences
- Building a deterministic hypervisor (ChaosControl)
- Uses Rust + KVM via rust-vmm crates
- Nix flake for dev environment (must use `nix develop --command bash -c "..."`)
- Project structure: workspace with crates/ directory

## Patterns That Work
- vm_superio::Serial with EventFd + register_irqfd for interrupt-driven serial
- CapturingWriter pattern: write to stdout + capture in Arc<Mutex<Vec<u8>>>
- Identity-mapped 2MB pages for first 1GB via PML4→PDPT→PD

## Patterns That Don't Work
- (none yet)

## Domain Notes
- KVM requires: set_tss_address BEFORE create_irq_chip, create_irq_chip BEFORE create_vcpu
- Guest memory layout follows Firecracker conventions (HIMEM_START=0x100000)
- Design doc at docs/deterministic-vmm-design.md has full architecture
- Milestones: boot✅ → determinism controls → virtio devices → multi-VM simulation
