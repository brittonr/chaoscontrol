# ChaosControl - Deterministic VMM

A deterministic Virtual Machine Monitor (VMM) for x86_64 built with KVM.

## Features

- **Deterministic execution**: CPUID filtering to disable RDRAND/RDSEED
- **Pinned TSC**: Fixed time stamp counter frequency for reproducible timing
- **x86_64 boot**: Full long mode setup with GDT, page tables, and boot parameters
- **In-kernel IRQ chip**: PIC, IOAPIC, and LAPIC support
- **Serial console**: COM1 serial port for kernel output
- **Linux kernel support**: Loads ELF kernels via linux-loader

## Project Structure

```
chaoscontrol/
├── flake.nix                          # Nix development environment
├── Cargo.toml                         # Workspace root
└── crates/
    └── chaoscontrol-vmm/              # VMM implementation
        ├── Cargo.toml
        └── src/
            ├── lib.rs                 # Library root
            ├── vm.rs                  # Core VM implementation
            ├── cpu.rs                 # CPU configuration (stub)
            ├── memory.rs              # Memory management (stub)
            ├── snapshot.rs            # Snapshot/restore (stub)
            └── devices/
                ├── mod.rs
                └── serial.rs          # Serial device (stub)
```

## Building

This project uses Nix flakes for reproducible builds:

```bash
# Enter development environment
nix develop

# Build the project
cargo build

# Check compilation
cargo check
```

## Implementation Details

### VM Setup (`vm.rs`)

The `DeterministicVm` implementation includes:

1. **KVM initialization**: Creates KVM instance, VM, and vCPU
2. **Guest memory**: Configurable memory size (default 128MB)
3. **Kernel loading**: Uses linux-loader to load ELF kernels at 1MB (HIMEM_START)
4. **x86_64 boot sequence**:
   - GDT setup with code/data/TSS segments for 64-bit mode
   - Identity-mapped page tables (PML4 → PDPT → PD) covering first 1GB
   - Special register configuration (CR0, CR3, CR4, EFER)
   - Boot parameters (zero page) at 0x7000
5. **Determinism features**:
   - CPUID filtering removes RDRAND (ECX bit 30) and RDSEED (EBX bit 18)
   - TSC pinned to 3.0 GHz
6. **Device support**:
   - In-kernel IRQ chip (PIC/IOAPIC/LAPIC)
   - PIT (Programmable Interval Timer)
   - Serial port (COM1 at 0x3f8) for console I/O

### Memory Layout

Based on Firecracker's x86_64 layout:

- `0x0000_0000 - 0x0000_0500`: Low memory / BIOS data
- `0x0000_0500`: GDT
- `0x0000_0520`: IDT
- `0x0000_7000`: Zero page (boot parameters)
- `0x0000_8ff0`: Boot stack pointer
- `0x0000_9000`: PML4 (page tables)
- `0x0000_a000`: PDPT
- `0x0000_b000`: PDE
- `0x0002_0000`: Kernel command line
- `0x0010_0000`: HIMEM_START (kernel load address, 1MB)

### Run Loop

The VM run loop handles:
- **IoIn/IoOut**: Serial port I/O at 0x3f8
- **Hlt**: VM halted (clean shutdown)
- **Shutdown**: VM shutdown event
- **MmioRead/MmioWrite**: Memory-mapped I/O (basic handling)

## Dependencies

Core dependencies:
- `kvm-ioctls`: KVM interface
- `kvm-bindings`: KVM structure bindings
- `vm-memory`: Guest memory management
- `linux-loader`: Kernel loading (ELF/bzImage)
- `vmm-sys-util`: VMM utilities

## Next Steps

Stub modules ready for implementation:
- [ ] `cpu.rs`: Advanced CPUID filtering, MSR traps
- [ ] `memory.rs`: Memory region management, ballooning
- [ ] `devices/serial.rs`: Full 16550 UART emulation
- [ ] `snapshot.rs`: VM state snapshot/restore for record/replay

## License

Apache-2.0
